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

        /** Intero sicuro di JavaScript (|n| ≤ 2^53 - 1, senza parte decimale), altrimenti `null`. */
        fun safeInteger(primitive: JsonPrimitive): Long? {
            if (primitive.isString || primitive.booleanOrNull != null || primitive is JsonNull) return null
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

/** Descrizione breve dell'azione per il dialogo di conferma (mai il token di `addToken`). */
fun describeAction(action: JsonObject): String {
    fun field(key: String): String = when (val value = action[key]) {
        null, JsonNull -> "?"
        is JsonPrimitive -> value.content
        else -> value.toString()
    }
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
        else -> kind
    }
}
