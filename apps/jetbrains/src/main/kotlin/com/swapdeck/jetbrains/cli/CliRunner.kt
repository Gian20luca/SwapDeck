package com.swapdeck.jetbrains.cli

import com.swapdeck.jetbrains.SwapDeckThreads
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.io.OutputStream
import java.nio.file.Files
import java.nio.file.Path
import java.util.concurrent.CancellationException
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ExecutorService
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Future
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

/** Ambiente dei processi `swapdeck-cli`: quello dell'IDE più `SWAPDECK_LOG_SOURCE`. */
object CliEnv {
    const val LOG_SOURCE_VAR = "SWAPDECK_LOG_SOURCE"

    /** Nome con cui la CLI lanciata dal plugin firma le sue righe nel log di SwapDeck. */
    const val LOG_SOURCE = "jetbrains"

    fun of(base: Map<String, String>, extra: Map<String, String> = emptyMap()): Map<String, String> =
        base + extra + (LOG_SOURCE_VAR to LOG_SOURCE)
}

/** Limiti dell'output letto (lo storico di 7 giorni con molti account resta sotto). */
object CliLimits {
    const val MAX_STDOUT_BYTES: Int = 64 * 1024 * 1024
    const val STDERR_TAIL_BYTES: Int = 4096

    /** Coda di stderr riportata nel messaggio di una risposta non JSON. */
    const val STDERR_IN_MESSAGE: Int = 500
}

/**
 * Lancia `swapdeck-cli` senza shell: argomenti come lista, input JSON solo su stdin, timeout con
 * terminazione (`destroy`, attesa, `destroyForcibly`), limite di stdout e coda di stderr.
 *
 * Thread: ogni comando usa thread di [executor] per stdin, stdout, stderr e l'attesa dell'uscita;
 * il future si completa da uno di quelli. Callback (`onEvent`) dal thread che legge stdout, `channel`
 * da un thread di [executor] dopo l'eventuale prima riga di stdin. Chi lancia o annulla un comando
 * non si blocca mai sulla scrittura di stdin.
 *
 * [close] (servizio smaltito) annulla i comandi in corso, che terminano i processi; dopo, ogni
 * comando fallisce con `cancelled` senza partire.
 */
class CliRunner(
    private val executable: () -> Path,
    private val baseEnv: () -> Map<String, String> = { System.getenv() },
    private val executor: ExecutorService = SwapDeckThreads.io,
    private val scheduler: ScheduledExecutorService = SwapDeckThreads.scheduler,
    private val killGraceMs: Long = CliTimeouts.KILL_GRACE_MS,
    private val maxStdoutBytes: Int = CliLimits.MAX_STDOUT_BYTES,
) : CliCaller, AutoCloseable {
    private val active: MutableSet<CompletableFuture<JsonElement>> = ConcurrentHashMap.newKeySet()

    @Volatile
    private var closed = false

    override fun run(args: List<String>, options: CliOptions): CompletableFuture<JsonElement> {
        if (closed) return CompletableFuture.failedFuture(closedError())
        val file = executable()
        val result = CompletableFuture<JsonElement>()
        val builder = ProcessBuilder(listOf(file.toString()) + args)
        builder.environment().apply {
            clear()
            putAll(CliEnv.of(baseEnv(), options.env))
        }
        val process = try {
            builder.start()
        } catch (e: IOException) {
            result.completeExceptionally(startError(file, e))
            return result
        }
        active += result
        result.whenComplete { _, _ -> active -= result }
        Call(process, args, options, result).begin()
        // Chiuso mentre partiva: il comando non resta vivo.
        if (closed) result.cancel(true)
        return result
    }

    /** Annulla i comandi in corso (processi terminati) e rifiuta i successivi. */
    override fun close() {
        closed = true
        active.toList().forEach { it.cancel(true) }
    }

    private fun closedError() = CliException("cancelled", "swapdeck-cli: plugin chiuso")

    private fun startError(file: Path, e: IOException): CliException {
        val missing = !Files.exists(file) || e.message?.contains("error=2,") == true
        return if (missing) {
            CliNotFoundException("swapdeck-cli non trovato: $file")
        } else {
            CliException("failed", "swapdeck-cli non avviato: ${e.message}")
        }
    }

    private inner class Call(
        private val process: Process,
        private val args: List<String>,
        private val options: CliOptions,
        private val result: CompletableFuture<JsonElement>,
    ) {
        private val stdout = ByteArrayOutputStream()
        private val stderrTail = TailBuffer(CliLimits.STDERR_TAIL_BYTES)
        /** Solo per [timers]: mai preso durante una scrittura su stdin. */
        private val lock = Any()

        /** Righe intere su stdin, una alla volta (può bloccarsi finché il processo legge). */
        private val stdinLock = Any()
        private var timers = mutableListOf<ScheduledFuture<*>>()

        @Volatile
        private var expired = false

        @Volatile
        private var channelOpen = true

        fun begin() {
            // Annullato dal chiamante, risposta troppo grande, timeout: il processo non resta vivo.
            result.whenComplete { _, error ->
                channelOpen = false
                // Prima il processo: una scrittura bloccata su stdin si sblocca quando muore.
                if (error is CancellationException || process.isAlive) terminate()
                cancelTimers()
            }
            try {
                start()
            } catch (_: RejectedExecutionException) {
                // Pool già chiusi (plugin in chiusura).
                process.destroyForcibly()
                result.completeExceptionally(closedError())
            }
        }

        private fun start() {
            schedule(options.timeoutMs) {
                expired = true
                process.destroy()
                schedule(killGraceMs) {
                    process.destroyForcibly()
                    schedule(killGraceMs) { result.completeExceptionally(timeoutError()) }
                }
            }
            val out = executor.submit { readStdout(process.inputStream) }
            val err = executor.submit { readStderr(process.errorStream) }
            executor.execute { waitExit(out, err) }
            val channel = options.channel
            if (channel == null) {
                executor.execute { writeAndClose(process.outputStream, options.stdin) }
            } else {
                val stdin = process.outputStream
                // Fuori dal thread del chiamante: la prima riga può bloccarsi se il processo non legge.
                executor.execute {
                    options.stdin?.let { writeLine(stdin, it) }
                    channel(
                        object : StdinChannel {
                            override fun write(text: String): Boolean = channelOpen && writeLine(stdin, text)

                            override fun end() {
                                if (channelOpen) closeQuietly(stdin)
                            }
                        },
                    )
                }
            }
        }

        private fun waitExit(out: Future<*>, err: Future<*>) {
            val code = try {
                process.waitFor()
            } catch (_: InterruptedException) {
                return
            }
            // Come `close` di Node: l'esito dopo la fine di stdout e stderr.
            runCatching { out.get() }
            runCatching { err.get() }
            channelOpen = false
            if (expired) {
                result.completeExceptionally(timeoutError())
                return
            }
            if (result.isDone) return
            try {
                val text = synchronized(stdout) { stdout.toString(Charsets.UTF_8) }
                result.complete(parseCliResult(code, if (options.onEvent != null) lastLine(text) else text, stderrTail.text()))
            } catch (e: CliException) {
                result.completeExceptionally(e)
            }
        }

        private fun readStdout(stream: InputStream) {
            val events = options.onEvent?.let { EventLines(it) }
            val buffer = ByteArray(64 * 1024)
            var size = 0L
            stream.use {
                while (true) {
                    val n = try {
                        stream.read(buffer)
                    } catch (_: IOException) {
                        -1
                    }
                    if (n < 0) break
                    size += n
                    if (size > maxStdoutBytes) {
                        result.completeExceptionally(CliException("failed", "swapdeck-cli: risposta troppo grande"))
                        break
                    }
                    synchronized(stdout) { stdout.write(buffer, 0, n) }
                    events?.push(buffer, n)
                }
            }
        }

        private fun readStderr(stream: InputStream) {
            val buffer = ByteArray(8 * 1024)
            stream.use {
                while (true) {
                    val n = try {
                        stream.read(buffer)
                    } catch (_: IOException) {
                        -1
                    }
                    if (n < 0) break
                    stderrTail.append(buffer, n)
                }
            }
        }

        private fun writeAndClose(stdin: OutputStream, text: String?) {
            // Il processo può chiudere stdin prima di leggerlo (errore d'uso): l'esito arriva dall'uscita.
            try {
                if (text != null) stdin.write(text.toByteArray(Charsets.UTF_8))
            } catch (_: IOException) {
            } finally {
                closeQuietly(stdin)
            }
        }

        private fun writeLine(stdin: OutputStream, text: String): Boolean = synchronized(stdinLock) {
            try {
                stdin.write(text.toByteArray(Charsets.UTF_8))
                stdin.flush()
                true
            } catch (_: IOException) {
                false
            }
        }

        private fun terminate() {
            if (!process.isAlive) return
            process.destroy()
            try {
                scheduler.schedule({ if (process.isAlive) process.destroyForcibly() }, killGraceMs, TimeUnit.MILLISECONDS)
            } catch (_: RejectedExecutionException) {
                process.destroyForcibly()
            }
        }

        private fun schedule(delayMs: Long, task: () -> Unit) {
            synchronized(lock) {
                if (result.isDone) return
                try {
                    timers += scheduler.schedule(task, delayMs, TimeUnit.MILLISECONDS)
                } catch (_: RejectedExecutionException) {
                    // Pool chiusi: lo smaltimento annulla comunque il comando.
                }
            }
        }

        private fun cancelTimers() {
            val pending = synchronized(lock) { timers.also { timers = mutableListOf() } }
            // Il timer che ha completato il future è già partito: `cancel` non lo tocca.
            pending.forEach { it.cancel(false) }
        }

        private fun timeoutError() =
            CliException("timeout", "swapdeck-cli ${args.firstOrNull() ?: ""}: nessuna risposta in tempo")
    }

    companion object {
        private fun closeQuietly(stream: OutputStream) {
            try {
                stream.close()
            } catch (_: IOException) {
            }
        }
    }
}

private val lenient = Json { ignoreUnknownKeys = true }

/** Esito di un comando dal codice di uscita e da stdout (busta `{"error": …}` se il codice non è 0). */
fun parseCliResult(code: Int, stdout: String, stderr: String = ""): JsonElement {
    val value = try {
        lenient.parseToJsonElement(stdout.trim())
    } catch (_: Exception) {
        val tail = stderr.trim().takeLast(CliLimits.STDERR_IN_MESSAGE)
        throw CliException(
            "failed",
            "swapdeck-cli: risposta non JSON (uscita $code)${if (tail.isNotEmpty()) ": $tail" else ""}",
        )
    }
    if (code == 0) return value
    val error = CswapError.parse((value as? JsonObject)?.get("error"))
    throw if (error != null) CliException(error) else CliException("failed", "swapdeck-cli: uscita $code")
}

/** Ultima riga non vuota (l'esito dei comandi con eventi). */
fun lastLine(text: String): String = text.split('\n').lastOrNull { it.isNotBlank() } ?: ""

/** Spezza stdout in righe JSON e passa quelle con `event` stringa; le altre si ignorano. */
internal class EventLines(private val onEvent: (JsonObject) -> Unit) {
    private val pending = ByteArrayOutputStream()

    fun push(bytes: ByteArray, length: Int) {
        for (i in 0 until length) {
            val b = bytes[i]
            if (b == '\n'.code.toByte()) {
                emit(pending.toString(Charsets.UTF_8))
                pending.reset()
            } else {
                pending.write(b.toInt())
            }
        }
    }

    private fun emit(line: String) {
        val value = try {
            lenient.parseToJsonElement(line) as? JsonObject
        } catch (_: Exception) {
            null
        } ?: return
        if (value.string("event") == null) return
        try {
            onEvent(value)
        } catch (_: Exception) {
            // Un errore di chi ascolta non ferma la lettura.
        }
    }
}

/** Ultimi [capacity] byte di uno stream (stderr). */
internal class TailBuffer(private val capacity: Int) {
    private var data = ByteArray(0)

    @Synchronized
    fun append(bytes: ByteArray, length: Int) {
        val joined = data + bytes.copyOf(length)
        data = if (joined.size > capacity) joined.copyOfRange(joined.size - capacity, joined.size) else joined
    }

    @Synchronized
    fun text(): String = data.toString(Charsets.UTF_8)
}
