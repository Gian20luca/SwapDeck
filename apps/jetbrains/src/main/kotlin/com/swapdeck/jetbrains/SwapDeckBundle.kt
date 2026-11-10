package com.swapdeck.jetbrains

import com.intellij.DynamicBundle
import com.intellij.openapi.components.serviceIfCreated
import com.swapdeck.jetbrains.toolwindow.ViewSettings
import com.swapdeck.jetbrains.toolwindow.ViewSettingsService
import org.jetbrains.annotations.NonNls
import org.jetbrains.annotations.PropertyKey
import java.text.MessageFormat
import java.util.Locale
import java.util.ResourceBundle
import java.util.concurrent.ConcurrentHashMap

@NonNls
const val BUNDLE = "messages.SwapDeckBundle"

/** Traduzione con segnaposto `{0}`, `{1}`… (testi del plugin in una lingua già scelta). */
fun interface Translate {
    fun message(key: String, vararg params: Any): String
}

/**
 * Testi del plugin in una lingua esplicita (`it` o `en`), senza IDE: `ResourceBundle` con la `Locale`
 * scelta e senza ripiego sulla lingua della JVM (con il ripiego standard una richiesta `en` su una
 * JVM italiana darebbe l'italiano). Come `DynamicBundle`: `MessageFormat` solo con parametri, quindi
 * gli apostrofi vanno raddoppiati (`''`) solo nei testi con segnaposto. I numeri arrivano come testo
 * (niente separatori delle migliaia nei PID).
 */
object SwapDeckMessages {
    private val control = ResourceBundle.Control.getNoFallbackControl(ResourceBundle.Control.FORMAT_PROPERTIES)
    private val bundles = ConcurrentHashMap<String, ResourceBundle>()

    private fun localeOf(language: String): Locale = if (language == "it") Locale.ITALIAN else Locale.ROOT

    fun bundle(language: String): ResourceBundle = bundles.computeIfAbsent(language) {
        ResourceBundle.getBundle(BUNDLE, localeOf(it), SwapDeckMessages::class.java.classLoader, control)
    }

    fun get(language: String, key: String, vararg params: Any): String {
        val value = bundle(language).getString(key)
        if (params.isEmpty()) return value
        val args = params.map { if (it is Number) it.toString() else it }.toTypedArray()
        return MessageFormat(value, localeOf(language)).format(args)
    }

    fun translate(language: String): Translate = Translate { key, params -> get(language, key, *params) }
}

/**
 * Testi del plugin: inglese in `SwapDeckBundle.properties`, italiano in `SwapDeckBundle_it.properties`.
 *
 * - [message] (barra di stato, azioni a runtime, dialoghi, notifiche) segue la lingua delle
 *   preferenze di SwapDeck come il pannello ([ViewSettingsService], se già creato), altrimenti
 *   quella dell'IDE.
 * - I testi registrati in `plugin.xml` (nomi delle azioni, pagina delle impostazioni, gruppo di
 *   notifiche, nome del widget) li legge la piattaforma con `DynamicBundle`: seguono sempre la
 *   lingua dell'IDE, che non si cambia a runtime.
 */
object SwapDeckBundle : DynamicBundle(BUNDLE) {
    /** `it` o `en`: preferenza di SwapDeck, altrimenti lingua dell'IDE. */
    fun language(): String =
        runCatching { serviceIfCreated<ViewSettingsService>()?.current()?.locale }.getOrNull()
            ?: ViewSettings.localeOf(null, DynamicBundle.getLocale().language)

    @JvmStatic
    fun message(@PropertyKey(resourceBundle = BUNDLE) key: String, vararg params: Any): String =
        SwapDeckMessages.get(language(), key, *params)

    /** [Translate] nella lingua attuale. */
    fun translate(): Translate = SwapDeckMessages.translate(language())
}
