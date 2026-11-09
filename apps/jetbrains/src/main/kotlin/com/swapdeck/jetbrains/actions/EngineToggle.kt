package com.swapdeck.jetbrains.actions

import com.swapdeck.jetbrains.cli.CliCaller
import com.swapdeck.jetbrains.cli.runBlocking
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** Risposta al dialogo della prima accensione; `null` = annullato. */
enum class FirstEnableAnswer { ON, OBSERVE }

/** Esito di «Avvia/Ferma motore». */
sealed interface EngineToggleResult {
    /** Annullato nel dialogo o risposta inattesa: niente è cambiato. */
    data object Unchanged : EngineToggleResult

    /** Regole scritte: [enabled] e solo osservazione ([dryRun]). */
    data class Changed(val enabled: Boolean, val dryRun: Boolean) : EngineToggleResult
}

/**
 * «Avvia/Ferma motore» come «Accendi» e «Motore acceso» della scheda Regole (e di VS Code):
 * `engine.enabled` delle regole condivise, scritto dalla CLI sotto il lock della config. Alla prima
 * accensione la CLI risponde `needsConfirmation`: [enable] chiede ad `ask` (Accendi / Solo
 * osservazione / Annulla) e poi passa `--confirm` (e `--observe`). Bloccante: dai thread del plugin.
 */
class EngineToggle(private val cli: CliCaller) {
    fun enable(ask: () -> FirstEnableAnswer?): EngineToggleResult {
        var result = cli.runBlocking(listOf("engine", "enable")) as? JsonObject ?: return EngineToggleResult.Unchanged
        if (result.kind() == "needsConfirmation") {
            val answer = ask() ?: return EngineToggleResult.Unchanged
            result = cli.runBlocking(confirmArgs(answer)) as? JsonObject ?: return EngineToggleResult.Unchanged
        }
        if (result.kind() != "enabled") return EngineToggleResult.Unchanged
        val rules = (result["state"] as? JsonObject)?.get("rules") as? JsonObject
        return EngineToggleResult.Changed(enabled = true, dryRun = rules.flag("dryRun"))
    }

    fun disable(): EngineToggleResult {
        val rules = (cli.runBlocking(listOf("engine", "disable")) as? JsonObject)?.get("rules") as? JsonObject
        return EngineToggleResult.Changed(enabled = false, dryRun = rules.flag("dryRun"))
    }

    companion object {
        /** Argomenti dopo il dialogo della prima accensione. */
        fun confirmArgs(answer: FirstEnableAnswer): List<String> =
            if (answer == FirstEnableAnswer.OBSERVE) {
                listOf("engine", "enable", "--confirm", "--observe")
            } else {
                listOf("engine", "enable", "--confirm")
            }

        /** Scelta del dialogo (indice del pulsante: 0 Accendi, 1 Solo osservazione, altro Annulla). */
        fun answerOf(choice: Int): FirstEnableAnswer? = when (choice) {
            0 -> FirstEnableAnswer.ON
            1 -> FirstEnableAnswer.OBSERVE
            else -> null
        }

        private fun JsonObject.kind(): String? = (this["kind"] as? JsonPrimitive)?.takeIf { it.isString }?.content

        private fun JsonObject?.flag(key: String): Boolean =
            (this?.get(key) as? JsonPrimitive)?.takeIf { !it.isString }?.content == "true"
    }
}
