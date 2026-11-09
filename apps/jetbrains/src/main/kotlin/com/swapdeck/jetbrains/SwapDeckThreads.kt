package com.swapdeck.jetbrains

import com.intellij.openapi.Disposable
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.ThreadFactory
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/**
 * Pool di thread del plugin, tutti daemon (non trattengono la chiusura dell'IDE né dei test).
 *
 * - [io]: lavori che si bloccano (lettura degli stream dei processi, attese di uscita, arresto del
 *   motore, scrittura del segnale); pool senza limite, i thread inattivi muoiono dopo 60 s.
 * - [scheduler]: solo timer brevi (timeout, debounce, polling): il lavoro vero va su [io].
 *
 * Dopo [shutdown] i pool rifiutano nuovi lavori (`RejectedExecutionException`) e quelli in corso
 * ricevono un'interruzione.
 */
class PluginThreads(prefix: String = "SwapDeck") {
    val io: ExecutorService = Executors.newCachedThreadPool(factory("$prefix I/O"))

    val scheduler: ScheduledExecutorService =
        ScheduledThreadPoolExecutor(1, factory("$prefix timer")).apply { removeOnCancelPolicy = true }

    val isShutdown: Boolean get() = io.isShutdown

    /** Ferma i pool: timer annullati, lavori interrotti, attesa breve della fine dei thread. */
    fun shutdown(waitMs: Long = SHUTDOWN_WAIT_MS) {
        scheduler.shutdownNow()
        io.shutdownNow()
        try {
            scheduler.awaitTermination(waitMs, TimeUnit.MILLISECONDS)
            io.awaitTermination(waitMs, TimeUnit.MILLISECONDS)
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
        }
    }

    private fun factory(name: String): ThreadFactory {
        val count = AtomicInteger()
        return ThreadFactory { task ->
            Thread(task, "$name ${count.incrementAndGet()}").apply { isDaemon = true }
        }
    }

    companion object {
        const val SHUTDOWN_WAIT_MS: Long = 2_000
    }
}

/**
 * Thread del plugin (un solo [PluginThreads] per processo IDE). Non si usano i pool della
 * piattaforma così la logica resta provabile senza IDE; li chiude [SwapDeckLifecycle].
 */
object SwapDeckThreads {
    private val threads = PluginThreads()

    val io: ExecutorService get() = threads.io

    val scheduler: ScheduledExecutorService get() = threads.scheduler

    val isShutdown: Boolean get() = threads.isShutdown

    fun shutdown() = threads.shutdown()
}

/**
 * Fine vita del plugin (chiusura dell'IDE o scaricamento del plugin): chiude i pool di
 * [SwapDeckThreads]. Ogni servizio del plugin lo chiede per primo nel costruttore ([ensure]), così
 * questo servizio nasce prima degli altri e la piattaforma lo smaltisce per ultimo, dopo che motore,
 * stato, sincronia e CLI (che annulla i comandi in corso) si sono chiusi.
 */
@Service(Service.Level.APP)
class SwapDeckLifecycle : Disposable {
    override fun dispose() = SwapDeckThreads.shutdown()

    companion object {
        /** Da chiamare all'inizio del costruttore di ogni servizio del plugin. */
        fun ensure() {
            service<SwapDeckLifecycle>()
        }
    }
}
