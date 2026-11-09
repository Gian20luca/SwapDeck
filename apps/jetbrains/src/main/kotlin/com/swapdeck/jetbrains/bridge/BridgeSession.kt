package com.swapdeck.jetbrains.bridge

import com.intellij.openapi.Disposable
import com.intellij.openapi.diagnostic.logger
import com.swapdeck.jetbrains.cli.CliException
import com.swapdeck.jetbrains.cli.CswapError
import com.swapdeck.jetbrains.cli.unwrap
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import java.util.concurrent.CancellationException
import java.util.concurrent.CompletableFuture

/**
 * Un pannello (browser JCEF di una tool window) collegato al ponte: riceve i messaggi `request` e
 * risponde con `progress` e `response` (forma di `HostMessage` in `packages/host/src/protocol.ts`)
 * tramite la funzione `send`, che resta del chiamante (trasporto JCEF, sottotask 9).
 *
 * `send` arriva dai thread del plugin, mai dall'EDT. Dopo [dispose] (pannello chiuso) il «Rifai
 * login» si annulla e non si manda più nulla; installazione e aggiornamento di cswap arrivano in fondo
 * (interromperli può lasciare cswap a metà) e il motore riparte, senza più avanzamento al pannello.
 */
class BridgeSession internal constructor(
    private val handler: BridgeHandler,
    /** Dialoghi, selettori e appunti di questo pannello. */
    val ui: BridgeUi,
) : Disposable {
    /** Durata del pannello: [dispose] la chiude. */
    val scope = AbortScope()

    val disposed: Boolean get() = scope.aborted

    /**
     * Messaggio del pannello già letto come JSON. Gestisce solo `{"type":"request"}` con `id` intero
     * sicuro (risposta asincrona con [send]) e restituisce `true`; per gli altri (`ready`, forme non
     * valide) restituisce `false` senza rispondere, come VS Code.
     */
    fun receive(message: JsonElement, send: (JsonObject) -> Unit): Boolean {
        val obj = message as? JsonObject ?: return false
        if (obj.textOrNull("type") != "request") return false
        val id = (obj["id"] as? JsonPrimitive)?.let { BridgeValidation.safeInteger(it) } ?: return false
        request(id, obj["method"], obj["params"], send)
        return true
    }

    /**
     * Come [receive], dal testo JSON del pannello; un testo non JSON o più lungo di
     * [MAX_MESSAGE_CHARS] si ignora (`false`) senza leggerlo.
     */
    fun receive(text: String, send: (JsonObject) -> Unit): Boolean {
        if (text.length > MAX_MESSAGE_CHARS) return false
        val message = try {
            Json.parseToJsonElement(text)
        } catch (e: Exception) {
            return false
        }
        return receive(message, send)
    }

    /** Richiesta [id]: avanzamento e risposta (anche d'errore) con [send]; si completa dopo la risposta. */
    fun request(id: Long, method: JsonElement?, params: JsonElement?, send: (JsonObject) -> Unit): CompletableFuture<Unit> {
        val post = { message: JsonObject -> if (!disposed) send(message) }
        val progress = { event: JsonObject ->
            post(
                buildJsonObject {
                    put("type", JsonPrimitive("progress"))
                    put("id", JsonPrimitive(id))
                    put("event", event)
                },
            )
        }
        return handler.handle(method, params, this, progress).handle { result, error ->
            post(
                buildJsonObject {
                    put("type", JsonPrimitive("response"))
                    put("id", JsonPrimitive(id))
                    if (error == null) {
                        put("ok", JsonPrimitive(true))
                        put("result", result)
                    } else {
                        put("ok", JsonPrimitive(false))
                        put("error", errorOf(error).json)
                    }
                },
            )
        }
    }

    /** Pannello chiuso: annulla il «Rifai login»; avanzamento e risposte successive si scartano. */
    override fun dispose() = scope.abort()

    companion object {
        /**
         * Lunghezza massima (in caratteri) di un messaggio del pannello: sopra il testo più lungo che
         * il protocollo ammette (`copyText`, un milione di caratteri) con margine per il JSON e gli
         * escape. Oltre si scarta prima di leggerlo.
         */
        const val MAX_MESSAGE_CHARS: Int = 4 * 1024 * 1024

        private val LOG = logger<BridgeSession>()

        /**
         * Errore per il pannello: quello della CLI, `cancelled` per un annullamento, altrimenti
         * `failed` con un messaggio generico (il dettaglio, che può avere percorsi locali, va solo nel
         * log dell'IDE).
         */
        fun errorOf(error: Throwable): CswapError = when (val cause = error.unwrap()) {
            is CliException -> cause.error
            is CancellationException -> CswapError.of("cancelled", "operazione annullata")
            else -> {
                LOG.warn("richiesta del pannello non riuscita", cause)
                CswapError.of("failed", "errore interno del plugin (dettagli nel log dell'IDE)")
            }
        }
    }
}
