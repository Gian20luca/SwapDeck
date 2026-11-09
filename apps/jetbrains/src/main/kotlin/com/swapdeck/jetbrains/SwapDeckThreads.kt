package com.swapdeck.jetbrains

import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledThreadPoolExecutor
import java.util.concurrent.ThreadFactory
import java.util.concurrent.atomic.AtomicInteger

/**
 * Thread del plugin, tutti daemon (non trattengono la chiusura dell'IDE né dei test).
 *
 * - [io]: lavori che si bloccano (lettura degli stream dei processi, attese di uscita, arresto del
 *   motore, scrittura del segnale); pool senza limite, i thread inattivi muoiono dopo 60 s.
 * - [scheduler]: solo timer brevi (timeout, debounce, polling): il lavoro vero va su [io].
 *
 * Non si usano i pool della piattaforma così la logica resta provabile senza IDE.
 */
object SwapDeckThreads {
    val io: ExecutorService = Executors.newCachedThreadPool(factory("SwapDeck I/O"))

    val scheduler: ScheduledExecutorService =
        ScheduledThreadPoolExecutor(1, factory("SwapDeck timer")).apply { removeOnCancelPolicy = true }

    private fun factory(name: String): ThreadFactory {
        val count = AtomicInteger()
        return ThreadFactory { task ->
            Thread(task, "$name ${count.incrementAndGet()}").apply { isDaemon = true }
        }
    }
}
