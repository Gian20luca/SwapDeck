package com.swapdeck.jetbrains.toolwindow

import com.swapdeck.jetbrains.cli.CliCaller
import com.swapdeck.jetbrains.cli.CliOptions
import kotlinx.serialization.json.JsonObject
import java.util.concurrent.CompletableFuture
import java.util.concurrent.atomic.AtomicLong

/**
 * Lingua e tema dei pannelli, senza IDE. La lingua è quella delle preferenze di SwapDeck
 * (`preferences get` → `language`), altrimenti quella dell'IDE; il tema viene dal Look and Feel.
 * [changed] riceve le nuove impostazioni quando cambiano, fuori da ogni lock.
 */
class ViewSettingsTracker(
    private val cli: CliCaller,
    private val ideLanguage: () -> String,
    private val bright: () -> Boolean,
    private val changed: (ViewSettings) -> Unit,
) {
    private val lock = Any()
    private var preferred: String? = null

    /** Impostazioni dell'ultimo avviso (all'inizio quelle alla creazione). */
    private var last: ViewSettings = compute()
    private val generation = AtomicLong()

    /** Impostazioni attuali. */
    fun current(): ViewSettings = synchronized(lock) { compute() }

    /**
     * Rilegge la lingua delle preferenze (dopo `setPreferences` o un cambio della config). Vale solo
     * l'ultima rilettura avviata; con un errore della CLI resta la lingua attuale, come VS Code.
     */
    fun reload(): CompletableFuture<Unit> {
        val id = generation.incrementAndGet()
        return cli.run(listOf("preferences", "get"), CliOptions()).handle { json, error ->
            if (error != null || id != generation.get()) return@handle
            val locale = ViewSettings.preferredLocaleOf((json as? JsonObject)?.get("language"))
            update { preferred = locale }
        }
    }

    /** Il Look and Feel dell'IDE è cambiato: nuovo tema, se diverso. */
    fun lookAndFeelChanged() = update {}

    private fun update(change: () -> Unit) {
        val next = synchronized(lock) {
            val before = last
            change()
            val next = compute()
            last = next
            next.takeIf { it != before }
        }
        next?.let(changed)
    }

    private fun compute() = ViewSettings(ViewSettings.localeOf(preferred, ideLanguage()), ViewSettings.themeOf(bright()))
}
