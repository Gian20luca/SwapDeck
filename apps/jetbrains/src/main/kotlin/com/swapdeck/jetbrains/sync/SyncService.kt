package com.swapdeck.jetbrains.sync

import com.intellij.openapi.Disposable
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.logger
import com.swapdeck.jetbrains.SwapDeckThreads
import com.swapdeck.jetbrains.cli.SwapDeckCli
import com.swapdeck.jetbrains.cli.runBlocking
import com.swapdeck.jetbrains.cli.toCswapError
import com.swapdeck.jetbrains.state.StateService
import kotlinx.serialization.json.JsonObject
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Sincronia dell'IDE con gli altri host (servizio dell'applicazione): la cartella della config
 * arriva da `swapdeck-cli config get` (anche con `SWAPDECK_HOME`), poi [SyncWatcher]. Un cambio
 * altrui fa rileggere lo stato e avvisa `configChanged`; dopo le proprie azioni [signal].
 */
@Service(Service.Level.APP)
class SyncService : Disposable {
    /** Identifica questo processo IDE nel segnale. */
    val source: String = SyncWatcher.newSource()

    @Volatile
    private var watcher: SyncWatcher? = null
    private val starting = AtomicBoolean(false)

    @Volatile
    private var disposed = false

    val watching: Boolean get() = watcher?.watching == true

    /**
     * Avvia l'osservazione se non è attiva (in background). La cartella può nascere dopo (primo
     * salvataggio): lo stato lo richiama a ogni lettura.
     */
    fun ensureStarted() {
        if (disposed || watching || !starting.compareAndSet(false, true)) return
        SwapDeckThreads.io.execute {
            try {
                val w = watcher ?: create()
                if (w.start()) LOG.info("sync: osservo la cartella della config di SwapDeck")
            } catch (e: Exception) {
                LOG.warn("sync: ${e.toCswapError().message}")
            } finally {
                starting.set(false)
            }
        }
    }

    private fun create(): SyncWatcher {
        val view = SwapDeckCli.getInstance().runBlocking(listOf("config", "get")) as? JsonObject
            ?: error("config get: risposta non valida")
        val dir = SyncWatcher.configDir(view)
        SyncWatcher.ensurePrivateDir(dir)
        return SyncWatcher(
            dir = dir,
            onChange = { StateService.getInstance().onExternalChange() },
            source = source,
            log = LOG::info,
        ).also { created ->
            synchronized(this) {
                if (disposed) created.close() else watcher = created
            }
        }
    }

    /** Avvisa gli altri host dopo una propria azione (in background; un errore lascia il polling). */
    fun signal() {
        val w = watcher ?: return
        SwapDeckThreads.io.execute { w.signal() }
    }

    override fun dispose() {
        synchronized(this) { disposed = true }
        watcher?.close()
    }

    companion object {
        private val LOG = logger<SyncService>()

        fun getInstance(): SyncService = service()
    }
}
