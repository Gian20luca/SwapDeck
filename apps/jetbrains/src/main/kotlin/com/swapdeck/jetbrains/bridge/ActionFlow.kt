package com.swapdeck.jetbrains.bridge

import com.swapdeck.jetbrains.cli.CliCaller
import com.swapdeck.jetbrains.cli.CliException
import com.swapdeck.jetbrains.cli.CliOptions
import com.swapdeck.jetbrains.cli.CliTimeouts
import com.swapdeck.jetbrains.cli.runBlocking
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull

/**
 * Passi comuni di un'azione di cswap (`CswapAction`), usati dal ponte (`runAction`) e dalle azioni
 * dell'IDE (cambio di account): conferma dovuta decisa dal Rust (`confirm -`), esecuzione
 * (`action -`), terminale e avvisi dell'esito. Bloccanti: dai thread del plugin.
 */
class ActionFlow(private val cli: CliCaller) {
    /** `confirm -` con l'account attivo se la lista è nota ([list] = `ListResponse`). */
    fun confirmationFor(action: JsonObject, list: JsonObject?): JsonElement {
        val args = mutableListOf("confirm", "-")
        if (list != null) {
            val active = list["activeAccountNumber"]
            args += listOf("--active", if (active is JsonPrimitive && active !is JsonNull) active.content else "none")
        }
        return cli.runBlocking(args, CliOptions(stdin = action.toString()))
    }

    /** Conferma dovuta; `null` = nessuna. */
    fun required(action: JsonObject, list: JsonObject?): Confirmation? = confirmationOf(confirmationFor(action, list))

    /** `action -` (6 minuti). */
    fun run(action: JsonObject): JsonElement =
        cli.runBlocking(listOf("action", "-"), CliOptions(stdin = action.toString(), timeoutMs = CliTimeouts.ACTION_MS))

    companion object {
        /** `null` = nessuna conferma; `kind` sconosciuto o senza parola = conferma con parola impossibile. */
        fun confirmationOf(value: JsonElement): Confirmation? {
            val obj = value as? JsonObject ?: throw CliException("failed", "confirm: risposta non valida")
            return when (obj.textOrNull("kind")) {
                "none" -> null
                "simple" -> Confirmation.Simple
                else -> Confirmation.Typed(obj.textOrNull("word") ?: throw CliException("failed", "confirm: risposta non valida"))
            }
        }

        /** Terminale per `run`, avvisi dell'esito (`ActionOutcome`) e cambio di account non riuscito. */
        fun showOutcome(outcome: JsonObject, ui: BridgeUi) {
            (outcome["command"] as? JsonObject)?.let { command ->
                val program = command.textOrNull("program")
                val args = (command["args"] as? JsonArray)?.mapNotNull { it.textOrNull() } ?: emptyList()
                if (program != null) ui.openCommand(program, args)
            }
            outcome.textOrNull("warning")?.let { ui.warn("SwapDeck: $it") }
            (outcome["switch"] as? JsonObject)?.let { sw ->
                val switched = (sw["switched"] as? JsonPrimitive)?.takeIf { !it.isString }?.booleanOrNull
                if (switched == false) ui.warn("SwapDeck: ${sw.textOrNull("message") ?: ""}")
            }
        }
    }
}
