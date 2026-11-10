package com.swapdeck.jetbrains.toolwindow

import com.swapdeck.jetbrains.bridge.BridgeSession
import com.swapdeck.jetbrains.state.CliState
import kotlinx.serialization.json.JsonObject

/**
 * Instradamento dei messaggi di un pannello, senza IDE (il browser e il ponte arrivano come funzioni).
 *
 * - Dal pannello: [fromPage] passa il testo al ponte ([receive], cioè `BridgeSession.receive`); se il
 *   ponte non lo prende ed è `{"type":"ready"}`, il pannello è pronto e riceve `view`, `state` ed
 *   `engine` ([snapshot]).
 * - Al pannello: [post] scrive con [execute] (`executeJavaScript`) solo a pannello pronto e non chiuso.
 *   Prima del `ready` non serve una coda: il pannello lo manda dopo l'iniezione di
 *   `window.__swapdeckHost`, e la risposta al `ready` porta lingua, tema e stato attuali; le risposte
 *   a richieste di una pagina ricaricata non hanno più chi le aspetta.
 * - [pageLoading] (nuovo caricamento) e [close] (pannello chiuso) fermano i messaggi; dopo [close]
 *   [execute] non si chiama più.
 */
class PanelConnection(
    private val receive: (String, (JsonObject) -> Unit) -> Boolean,
    private val execute: (String) -> Unit,
    private val snapshot: () -> Pair<ViewSettings, CliState?>,
) {
    private val lock = Any()
    private var live = false
    private var closed = false

    /** Il pannello ha mandato `ready` e non si è ricaricato né chiuso. */
    val isLive: Boolean get() = synchronized(lock) { live && !closed }

    /** Messaggio del pannello (testo JSON, non fidato). */
    fun fromPage(text: String) {
        if (synchronized(lock) { closed }) return
        // Troppo lungo: si scarta senza leggerlo (né qui né nel ponte).
        if (text.length > BridgeSession.MAX_MESSAGE_CHARS) return
        if (receive(text, ::post)) return
        if (PanelMessages.typeOf(text) != "ready") return
        synchronized(lock) {
            if (closed) return
            live = true
            val (view, state) = snapshot()
            for (message in PanelMessages.handshake(view, state)) execute(PanelMessages.dispatchScript(message))
        }
    }

    /** Messaggio dell'host (`HostMessage`) al pannello, se è pronto; altrimenti si scarta. */
    fun post(message: JsonObject) {
        synchronized(lock) {
            if (live && !closed) execute(PanelMessages.dispatchScript(message))
        }
    }

    /** Inizio di un caricamento della pagina: niente messaggi fino al prossimo `ready`. */
    fun pageLoading() = synchronized(lock) { live = false }

    /** Pannello chiuso: dopo il ritorno nessun messaggio arriva più a [execute]. */
    fun close() = synchronized(lock) {
        closed = true
        live = false
    }
}
