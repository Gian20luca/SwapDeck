package com.swapdeck.jetbrains.bridge

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import java.math.BigDecimal

/**
 * Controlli dei parametri del pannello (non fidato), uguali a quelli di VS Code: un valore non
 * valido è `invalidUsage` e la CLI non parte.
 */
internal class BridgeValidation(private val rules: BridgeRules) {
    /** Azione con `kind` noto; per `run` solo argomenti di `claude` dell'elenco chiuso. */
    fun action(value: JsonElement?): JsonObject {
        val action = value as? JsonObject ?: throw invalid("azione non valida")
        val kind = action.textOrNull("kind")
        if (kind == null || kind !in rules.actionKinds) throw invalid("azione sconosciuta")
        if (kind == "run") {
            val options = action["options"] as? JsonObject
            if (options != null && options.containsKey("claudeArgs")) {
                val args = options["claudeArgs"] as? JsonArray
                val ok = args != null &&
                    args.all { it is JsonPrimitive && it.isString && it.content in rules.safeClaudeArgs }
                if (!ok) throw invalid("argomenti di claude non ammessi")
            }
        }
        return action
    }

    /** Opzioni del selettore: solo i testi ammessi e filtri con estensioni semplici. */
    fun pick(params: JsonObject): PickOptions {
        val texts = mutableMapOf<String, String>()
        for (key in rules.pickTextFields) {
            val value = params[key] ?: continue
            val text = value.textOrNull()
            if (text == null || text.length > rules.pickTextMaxLength) throw invalid("$key non valido")
            texts[key] = text
        }
        val filters = params["filters"]?.let { raw ->
            val list = raw as? JsonArray
            if (list == null || list.size > rules.pickMaxFilters) throw invalid("filtri non validi")
            list.map { item ->
                val filter = item as? JsonObject
                val name = filter?.textOrNull("name")
                val extensions = filter?.get("extensions") as? JsonArray
                if (name == null || name.length > rules.pickFilterNameMaxLength || extensions == null) {
                    throw invalid("filtri non validi")
                }
                val exts = extensions.map { e ->
                    val ext = e.textOrNull()
                    if (ext == null || !rules.pickExtension.matches(ext)) throw invalid("filtri non validi")
                    ext
                }
                PickFilter(name, exts)
            }
        }
        return PickOptions(title = texts["title"], defaultPath = texts["defaultPath"], filters = filters)
    }

    /** Oggetto JSON semplice (la validazione completa la fa il Rust). */
    fun plainObject(value: JsonElement?, name: String): JsonObject =
        value as? JsonObject ?: throw invalid("$name non valide")

    /**
     * Intero facoltativo, non negativo e «sicuro» come `Number.isSafeInteger` (anche `3.0`);
     * assente o `null` = `null`.
     */
    fun optionalInt(value: JsonElement?, name: String): Long? {
        if (value == null || value is JsonNull) return null
        val primitive = value as? JsonPrimitive ?: throw invalid("$name non valido")
        return safeInteger(primitive)?.takeIf { it >= 0 } ?: throw invalid("$name non valido")
    }

    /** Stringa che rispetta [pattern] (tutto il testo). */
    fun matching(value: JsonElement?, pattern: Regex, message: String): String {
        val text = value.textOrNull()
        if (text == null || !pattern.matches(text)) throw invalid(message)
        return text
    }

    /** Valore fra quelli ammessi. */
    fun oneOf(value: JsonElement?, allowed: Collection<String>, message: String): String {
        val text = value.textOrNull()
        if (text == null || text !in allowed) throw invalid(message)
        return text
    }

    companion object {
        private val MAX_SAFE = BigDecimal(9_007_199_254_740_991L)

        /**
         * Lunghezza massima del testo di un numero. Un intero sicuro ne usa al più 17 (anche `-9007199254740991.0`
         * o `9.007199254740991e15` stanno larghi); oltre si rifiuta prima di `BigDecimal`, che su testi lunghi
         * costa un tempo quadratico (un milione di cifre ≈ 10 s sul thread che riceve i messaggi).
         */
        const val MAX_NUMBER_LENGTH = 32

        /** Intero sicuro di JavaScript (|n| ≤ 2^53 - 1, senza parte decimale), altrimenti `null`. */
        fun safeInteger(primitive: JsonPrimitive): Long? {
            if (primitive.isString || primitive.booleanOrNull != null || primitive is JsonNull) return null
            if (primitive.content.length > MAX_NUMBER_LENGTH) return null
            val number = primitive.content.toBigDecimalOrNull() ?: return null
            if (number.abs() > MAX_SAFE) return null
            if (number.signum() != 0 && number.stripTrailingZeros().scale() > 0) return null
            return number.toLong()
        }
    }
}

/** Stringa JSON, altrimenti `null`. */
internal fun JsonElement?.textOrNull(): String? = (this as? JsonPrimitive)?.takeIf { it.isString }?.content

internal fun JsonObject.textOrNull(key: String): String? = this[key].textOrNull()

/** Lunghezza massima di un valore del pannello nel dialogo di conferma (oltre: troncato con `…`). */
const val DESCRIBE_MAX_VALUE_LENGTH = 120

/**
 * Valore del pannello (non fidato) come si mostra in un dialogo: caratteri di controllo (Cc),
 * di formato (Cf: U+200E/F, U+202A–U+202E, U+2066–U+2069…) e separatori di riga o paragrafo
 * diventano `\uXXXX` visibili, così un a capo o un cambio di direzione non travestono il
 * bersaglio; oltre [max] caratteri si tronca con `…` (mai a metà di una coppia surrogata).
 */
fun displaySafe(value: String, max: Int = DESCRIBE_MAX_VALUE_LENGTH): String {
    val out = StringBuilder()
    var i = 0
    while (i < value.length) {
        if (out.length >= max) {
            out.append('…')
            break
        }
        val cp = value.codePointAt(i)
        when (Character.getType(cp).toByte()) {
            Character.CONTROL, Character.FORMAT, Character.LINE_SEPARATOR, Character.PARAGRAPH_SEPARATOR ->
                out.append(String.format("\\u%04X", cp))
            else -> out.appendCodePoint(cp)
        }
        i += Character.charCount(cp)
    }
    return out.toString()
}

/**
 * Descrizione breve dell'azione per il dialogo di conferma (mai il token di `addToken`). I valori
 * vengono dal pannello: passano da [displaySafe].
 */
fun describeAction(action: JsonObject): String {
    fun field(key: String): String = displaySafe(
        when (val value = action[key]) {
            null, JsonNull -> "?"
            is JsonPrimitive -> value.content
            else -> value.toString()
        },
    )
    return when (val kind = action.textOrNull("kind") ?: "?") {
        "remove", "disable", "enable", "setAlias" -> "$kind #${field("account")}"
        "swap" -> "swap #${field("a")} ↔ #${field("b")}"
        "move" -> "move #${field("account")} → ${field("slot")}"
        "map" -> "map #${field("account")} ${field("path")}"
        "unmap", "import" -> "$kind ${field("path")}"
        "export" -> {
            val account = action["account"]
            val who = if (account == null || account is JsonNull) "" else " #${field("account")}"
            "export$who → ${field("path")}"
        }
        "unclaimedPurge" -> "unclaimed --purge ${field("id")}"
        "configSet", "configUnset" -> "$kind ${field("key")}"
        else -> displaySafe(kind)
    }
}
