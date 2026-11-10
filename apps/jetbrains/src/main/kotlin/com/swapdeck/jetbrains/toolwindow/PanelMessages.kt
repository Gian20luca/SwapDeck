package com.swapdeck.jetbrains.toolwindow

import com.swapdeck.jetbrains.state.CliState
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

/** Lingua e tema delle viste (`ViewSettings` di `packages/host/src/protocol.ts`). */
data class ViewSettings(val locale: String, val theme: String) {
    fun toJson(): JsonObject = buildJsonObject {
        put("locale", JsonPrimitive(locale))
        put("theme", JsonPrimitive(theme))
    }

    companion object {
        /** `language` delle preferenze di SwapDeck, se valida (`preferredLocaleOf`). */
        fun preferredLocaleOf(language: JsonElement?): String? =
            (language as? JsonPrimitive)?.takeIf { it.isString }?.content?.takeIf { it == "it" || it == "en" }

        /** Lingua scelta in SwapDeck, altrimenti quella dell'IDE (`it…` o inglese), come `viewLocale`. */
        fun localeOf(preferred: String?, ideLanguage: String): String =
            preferred ?: if (ideLanguage.lowercase().startsWith("it")) "it" else "en"

        /** Tema dal Look and Feel dell'IDE (`JBColor.isBright()`). */
        fun themeOf(bright: Boolean): String = if (bright) "light" else "dark"
    }
}

/**
 * Messaggi dell'host al pannello (`HostMessage`) e script per consegnarli con `executeJavaScript`
 * (contratto nel README). Nessun dato diventa codice: il JSON entra nello script come letterale
 * stringa ([jsStringLiteral]).
 */
object PanelMessages {
    const val HOST_MESSAGE_EVENT = "swapdeck-host"
    const val HOST_READY_EVENT = "swapdeck-host-ready"

    fun view(settings: ViewSettings): JsonObject = buildJsonObject {
        put("type", JsonPrimitive("view"))
        put("settings", settings.toJson())
    }

    /** `state` con la `ListResponse`; `null` se la lettura è fallita (come VS Code non si manda). */
    fun state(state: CliState): JsonObject? = state.list?.let { list ->
        buildJsonObject {
            put("type", JsonPrimitive("state"))
            put("list", list)
        }
    }

    /** `engine`: sospensione del motore decisa dal Rust (`suspensionOf`), `null` se non c'è. */
    fun engine(state: CliState): JsonObject = buildJsonObject {
        put("type", JsonPrimitive("engine"))
        put("suspended", suspensionOf(state.engine))
    }

    val configChanged: JsonObject = buildJsonObject { put("type", JsonPrimitive("configChanged")) }

    /** `{reason, detail}` se `engine.suspended` c'è (stringa), altrimenti `null`. */
    fun suspensionOf(engine: JsonObject): JsonElement {
        val reason = (engine["suspended"] as? JsonPrimitive)?.takeIf { it.isString } ?: return JsonNull
        return buildJsonObject {
            put("reason", reason)
            put("detail", engine["suspendDetail"] as? JsonArray ?: JsonArray(emptyList()))
        }
    }

    /** Risposta al `ready` del pannello: `view`, poi `state` ed `engine` se c'è uno stato. */
    fun handshake(view: ViewSettings, state: CliState?): List<JsonObject> = buildList {
        add(view(view))
        if (state != null) {
            state(state)?.let(::add)
            add(engine(state))
        }
    }

    /** `type` di un messaggio del pannello in JSON, `null` se non è un oggetto con `type` stringa. */
    fun typeOf(text: String): String? = try {
        ((Json.parseToJsonElement(text) as? JsonObject)?.get("type") as? JsonPrimitive)?.takeIf { it.isString }?.content
    } catch (_: Exception) {
        null
    }

    /** Script che consegna [message] al pannello (evento `swapdeck-host` con il JSON come stringa). */
    fun dispatchScript(message: JsonObject): String =
        "window.dispatchEvent(new CustomEvent('$HOST_MESSAGE_EVENT', { detail: ${jsStringLiteral(message.toString())} }));"

    /**
     * Script che crea `window.__swapdeckHost` con `post(json)` (il corpo è [postCall], dal
     * `JBCefJSQuery.inject("json")`) e lancia `swapdeck-host-ready`. Si ripete a ogni caricamento.
     */
    fun injectScript(postCall: String): String =
        "window.__swapdeckHost = Object.freeze({ post: function (json) { $postCall } });" +
            "window.dispatchEvent(new Event('$HOST_READY_EVENT'));"

    /**
     * [value] come letterale stringa JavaScript tra doppi apici (ed è anche JSON valido): escape di
     * apici, barra rovesciata, caratteri di controllo, `<`, `>`, `&` (niente `</script>` né `<!--`) e
     * U+2028/U+2029 (fine riga per i vecchi motori JavaScript).
     */
    fun jsStringLiteral(value: String): String = buildString(value.length + 16) {
        append('"')
        for (c in value) {
            when (c) {
                '"' -> append("\\\"")
                '\\' -> append("\\\\")
                '\n' -> append("\\n")
                '\r' -> append("\\r")
                '\t' -> append("\\t")
                '<', '>', '&', '\'', ' ', ' ', '\u007f' -> unicode(c)
                else -> if (c < ' ') unicode(c) else append(c)
            }
        }
        append('"')
    }

    private fun StringBuilder.unicode(c: Char) {
        append("\\u")
        append(c.code.toString(16).padStart(4, '0'))
    }
}
