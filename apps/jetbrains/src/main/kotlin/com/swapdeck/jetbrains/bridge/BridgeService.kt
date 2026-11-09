package com.swapdeck.jetbrains.bridge

import com.intellij.ide.plugins.PluginManagerCore
import com.intellij.openapi.application.ApplicationInfo
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.extensions.PluginId
import com.intellij.util.messages.Topic
import com.swapdeck.jetbrains.SwapDeckLifecycle
import com.swapdeck.jetbrains.cli.SwapDeckCli
import com.swapdeck.jetbrains.engine.EnginePause
import com.swapdeck.jetbrains.engine.EngineService
import com.swapdeck.jetbrains.state.CliState
import com.swapdeck.jetbrains.state.StateService
import com.swapdeck.jetbrains.state.SwapDeckStateListener
import com.swapdeck.jetbrains.sync.SyncService
import kotlinx.serialization.json.JsonObject
import java.util.concurrent.CompletableFuture

/** Cambi che chiedono alle viste di ridisegnarsi (bus dell'applicazione), dai thread del plugin. */
fun interface SwapDeckViewListener {
    /**
     * Preferenze di SwapDeck salvate dal pannello (`setPreferences`): la tool window rilegge la
     * lingua (`preferences get`) e manda `view` ai pannelli, come `loadLanguage` di VS Code.
     */
    fun preferencesChanged()

    companion object {
        @JvmField
        @Topic.AppLevel
        val TOPIC: Topic<SwapDeckViewListener> = Topic(SwapDeckViewListener::class.java, Topic.BroadcastDirection.NONE)
    }
}

/** [BridgeHost] sui servizi del plugin: stato, motore, sincronia, bus dell'applicazione. */
class IdeBridgeHost : BridgeHost {
    private val bus get() = ApplicationManager.getApplication().messageBus

    override fun lastState(): CliState? = StateService.getInstance().state

    override fun refresh(): CompletableFuture<CliState> = StateService.getInstance().refresh()

    /** `StateService.afterAction` manda anche il segnale di sincronia e `configChanged`. */
    override fun afterAction(outcome: JsonObject): CompletableFuture<*> = StateService.getInstance().afterAction(outcome)

    override fun pauseEngine(why: String): EnginePause = EngineService.getInstance().pause(why)

    override fun changed() {
        SyncService.getInstance().signal()
        bus.syncPublisher(SwapDeckStateListener.TOPIC).configChanged()
    }

    override fun preferencesChanged() = bus.syncPublisher(SwapDeckViewListener.TOPIC).preferencesChanged()

    override val clientInfo: String get() = CLIENT_INFO

    companion object {
        /** `<IDE> <versione>, plugin <versione>`: `diagnostics --client` e riga d'avvio nel log. */
        val CLIENT_INFO: String by lazy {
            val ide = ApplicationInfo.getInstance().fullApplicationName
            val plugin = PluginManagerCore.getPlugin(PluginId.getId(SwapDeckCli.PLUGIN_ID))?.version ?: "?"
            "$ide, plugin $plugin"
        }
    }
}

/**
 * Il ponte del plugin (servizio dell'applicazione: un solo «Rifai login» per processo IDE). Ogni
 * pannello apre la sua sessione:
 *
 * ```
 * val session = BridgeService.getInstance().openSession(IdeBridgeUi(project))
 * Disposer.register(content, session)
 * session.receive(jsonText) { message -> post(message) } // post: trasporto verso il pannello
 * ```
 */
@Service(Service.Level.APP)
class BridgeService {
    init {
        SwapDeckLifecycle.ensure()
    }

    val handler: BridgeHandler = BridgeHandler(SwapDeckCli.getInstance(), IdeBridgeHost())

    fun openSession(ui: BridgeUi): BridgeSession = handler.openSession(ui)

    companion object {
        fun getInstance(): BridgeService = service()
    }
}
