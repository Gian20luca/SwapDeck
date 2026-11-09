package com.swapdeck.jetbrains.bridge

import com.swapdeck.jetbrains.protocol.HostProtocol
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long

/**
 * Regole di validazione delle richieste del pannello, lette da `protocol.json` (le stesse di
 * `packages/host/src/rules.ts` per VS Code). Il pannello non è fidato: il ponte le applica prima di
 * chiamare `swapdeck-cli`, che rivalida tutto. Le lunghezze sono in unità UTF-16, come in JavaScript.
 *
 * Le espressioni regolari valgono sull'intero testo ([Regex.matches]): quelle di `protocol.json`
 * sono ancorate (`^…$`), e così una riga a capo finale non passa come succederebbe con `$` di Java.
 */
class BridgeRules(json: JsonObject) {
    /** Metodi che il pannello può chiedere (`requestMethods`). */
    val methods: Set<String> = json.strings("requestMethods").toSet()

    /** `kind` ammessi per `runAction` e `confirmationFor`. */
    val actionKinds: Set<String> = json.strings("actionKinds").toSet()

    /** Argomenti di `claude` ammessi per l'azione `run`. */
    val safeClaudeArgs: Set<String> = json.strings("safeClaudeArgs").toSet()

    private val limits = json.getValue("limits").jsonObject
    private val pick = limits.getValue("pick").jsonObject
    private val usage = limits.getValue("usage").jsonObject

    /** Chiave della config di cswap (`getConfigValue`). */
    val configKey: Regex = Regex(limits.text("configKeyPattern"))

    /** Versione di claude-swap (`dismissCswapUpdate`). */
    val version: Regex = Regex(limits.text("versionPattern"))

    /** Codice di «Rifai login»: una riga, al massimo questa lunghezza. */
    val reloginCodeMaxLength: Int = limits.getValue("reloginCodeMaxLength").jsonPrimitive.int

    /** Testo massimo copiato negli appunti su richiesta del pannello. */
    val copyTextMaxLength: Int = limits.getValue("copyTextMaxLength").jsonPrimitive.int

    /** Campi di testo delle opzioni dei selettori (`title`, `defaultPath`). */
    val pickTextFields: List<String> = pick.strings("textFields")
    val pickTextMaxLength: Int = pick.getValue("textMaxLength").jsonPrimitive.int
    val pickMaxFilters: Int = pick.getValue("maxFilters").jsonPrimitive.int
    val pickFilterNameMaxLength: Int = pick.getValue("filterNameMaxLength").jsonPrimitive.int
    val pickExtension: Regex = Regex(pick.text("extensionPattern"))

    /** Finestre e raggruppamenti di `getUsageBreakdown`. */
    val usageWindows: List<String> = usage.strings("windows")
    val usageGroups: List<String> = usage.strings("groupBy")

    /** Periodo di `getHistory` senza `sinceSeconds`. */
    val historySeconds: Long =
        json.getValue("defaults").jsonObject.getValue("historySeconds").jsonPrimitive.long

    companion object {
        /** Regole di `protocol.json` nel jar del plugin. */
        val DEFAULT: BridgeRules by lazy { BridgeRules(HostProtocol.json) }

        private fun JsonObject.strings(key: String): List<String> =
            getValue(key).jsonArray.map { it.jsonPrimitive.content }

        private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
    }
}
