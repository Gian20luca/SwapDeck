package com.swapdeck.jetbrains.engine

import com.swapdeck.jetbrains.SwapDeckThreads
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.nio.file.Path
import java.util.concurrent.ExecutorService
import java.util.concurrent.Future
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit

/** Costanti del motore come processo figlio. */
object EngineDefaults {
    /** Attesa dell'uscita pulita (stdin chiuso): il giro in corso (conflitti, cswap) va finito. */
    const val STOP_GRACE_MS: Long = 15_000

    /** Alla chiusura dell'IDE pochi secondi: un lock lasciato da un PID morto scade subito. */
    const val DISPOSE_GRACE_MS: Long = 4_000

    /** Dopo il `destroy` (SIGTERM): attesa prima del kill. */
    const val KILL_WAIT_MS: Long = 1_000

    /** `headless::exit_code::LOCK_BUSY` del core: il motore gira già altrove. */
    const val LOCK_BUSY_EXIT = 3

    /** Riga più lunga inoltrata (il resto si scarta). */
    const val MAX_LINE_BYTES = 4096

    /** Argomenti: `--notify=stdout` evita il doppione con le notifiche di sistema. */
    val ARGS: List<String> = listOf("headless", "--stop-on-stdin-eof", "--notify=stdout")
}

/** Spezza uno stream in righe intere (UTF-8 anche a cavallo dei blocchi), righe vuote escluse. */
class LineSplitter(
    private val onLine: (String) -> Unit,
    private val maxBytes: Int = EngineDefaults.MAX_LINE_BYTES,
) {
    private val pending = ByteArrayOutputStream()
    private var truncated = false

    fun push(bytes: ByteArray, length: Int = bytes.size) {
        for (i in 0 until length) {
            val b = bytes[i]
            if (b == '\n'.code.toByte()) {
                emit()
            } else if (pending.size() < maxBytes) {
                pending.write(b.toInt())
            } else {
                truncated = true
            }
        }
    }

    fun end() = emit()

    private fun emit() {
        var line = pending.toString(Charsets.UTF_8).removeSuffix("\r")
        if (truncated) line += "…"
        pending.reset()
        truncated = false
        if (line.isNotBlank()) onLine(line)
    }
}

/**
 * Motore come processo figlio: `swapdeck-cli headless --stop-on-stdin-eof --notify=stdout`
 * (rispetta il lock). Righe di stdout e stderr a [Events.onLine] dai thread che leggono; uscita a
 * [Events.onUnexpectedExit] o [Events.onLockBusy], mai dopo un [stop]. Ogni avvio porta l'identità
 * `run` data da chi chiama [start], ripetuta negli eventi d'uscita: chi ascolta scarta quelli di un
 * avvio che non è più l'ultimo.
 *
 * Gli eventi non arrivano mai dal thread che chiama [start] (anche l'avvio fallito passa da
 * [executor]): chi chiama può tenere un proprio lock.
 */
class EngineProcess(
    private val events: Events,
    private val executor: ExecutorService = SwapDeckThreads.io,
    private val starter: (ProcessBuilder) -> Process = { it.start() },
) {
    interface Events {
        fun onLine(line: String)

        /** Uscita non chiesta dell'avvio [run] (errore, config non valida, avvio fallito: `null`). */
        fun onUnexpectedExit(run: Long, code: Int?)

        /** Il lock è di un'altra istanza (avvio [run]): chi ascolta resta in attesa e riprova. */
        fun onLockBusy(run: Long)
    }

    private class Run(val id: Long, val process: Process) {
        @Volatile
        var stopping = false
    }

    private var current: Run? = null

    val running: Boolean get() = synchronized(this) { current != null }

    val pid: Long? get() = synchronized(this) { current?.process?.pid() }

    /** Avvia il motore (avvio [run]) se non è già acceso. */
    fun start(cliPath: Path, env: Map<String, String>, run: Long = 0) {
        val failure = synchronized(this) {
            if (current != null) return
            val builder = ProcessBuilder(listOf(cliPath.toString()) + EngineDefaults.ARGS)
            builder.environment().apply {
                clear()
                putAll(env)
            }
            val started = try {
                Run(run, starter(builder))
            } catch (e: IOException) {
                return@synchronized "swapdeck-cli headless: ${e.message}"
            }
            current = started
            try {
                val out = executor.submit { pipeLines(started.process.inputStream) }
                val err = executor.submit { pipeLines(started.process.errorStream) }
                executor.execute { waitExit(started, out, err) }
            } catch (_: RejectedExecutionException) {
                // Pool chiusi (plugin in chiusura): niente motore.
                started.process.destroyForcibly()
                current = null
                return
            }
            null
        } ?: return
        try {
            executor.execute {
                deliver(failure)
                events.onUnexpectedExit(run, null)
            }
        } catch (_: RejectedExecutionException) {
        }
    }

    /** Una riga a chi ascolta: un suo errore non ferma la lettura (le righe dopo arrivano lo stesso). */
    private fun deliver(line: String) {
        try {
            events.onLine(line)
        } catch (_: Exception) {
            // Chi ascolta registra i propri errori; qui conta solo continuare a leggere.
        }
    }

    private fun pipeLines(stream: InputStream) {
        val splitter = LineSplitter(::deliver)
        val buffer = ByteArray(8 * 1024)
        stream.use {
            while (true) {
                val n = try {
                    stream.read(buffer)
                } catch (_: IOException) {
                    -1
                }
                if (n < 0) break
                splitter.push(buffer, n)
            }
        }
        splitter.end()
    }

    private fun waitExit(run: Run, out: Future<*>, err: Future<*>) {
        val code = try {
            run.process.waitFor()
        } catch (_: InterruptedException) {
            return
        }
        // Tutte le righe prima dell'esito.
        runCatching { out.get(2, TimeUnit.SECONDS) }
        runCatching { err.get(2, TimeUnit.SECONDS) }
        synchronized(this) { if (current === run) current = null }
        if (run.stopping) return
        if (code == EngineDefaults.LOCK_BUSY_EXIT) events.onLockBusy(run.id) else events.onUnexpectedExit(run.id, code)
    }

    /**
     * Chiude stdin (il runner rilascia il lock ed esce), attende [graceMs], poi termina
     * (`destroy`, 1 s, `destroyForcibly` e attesa breve dell'uscita). Il motore risulta spento solo
     * dopo: un nuovo avvio non convive con il vecchio. Bloccante: da un thread in background.
     */
    fun stop(graceMs: Long = EngineDefaults.STOP_GRACE_MS) {
        val run = synchronized(this) { current ?: return }.also { it.stopping = true }
        val process = run.process
        try {
            process.outputStream.close()
        } catch (_: IOException) {
        }
        try {
            if (!process.waitFor(graceMs, TimeUnit.MILLISECONDS)) {
                process.destroy()
                if (!process.waitFor(EngineDefaults.KILL_WAIT_MS, TimeUnit.MILLISECONDS)) {
                    process.destroyForcibly()
                    awaitExit(process, EngineDefaults.KILL_WAIT_MS)
                }
            }
        } catch (_: InterruptedException) {
            process.destroyForcibly()
            awaitExit(process, EngineDefaults.KILL_WAIT_MS)
            Thread.currentThread().interrupt()
        }
        synchronized(this) { if (current === run) current = null }
    }

    companion object {
        /** Attende l'uscita al massimo [ms] anche se il thread viene interrotto (lo stato d'interruzione resta). */
        private fun awaitExit(process: Process, ms: Long) {
            val end = System.nanoTime() + TimeUnit.MILLISECONDS.toNanos(ms)
            var interrupted = Thread.interrupted()
            while (process.isAlive) {
                val left = end - System.nanoTime()
                if (left <= 0) break
                try {
                    process.waitFor(left, TimeUnit.NANOSECONDS)
                } catch (_: InterruptedException) {
                    interrupted = true
                }
            }
            if (interrupted) Thread.currentThread().interrupt()
        }
    }
}
