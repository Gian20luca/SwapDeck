package com.swapdeck.jetbrains

import com.intellij.DynamicBundle
import org.jetbrains.annotations.NonNls
import org.jetbrains.annotations.PropertyKey

@NonNls
const val BUNDLE = "messages.SwapDeckBundle"

/**
 * Testi del plugin: inglese in `SwapDeckBundle.properties`, italiano in `SwapDeckBundle_it.properties`.
 * Per ora la lingua è quella dell'IDE; la scelta delle preferenze di SwapDeck arriva con le
 * impostazioni (sottotask 10).
 */
object SwapDeckBundle : DynamicBundle(BUNDLE) {
    @JvmStatic
    fun message(@PropertyKey(resourceBundle = BUNDLE) key: String, vararg params: Any): String =
        getMessage(key, *params)
}
