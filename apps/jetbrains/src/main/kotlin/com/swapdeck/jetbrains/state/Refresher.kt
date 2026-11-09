package com.swapdeck.jetbrains.state

import java.util.concurrent.CompletableFuture

/**
 * Lettura condivisa con contatore di generazione: le chiamate concorrenti condividono una sola
 * lettura, e una lettura partita prima di un'azione ([invalidate]) non sovrascrive lo stato
 * prodotto dall'azione.
 *
 * @param read lettura (non dovrebbe fallire: gli errori diventano un valore).
 * @param accept riceve il valore e se è ancora attuale; restituisce lo stato da dare ai chiamanti.
 *   Gira col lock preso (come [invalidate] e [locked]): deve essere breve e senza I/O.
 * @param after solo per una lettura attuale, una volta, fuori dal lock: il future condiviso si
 *   completa quando ha finito (per esempio dopo il riepilogo della barra di stato).
 */
class Refresher<T : Any>(
    private val read: () -> CompletableFuture<T>,
    private val accept: (value: T, current: Boolean) -> T,
    private val after: (value: T) -> CompletableFuture<*> = { CompletableFuture.completedFuture(Unit) },
) {
    private val lock = Any()
    private var generation = 0L
    private var inflight: CompletableFuture<T>? = null

    /** Una sola lettura alla volta: le chiamate concorrenti ricevono lo stesso future. */
    fun refresh(): CompletableFuture<T> {
        val shared: CompletableFuture<T>
        val started: Long
        synchronized(lock) {
            inflight?.let { return it }
            shared = CompletableFuture<T>()
            inflight = shared
            started = generation
        }
        val reading = try {
            read()
        } catch (e: Exception) {
            CompletableFuture.failedFuture(e)
        }
        reading.whenComplete { value, error ->
            if (error != null) {
                release(shared)
                shared.completeExceptionally(error)
                return@whenComplete
            }
            val (accepted, current) = synchronized(lock) {
                val current = started == generation
                accept(value, current) to current
            }
            // Da qui una nuova chiamata avvia una nuova lettura.
            release(shared)
            val tail = if (current) {
                try {
                    after(accepted)
                } catch (e: Exception) {
                    CompletableFuture.failedFuture<Unit>(e)
                }
            } else {
                CompletableFuture.completedFuture(Unit)
            }
            tail.whenComplete { _, _ -> shared.complete(accepted) }
        }
        return shared
    }

    /** Dopo un'azione: le letture in corso diventano vecchie e la prossima ne avvia una nuova. */
    fun invalidate() {
        synchronized(lock) {
            generation++
            inflight = null
        }
    }

    /** Esegue [block] con il lock di [accept] (per cambiare lo stato fuori da una lettura). */
    fun <R> locked(block: () -> R): R = synchronized(lock) { block() }

    private fun release(shared: CompletableFuture<T>) {
        synchronized(lock) { if (inflight === shared) inflight = null }
    }
}
