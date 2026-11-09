package com.swapdeck.jetbrains.state

import com.intellij.openapi.Disposable
import com.intellij.openapi.application.ApplicationActivationListener
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.components.serviceIfCreated
import com.intellij.openapi.diagnostic.logger
import com.intellij.openapi.wm.IdeFrame
import com.intellij.util.messages.Topic
import com.swapdeck.jetbrains.SwapDeckThreads
import com.swapdeck.jetbrains.cli.SwapDeckCli
import com.swapdeck.jetbrains.engine.EngineService
import com.swapdeck.jetbrains.engine.SwapDeckEngineListener
import com.swapdeck.jetbrains.engine.SwapDeckNotification
import com.swapdeck.jetbrains.engine.SwapDeckNotificationListener
import com.swapdeck.jetbrains.settings.SwapDeckSettings
import com.swapdeck.jetbrains.settings.SwapDeckSettingsListener
import com.swapdeck.jetbrains.sync.SyncService
import kotlinx.serialization.json.JsonObject
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/** Stato, riepilogo della barra e cambi della config (bus dell'applicazione), dai thread del plugin. */
interface SwapDeckStateListener {
    fun stateChanged(state: CliState) {}

    fun statusChanged(status: IdeStatus) {}

    /** Config comune cambiata (altro host o propria azione): il pannello rilegge i suoi dati. */
    fun configChanged() {}

    companion object {
        @JvmField
        @Topic.AppLevel
        val TOPIC: Topic<SwapDeckStateListener> = Topic(SwapDeckStateListener::class.java, Topic.BroadcastDirection.NONE)
    }
}

/**
 * Stato di SwapDeck per tutta l'IDE (servizio dell'applicazione): polling di `state` ogni
 * `refreshIntervalSeconds` (minimo 15), rilettura al ritorno in primo piano se l'ultima ha più di
 * 10 s, motore e sincronia avviati con [start]. La logica sta in [StateController].
 */
@Service(Service.Level.APP)
class StateService : Disposable {
    private val bus get() = ApplicationManager.getApplication().messageBus
    private val started = AtomicBoolean(false)
    private var timer: ScheduledFuture<*>? = null

    @Volatile
    private var disposed = false

    private val controller = StateController(
        cli = SwapDeckCli.getInstance(),
        engine = object : EngineView {
            override val running: Boolean get() = EngineService.getInstance().running
            override val pid: Long? get() = EngineService.getInstance().pid

            override fun onLockState(lockHeldElsewhere: Boolean?) = EngineService.getInstance().onLockState(lockHeldElsewhere)
        },
        thresholds = { SwapDeckSettings.getInstance().snapshot.thresholds },
        events = object : StateEvents {
            override fun stateChanged(state: CliState) = bus.syncPublisher(SwapDeckStateListener.TOPIC).stateChanged(state)

            override fun statusChanged(status: IdeStatus) = bus.syncPublisher(SwapDeckStateListener.TOPIC).statusChanged(status)

            override fun notification(notification: SwapDeckNotification) =
                bus.syncPublisher(SwapDeckNotificationListener.TOPIC).notify(notification)

            override fun warn(message: String) = LOG.warn(message)
        },
        afterRead = { if (!disposed) SyncService.getInstance().ensureStarted() },
    )

    val state: CliState? get() = controller.state
    val status: IdeStatus? get() = controller.status

    /** Avvio (idempotente): motore, prima lettura, polling. */
    fun start() {
        if (disposed || !started.compareAndSet(false, true)) return
        val connection = bus.connect(this)
        connection.subscribe(
            SwapDeckSettingsListener.TOPIC,
            SwapDeckSettingsListener { old, new ->
                if (old.refreshIntervalSeconds != new.refreshIntervalSeconds) schedule()
                controller.updateStatus()
            },
        )
        connection.subscribe(
            SwapDeckEngineListener.TOPIC,
            object : SwapDeckEngineListener {
                override fun engineChanged() {
                    controller.updateStatus()
                }
            },
        )
        val engine = EngineService.getInstance()
        // Notifica del motore titolare del lock: gli altri host rileggono.
        engine.onNotification = { SyncService.getInstance().signal() }
        engine.sync()
        refresh().whenComplete { _, _ -> schedule() }
    }

    /** Rilettura; si completa con la barra di stato già aggiornata. */
    fun refresh(): CompletableFuture<CliState> = controller.refresh()

    /** Le letture in corso diventano vecchie (prima di applicare l'esito di un'azione). */
    fun invalidate() = controller.invalidate()

    /**
     * Dopo una propria azione (`ActionOutcome`): gli altri host rileggono subito, il pannello
     * rilegge la config, lo stato si aggiorna dall'esito (o si rilegge).
     */
    fun afterAction(outcome: JsonObject): CompletableFuture<CliState> {
        SyncService.getInstance().signal()
        bus.syncPublisher(SwapDeckStateListener.TOPIC).configChanged()
        return controller.afterAction(outcome)
    }

    /** Cambio fatto da un altro processo (config o segnale). */
    fun onExternalChange() {
        refresh()
        bus.syncPublisher(SwapDeckStateListener.TOPIC).configChanged()
    }

    /** Ritorno in primo piano dell'IDE: rilettura se l'ultima è più vecchia di 10 s. */
    fun onActivated() {
        if (started.get() && controller.ageMs() > FOCUS_REFRESH_AFTER_MS) refresh()
    }

    private fun schedule() {
        synchronized(this) {
            timer?.cancel(false)
            if (disposed) return
            val seconds = SwapDeckSettings.getInstance().snapshot.refreshIntervalSeconds.toLong()
            timer = SwapDeckThreads.scheduler.schedule(
                { SwapDeckThreads.io.execute { refresh().whenComplete { _, _ -> schedule() } } },
                seconds,
                TimeUnit.SECONDS,
            )
        }
    }

    override fun dispose() {
        synchronized(this) {
            disposed = true
            timer?.cancel(false)
        }
    }

    companion object {
        /** Al ritorno in primo piano si rilegge solo se l'ultima lettura è più vecchia. */
        const val FOCUS_REFRESH_AFTER_MS = 10_000L
        private val LOG = logger<StateService>()

        fun getInstance(): StateService = service()
    }
}

/** Ritorno in primo piano di una finestra dell'IDE (registrato in `plugin.xml`). */
class SwapDeckActivationListener : ApplicationActivationListener {
    override fun applicationActivated(ideFrame: IdeFrame) {
        // Senza servizio avviato non c'è niente da rileggere: non lo si crea qui.
        serviceIfCreated<StateService>()?.onActivated()
    }
}
