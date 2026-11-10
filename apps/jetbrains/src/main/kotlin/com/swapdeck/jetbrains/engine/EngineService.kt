package com.swapdeck.jetbrains.engine

import com.intellij.openapi.Disposable
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.logger
import com.intellij.util.messages.Topic
import com.swapdeck.jetbrains.SwapDeckLifecycle
import com.swapdeck.jetbrains.SwapDeckThreads
import com.swapdeck.jetbrains.cli.CliEnv
import com.swapdeck.jetbrains.cli.SwapDeckCli
import com.swapdeck.jetbrains.settings.SwapDeckSettings
import com.swapdeck.jetbrains.settings.SwapDeckSettingsListener
import java.util.concurrent.CompletableFuture
import java.util.concurrent.RejectedExecutionException

/** Cambi del motore (bus dell'applicazione), dai thread del plugin. */
interface SwapDeckEngineListener {
    /** Acceso, spento, in attesa del lock o PID cambiati. */
    fun engineChanged() {}

    /** Il motore si è fermato da solo (codice d'uscita; `null` se non è partito): un avviso per avvio. */
    fun engineStoppedUnexpectedly(exitCode: Int?) {}

    companion object {
        @JvmField
        @Topic.AppLevel
        val TOPIC: Topic<SwapDeckEngineListener> = Topic(SwapDeckEngineListener::class.java, Topic.BroadcastDirection.NONE)
    }
}

/**
 * Un solo motore per processo IDE (servizio dell'applicazione, non del progetto): segue
 * l'impostazione `engineEnabled`, riparte al cambio di `cliPath`, si ferma alla chiusura dell'IDE.
 * Le notifiche del motore vanno a [SwapDeckNotificationListener.TOPIC]; dopo ognuna [onNotification]
 * (la sincronia avvisa le altre finestre).
 */
@Service(Service.Level.APP)
class EngineService : Disposable {
    init {
        SwapDeckLifecycle.ensure()
    }

    private val bus get() = ApplicationManager.getApplication().messageBus

    /** Chiamato dopo ogni notifica del motore (lo imposta il servizio della sincronia). */
    @Volatile
    var onNotification: () -> Unit = {}

    private val controller = EngineController(
        enabled = { SwapDeckSettings.getInstance().snapshot.engineEnabled },
        cliPath = { SwapDeckCli.getInstance().executable() },
        env = { CliEnv.of(System.getenv()) },
        events = object : EngineControllerEvents {
            override fun log(message: String) = LOG.info(message)

            override fun warn(message: String) = LOG.warn(message)

            override fun notification(notification: SwapDeckNotification) {
                // Solo il titolare del lock manda eventi: una notifica per evento, non una per IDE.
                bus.syncPublisher(SwapDeckNotificationListener.TOPIC).notify(notification)
                onNotification()
            }

            override fun unexpectedExit(code: Int?) =
                bus.syncPublisher(SwapDeckEngineListener.TOPIC).engineStoppedUnexpectedly(code)

            override fun changed() = bus.syncPublisher(SwapDeckEngineListener.TOPIC).engineChanged()
        },
    )

    init {
        bus.connect(this).subscribe(
            SwapDeckSettingsListener.TOPIC,
            SwapDeckSettingsListener { old, new ->
                // Il salvataggio delle impostazioni avvisa spesso dall'EDT: avvio e arresto altrove.
                val task = when {
                    old.cliPath != new.cliPath -> Runnable { controller.restart() }
                    old.engineEnabled != new.engineEnabled -> Runnable { controller.sync() }
                    else -> null
                }
                if (task != null) {
                    try {
                        SwapDeckThreads.io.execute(task)
                    } catch (_: RejectedExecutionException) {
                        // Plugin in chiusura.
                    }
                }
            },
        )
    }

    val running: Boolean get() = controller.running
    val pid: Long? get() = controller.pid
    val waitingLock: Boolean get() = controller.waitingLock

    /** Avvia (o ferma) secondo le impostazioni. */
    fun sync(): CompletableFuture<Unit> = controller.sync()

    /** Ad ogni stato letto (`engine.running` senza `problem`, altrimenti `null`). */
    fun onLockState(lockHeldElsewhere: Boolean?) = controller.onLockState(lockHeldElsewhere)

    /**
     * Pausa per l'aggiornamento di cswap (`upgrade`) o il «Rifai login» (`relogin`): vale subito,
     * va sempre ripresa ([EnginePause.resume] in un `finally`).
     */
    fun pause(why: String): EnginePause = controller.pause(why)

    override fun dispose() = controller.dispose()

    companion object {
        private val LOG = logger<EngineService>()

        fun getInstance(): EngineService = service()
    }
}
