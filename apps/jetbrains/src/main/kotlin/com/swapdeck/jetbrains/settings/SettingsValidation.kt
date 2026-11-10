package com.swapdeck.jetbrains.settings

import java.io.File

/** Esito della validazione della pagina Impostazioni. */
sealed interface SettingsCheck {
    data class Valid(val snapshot: SettingsSnapshot) : SettingsCheck

    /** Chiave del testo d'errore in `SwapDeckBundle` e parametri. */
    data class Invalid(val key: String, val params: List<Any> = emptyList()) : SettingsCheck
}

/**
 * Regole della pagina Impostazioni, senza IDE: intervallo intero tra 15 e 3600 secondi, `cliPath`
 * vuoto (binario incluso) o assoluto, soglie: fino a 5 interi tra 1 e 100 separati da virgole o
 * spazi (vuoto = soglie predefinite della CLI, 80 e 95).
 */
object SettingsValidation {
    const val MAX_THRESHOLDS = 5

    private val SEPARATORS = Regex("[,;\\s]+")

    fun check(
        interval: String,
        cliPath: String,
        engineEnabled: Boolean,
        thresholds: String,
        isAbsolute: (String) -> Boolean = { File(it).isAbsolute },
    ): SettingsCheck {
        val seconds = interval.trim().toIntOrNull()
        if (seconds == null || seconds < SettingsSnapshot.MIN_INTERVAL_SECONDS || seconds > SettingsSnapshot.MAX_INTERVAL_SECONDS) {
            return SettingsCheck.Invalid(
                "settings.error.interval",
                listOf(SettingsSnapshot.MIN_INTERVAL_SECONDS, SettingsSnapshot.MAX_INTERVAL_SECONDS),
            )
        }
        val path = cliPath.trim()
        if (path.isNotEmpty() && (path.any { it.isISOControl() } || !isAbsolute(path))) {
            return SettingsCheck.Invalid("settings.error.cliPath")
        }
        val values = parseThresholds(thresholds) ?: return SettingsCheck.Invalid("settings.error.thresholds", listOf(MAX_THRESHOLDS))
        return SettingsCheck.Valid(SettingsSnapshot(seconds, path, engineEnabled, values))
    }

    /** Soglie dal testo; `null` se non valide (non intere, fuori da 1..100 o più di [MAX_THRESHOLDS]). */
    fun parseThresholds(text: String): List<Int>? {
        val parts = text.trim().split(SEPARATORS).filter { it.isNotEmpty() }
        if (parts.size > MAX_THRESHOLDS) return null
        return parts.map { part -> part.toIntOrNull()?.takeIf { it in 1..100 } ?: return null }
    }

    fun thresholdsText(values: List<Int>): String = values.joinToString(", ")
}
