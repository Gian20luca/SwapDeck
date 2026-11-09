package com.swapdeck.jetbrains.engine

import com.swapdeck.jetbrains.SwapDeckThreads
import java.nio.file.Path
import java.util.concurrent.CompletableFuture
import java.util.concurrent.Executor
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

/** Riprende il motore dopo una pausa; chiamarla più volte non cambia nulla. */
fun interface EngineResume {
    fun resume()
}

/**
 * Logica del motore (porting della parte motore dell'estensione VS Code), senza piattaforma:
 * segue l'impostazione [enabled], attende in silenzio se il lock è altrui (uscita 3) e riprova
 * quando uno stato dice che il lock è libero, avvisa una volta per avvio di un'uscita inattesa,
 * si mette in pausa per l'aggiornamento di cswap e il «Rifai login».
 *
 * Thread: i metodi si possono chiamare da qualunque thread; gli arresti (bloccanti fino a
 * [stopGraceMs]) girano su [executor] e i future si completano da lì.
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
        synchronized(lock) {
            transition?.let { pending -> return pending.thenCompose { sync() } }
            if (disposed || pauses > 0) return done()
            val wanted = enabled()
            when {
                wanted && !process.running -> {
                    warned = false
                    waitingLock = false
                    val path = cliPath()
                    events.log("[motore] avvio: $path headless")
                    process.start(path, env())
                }
                !wanted && process.running -> {
                    events.log("[motore] arresto")
                    return stopAsync(stopGraceMs).thenCompose { sync() }
                }
                !wanted -> waitingLock = false
            }
        }
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
            pauses == 0 && shouldRetry(enabled(), process.running, waitingLock, lockHeldElsewhere)
        }
        if (retry) {
            events.log("[motore] lock libero: nuovo avvio")
            sync()
        }
    }

    /**
     * Pausa per l'aggiornamento di cswap o il «Rifai login»: ferma il motore se acceso e impedisce
     * avvii e nuovi tentativi finché ogni pausa non è ripresa. Il future si completa a motore fermo.
     */
    fun pause(why: String): CompletableFuture<EngineResume> {
        val stopped = synchronized(lock) {
            pauses++
            when {
                process.running -> {
                    events.log("[motore] in pausa: $why")
                    stopAsync(stopGraceMs)
                }
                else -> transition ?: done()
            }
        }
        val resumed = AtomicBoolean(false)
        return stopped.thenApply {
            EngineResume {
                if (resumed.compareAndSet(false, true)) {
                    synchronized(lock) { pauses-- }
                    sync()
                }
            }
        }
    }

    /** Chiusura dell'IDE: niente più avvii, arresto con attesa breve. Bloccante. */
    fun dispose(graceMs: Long = EngineDefaults.DISPOSE_GRACE_MS) {
        synchronized(lock) { disposed = true }
        process.stop(graceMs)
    }

    /** Arresto su [executor]; chiamare con [lock] preso. */
    private fun stopAsync(graceMs: Long): CompletableFuture<Unit> {
        val future = CompletableFuture<Unit>()
        transition = future
        executor.execute {
            try {
                process.stop(graceMs)
            } finally {
                synchronized(lock) { if (transition === future) transition = null }
                future.complete(Unit)
                events.changed()
            }
        }
        return future
    }

    private inner class ProcessEvents : EngineProcess.Events {
        override fun onLine(line: String) {
            val note = NotificationRules.parseEngineEvent(line)
            if (note != null) events.notification(note) else events.log("[motore] $line")
        }

        override fun onUnexpectedExit(code: Int?) {
            events.warn("[motore] uscito con codice ${code ?: "segnale"}")
            val first = synchronized(lock) { (!warned).also { warned = true } }
            if (first) events.unexpectedExit(code)
            events.changed()
        }

        override fun onLockBusy() {
            waitingLock = true
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
