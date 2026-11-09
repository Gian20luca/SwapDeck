package com.swapdeck.jetbrains.engine

import com.swapdeck.jetbrains.SwapDeckThreads
import java.nio.file.Path
import java.util.concurrent.CompletableFuture
import java.util.concurrent.Executor
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.atomic.AtomicBoolean

/** Cosa succede al motore; chiamate dai thread del plugin (mai dall'EDT). */
interface EngineControllerEvents {
    /** Riga per il log dell'IDE (le righe della CLI sono già redatte). */
    fun log(message: String)

    fun warn(message: String)

    /** Notifica dal motore titolare del lock. */
    fun notification(notification: SwapDeckNotification)

    /** Uscita non chiesta: una sola volta per avvio. */
    fun unexpectedExit(code: Int?)

    /** Acceso, spento, in attesa del lock o PID cambiati. */
    fun changed()
}

/**
 * Pausa del motore ([EngineController.pause]): [stopped] si completa a motore fermo, [resume] la
 * toglie. La pausa vale da subito: chi la chiede chiama [resume] in un `finally` anche se l'attesa di
 * [stopped] fallisce o viene annullata. Chiamare [resume] più volte non cambia nulla.
 */
class EnginePause(
    val stopped: CompletableFuture<Unit>,
    private val onResume: () -> Unit,
) : AutoCloseable {
    private val resumed = AtomicBoolean(false)

    val isResumed: Boolean get() = resumed.get()

    fun resume() {
        if (resumed.compareAndSet(false, true)) onResume()
    }

    override fun close() = resume()
}

/**
 * Logica del motore (porting della parte motore dell'estensione VS Code), senza piattaforma:
 * segue l'impostazione [enabled], attende in silenzio se il lock è altrui (uscita 3) e riprova
 * quando uno stato dice che il lock è libero, avvisa una volta per avvio di un'uscita inattesa,
 * si mette in pausa per l'aggiornamento di cswap e il «Rifai login».
 *
 * Thread: i metodi si possono chiamare da qualunque thread; gli arresti (bloccanti fino a
 * [stopGraceMs]) girano su [executor] e i future si completano da lì. Gli [events] partono sempre
 * dopo aver lasciato il lock interno (chi ascolta può richiamare il controller o aspettare altri
 * thread che lo fanno). Ogni avvio ha un numero: gli eventi d'uscita di un avvio superato si scartano.
 */
class EngineController(
    private val enabled: () -> Boolean,
    private val cliPath: () -> Path,
    private val env: () -> Map<String, String>,
    private val events: EngineControllerEvents,
    private val executor: Executor = SwapDeckThreads.io,
    private val stopGraceMs: Long = EngineDefaults.STOP_GRACE_MS,
    processFactory: (EngineProcess.Events) -> EngineProcess = { EngineProcess(it) },
) {
    private val lock = Any()
    private val process = processFactory(ProcessEvents())
    private var warned = false
    private var pauses = 0
    private var transition: CompletableFuture<Unit>? = null
    private var disposed = false

    /** Numero dell'ultimo avvio (vedi [EngineProcess.Events]). */
    private var generation = 0L

    /** Il lock è di un'altra istanza (finestra VS Code, altra IDE, app): si ritenta quando si libera. */
    @Volatile
    var waitingLock: Boolean = false
        private set

    val running: Boolean get() = process.running

    /** PID del motore avviato qui, se acceso. */
    val pid: Long? get() = process.pid

    val paused: Boolean get() = synchronized(lock) { pauses > 0 }

    /**
     * Allinea il processo all'impostazione: avvia se voluto e spento, ferma se acceso e non voluto.
     * Durante un arresto in corso aspetta la fine; in pausa o dopo [dispose] non fa nulla.
     */
    fun sync(): CompletableFuture<Unit> {
        val after = mutableListOf<() -> Unit>()
        // Arresto da attendere prima di riallineare; `null` = niente da attendere.
        var waitFor: CompletableFuture<Unit>? = null
        val idle = synchronized(lock) {
            val pending = transition
            when {
                pending != null -> waitFor = pending
                disposed || pauses > 0 -> return done()
                else -> {
                    val wanted = enabled()
                    when {
                        wanted && !process.running -> {
                            warned = false
                            waitingLock = false
                            val path = cliPath()
                            after += { events.log("[motore] avvio: $path headless") }
                            process.start(path, env(), ++generation)
                        }
                        !wanted && process.running -> {
                            after += { events.log("[motore] arresto") }
                            waitFor = stopAsync(stopGraceMs)
                        }
                        !wanted -> waitingLock = false
                    }
                }
            }
            waitFor == null
        }
        after.forEach { it() }
        if (!idle) return waitFor!!.thenCompose { sync() }
        events.changed()
        return done()
    }

    /** Dopo un cambio di `cliPath`: riavvio con il nuovo binario (se acceso). */
    fun restart(): CompletableFuture<Unit> {
        val stopping = synchronized(lock) {
            if (process.running && !disposed) stopAsync(stopGraceMs) else null
        }
        return stopping?.thenCompose { sync() } ?: sync()
    }

    /**
     * Ad ogni stato letto: se il motore è voluto, non attivo qui, in attesa del lock e il lock ora è
     * libero ([lockHeldElsewhere] `false`; `null` = non noto), nuovo avvio.
     */
    fun onLockState(lockHeldElsewhere: Boolean?) {
        val retry = synchronized(lock) {
            pauses == 0 && !disposed && shouldRetry(enabled(), process.running, waitingLock, lockHeldElsewhere)
        }
        if (retry) {
            events.log("[motore] lock libero: nuovo avvio")
            sync()
        }
    }

    /**
     * Pausa per l'aggiornamento di cswap o il «Rifai login»: ferma il motore se acceso e impedisce
     * avvii e nuovi tentativi finché ogni pausa non è ripresa. La pausa vale subito; il suo
     * [EnginePause.stopped] si completa a motore fermo. Dopo [dispose] è una pausa vuota.
     */
    fun pause(why: String): EnginePause {
        var log = false
        val stopped = synchronized(lock) {
            if (disposed) return EnginePause(done()) {}
            pauses++
            if (process.running) {
                log = true
                stopAsync(stopGraceMs)
            } else {
                transition ?: done()
            }
        }
        if (log) events.log("[motore] in pausa: $why")
        return EnginePause(stopped) {
            synchronized(lock) { pauses-- }
            sync()
        }
    }

    /** Chiusura dell'IDE: niente più avvii, arresto con attesa breve. Bloccante. */
    fun dispose(graceMs: Long = EngineDefaults.DISPOSE_GRACE_MS) {
        synchronized(lock) { disposed = true }
        process.stop(graceMs)
    }

    /** Arresto su [executor]; chiamare con [lock] preso (gli eventi partono dal thread dell'arresto). */
    private fun stopAsync(graceMs: Long): CompletableFuture<Unit> {
        val future = CompletableFuture<Unit>()
        transition = future
        val task = Runnable {
            try {
                process.stop(graceMs)
            } finally {
                synchronized(lock) { if (transition === future) transition = null }
                future.complete(Unit)
                events.changed()
            }
        }
        try {
            executor.execute(task)
        } catch (_: RejectedExecutionException) {
            // Pool chiusi (plugin in chiusura): lo smaltimento ferma il motore.
            transition = null
            future.complete(Unit)
        }
        return future
    }

    private inner class ProcessEvents : EngineProcess.Events {
        override fun onLine(line: String) {
            try {
                val note = NotificationRules.parseEngineEvent(line)
                if (note != null) events.notification(note) else events.log("[motore] $line")
            } catch (e: Exception) {
                // Un errore di chi ascolta non ferma le righe successive.
                runCatching { events.warn("[motore] riga non consegnata: ${e.javaClass.simpleName}") }
            }
        }

        override fun onUnexpectedExit(run: Long, code: Int?) {
            val first = synchronized(lock) {
                // Uscita di un avvio superato: il motore attuale è un altro.
                if (run != generation) return
                waitingLock = false
                (!warned).also { warned = true }
            }
            events.warn(if (code == null) "[motore] avvio non riuscito" else "[motore] uscito con codice $code")
            if (first) events.unexpectedExit(code)
            events.changed()
        }

        override fun onLockBusy(run: Long) {
            synchronized(lock) {
                if (run != generation) return
                waitingLock = true
            }
            events.log("[motore] attivo altrove (lock occupato): riprovo quando si libera")
            events.changed()
        }
    }

    companion object {
        private fun done(): CompletableFuture<Unit> = CompletableFuture.completedFuture(Unit)

        /** Ritentare l'avvio: motore voluto, non attivo qui, in attesa del lock e lock ora libero. */
        fun shouldRetry(enabled: Boolean, localRunning: Boolean, waitingLock: Boolean, lockHeldElsewhere: Boolean?): Boolean =
            enabled && !localRunning && waitingLock && lockHeldElsewhere == false
    }
}
