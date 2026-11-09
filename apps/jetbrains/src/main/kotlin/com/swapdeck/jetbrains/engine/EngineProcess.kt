package com.swapdeck.jetbrains.engine

import com.swapdeck.jetbrains.SwapDeckThreads
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.nio.file.Path
import java.util.concurrent.ExecutorService
import java.util.concurrent.Future
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
 * [Events.onUnexpectedExit] o [Events.onLockBusy], mai dopo un [stop].
 */
class EngineProcess(
    private val events: Events,
    private val executor: ExecutorService = SwapDeckThreads.io,
) {
    interface Events {
        fun onLine(line: String)

        /** Uscita non chiesta (errore, config non valida, avvio fallito: `null`). */
        fun onUnexpectedExit(code: Int?)

        /** Il lock è di un'altra istanza: chi ascolta resta in attesa e riprova. */
        fun onLockBusy()
    }

    private class Run(val process: Process) {
        @Volatile
        var stopping = false
    }

    private var current: Run? = null

    val running: Boolean get() = synchronized(this) { current != null }

    val pid: Long? get() = synchronized(this) { current?.process?.pid() }

    /** Avvia il motore se non è già acceso. */
    fun start(cliPath: Path, env: Map<String, String>) {
        val failure = synchronized(this) {
            if (current != null) return
            val builder = ProcessBuilder(listOf(cliPath.toString()) + EngineDefaults.ARGS)
            builder.environment().apply {
                clear()
                putAll(env)
            }
            try {
                val run = Run(builder.start())
                current = run
                val out = executor.submit { pipeLines(run.process.inputStream) }
                val err = executor.submit { pipeLines(run.process.errorStream) }
                executor.execute { waitExit(run, out, err) }
                null
            } catch (e: IOException) {
                "swapdeck-cli headless: ${e.message}"
            }
        } ?: return
        events.onLine(failure)
        events.onUnexpectedExit(null)
    }

    private fun pipeLines(stream: InputStream) {
        val splitter = LineSplitter(events::onLine)
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
        if (code == EngineDefaults.LOCK_BUSY_EXIT) events.onLockBusy() else events.onUnexpectedExit(code)
    }

    /**
     * Chiude stdin (il runner rilascia il lock ed esce), attende [graceMs], poi termina
     * (`destroy`, 1 s, `destroyForcibly`). Bloccante: da un thread in background.
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
                if (!process.waitFor(EngineDefaults.KILL_WAIT_MS, TimeUnit.MILLISECONDS)) process.destroyForcibly()
            }
        } catch (_: InterruptedException) {
            process.destroyForcibly()
            Thread.currentThread().interrupt()
        }
        synchronized(this) { if (current === run) current = null }
    }
}
