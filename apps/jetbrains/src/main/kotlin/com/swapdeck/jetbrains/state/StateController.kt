package com.swapdeck.jetbrains.state

import com.swapdeck.jetbrains.cli.CliCaller
import com.swapdeck.jetbrains.cli.CliOptions
import com.swapdeck.jetbrains.cli.CswapError
import com.swapdeck.jetbrains.cli.toCswapError
import com.swapdeck.jetbrains.engine.NotificationRules
import com.swapdeck.jetbrains.engine.SwapDeckNotification
import kotlinx.serialization.json.JsonObject
import java.util.concurrent.CompletableFuture

/** Cosa succede allo stato; chiamate dai thread del plugin (mai dall'EDT). */
interface StateEvents {
    fun stateChanged(state: CliState)

    fun statusChanged(status: IdeStatus)

    /** Notifica da `notifications poll` (nessun motore vivo). */
    fun notification(notification: SwapDeckNotification)

    fun warn(message: String)
}

/** Ciò che lo stato deve sapere del motore di questo processo. */
interface EngineView {
    val running: Boolean
    val pid: Long?

    /** Ad ogni stato letto, per il nuovo tentativo dopo un lock occupato. */
    fun onLockState(lockHeldElsewhere: Boolean?)
}

/**
 * Stato letto dalla CLI (porting di `refresher.ts` e della lettura di `extension.ts`), senza
 * piattaforma: letture condivise ([Refresher]), esito delle azioni che vince sulle letture vecchie,
 * dopo ogni lettura avviso, nuovo tentativo del motore, notifiche da `notifications poll` quando
 * nessun motore è vivo e riepilogo `ide status` per la barra di stato.
 */
class StateController(
    private val cli: CliCaller,
    private val engine: EngineView,
    private val thresholds: () -> List<Int>,
    private val events: StateEvents,
    /** Dopo ogni lettura attuale (la sincronia ritenta l'avvio se la cartella è nata dopo). */
    private val afterRead: (CliState) -> Unit = {},
    private val clock: () -> Long = System::currentTimeMillis,
) {
    @Volatile
    var state: CliState? = null
        private set

    @Volatile
    var status: IdeStatus? = null
        private set

    private val statusLock = Any()
    private var statusFor: StatusKey? = null
    private var statusJob: CompletableFuture<Unit> = CompletableFuture.completedFuture(Unit)

    private class StatusKey(val state: CliState, val args: List<String>)

    private val refresher = Refresher(
        read = ::readState,
        accept = { next, current ->
            // Partita prima di un'azione: lo stato nuovo è già lì.
            if (!current) {
                state ?: next
            } else {
                state = next
                next
            }
        },
        after = ::published,
    )

    /** Rilettura; si completa con la barra di stato già aggiornata. Non fallisce. */
    fun refresh(): CompletableFuture<CliState> = refresher.refresh()

    /** Dopo un'azione: le letture in corso diventano vecchie. */
    fun invalidate() = refresher.invalidate()

    /** Millisecondi dall'ultima lettura (o dall'ultimo esito di un'azione); `Long.MAX_VALUE` se nessuna. */
    fun ageMs(): Long = state?.let { clock() - it.updatedAtMs } ?: Long.MAX_VALUE

    /**
     * Esito di un'azione (`ActionOutcome`): con `list` lo stato si aggiorna senza rileggere,
     * altrimenti si rilegge. Le letture partite prima non lo sovrascrivono.
     */
    fun afterAction(outcome: JsonObject): CompletableFuture<CliState> {
        refresher.invalidate()
        val list = outcome["list"] as? JsonObject ?: return refresh()
        if (state == null) return refresh().thenCompose { applyList(list) }
        return applyList(list)
    }

    private fun applyList(list: JsonObject): CompletableFuture<CliState> {
        val next = refresher.locked {
            val base = state ?: CliState.failed(CswapError.of("failed", ""), null, clock())
            base.withList(list, clock()).also { state = it }
        }
        pollNotifications(list)
        events.stateChanged(next)
        return updateStatus().thenApply { next }
    }

    /** Lettura di `state`: non fallisce, un errore della CLI diventa uno stato con `error`. */
    private fun readState(): CompletableFuture<CliState> =
        cli.run(listOf("state"), CliOptions()).handle { value, error ->
            val obj = value as? JsonObject
            if (error == null && obj != null) {
                CliState(obj)
            } else {
                val info = error?.toCswapError() ?: CswapError.of("failed", "state: risposta non valida")
                events.warn("state: ${info.message}")
                CliState.failed(info, state, clock())
            }
        }

    private fun published(next: CliState): CompletableFuture<Unit> {
        next.error?.let { events.warn("state: ${it.kind}: ${it.message}") }
        next.list?.let { pollNotifications(it) }
        events.stateChanged(next)
        afterRead(next)
        engine.onLockState(next.lockHeldElsewhere)
        return updateStatus()
    }

    /** Mittente unico: senza motore vivo (né il nostro né altrove) le notifiche le chiede la CLI. */
    private fun pollNotifications(list: JsonObject) {
        if (!NotificationRules.shouldPoll(engine.running, state?.lockHeldElsewhere)) return
        cli.run(listOf("notifications", "poll", "-"), CliOptions(stdin = list.toString())).whenComplete { value, error ->
            if (error != null) {
                events.warn("notifications: ${error.toCswapError().message}")
            } else {
                (value as? JsonObject)?.let { poll -> NotificationRules.fromPoll(poll).forEach(events::notification) }
            }
        }
    }

    /**
     * Chiede alla CLI il riepilogo per lo stato attuale, le soglie e il PID del motore di questo
     * processo. Stessa richiesta = stessa attesa; una risposta superata si scarta.
     */
    fun updateStatus(): CompletableFuture<Unit> {
        val current = state ?: return CompletableFuture.completedFuture(Unit)
        val args = IdeStatus.args(thresholds(), engine.pid)
        val mine = StatusKey(current, args)
        synchronized(statusLock) {
            val last = statusFor
            if (last != null && last.state === current && last.args == args) return statusJob
            statusFor = mine
            statusJob = cli.run(args, CliOptions(stdin = current.json.toString()))
                .handle { value, error ->
                    val obj = value as? JsonObject
                    if (error == null && obj != null) {
                        IdeStatus(obj)
                    } else {
                        val info = error?.toCswapError() ?: CswapError.of("failed", "ide status: risposta non valida")
                        events.warn("ide status: ${info.message}")
                        IdeStatus.error(info, clock())
                    }
                }
                .thenApply { next ->
                    val fresh = synchronized(statusLock) { (statusFor === mine).also { if (it) status = next } }
                    if (fresh) events.statusChanged(next)
                }
            return statusJob
        }
    }
}
