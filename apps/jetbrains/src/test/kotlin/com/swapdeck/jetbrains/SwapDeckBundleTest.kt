package com.swapdeck.jetbrains

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.util.Locale
import java.util.Properties

/** Inglese e italiano hanno le stesse chiavi, tutte con un testo. */
class SwapDeckBundleTest {
    private fun load(name: String): Properties {
        val stream = javaClass.getResourceAsStream("/messages/$name.properties")
            ?: error("$name.properties non trovato")
        return Properties().apply { stream.reader(Charsets.UTF_8).use { load(it) } }
    }

    @Test
    fun `stesse chiavi in inglese e in italiano`() {
        val en = load("SwapDeckBundle")
        val it = load("SwapDeckBundle_it")
        assertTrue(en.isNotEmpty())
        assertEquals(en.stringPropertyNames().sorted(), it.stringPropertyNames().sorted())
    }

    @Test
    fun `nessun testo vuoto`() {
        for (name in listOf("SwapDeckBundle", "SwapDeckBundle_it")) {
            val props = load(name)
            val empty = props.stringPropertyNames().filter { props.getProperty(it).isBlank() }
            assertEquals(emptyList<String>(), empty, "testi vuoti in $name")
        }
    }

    @Test
    fun `apostrofi raddoppiati solo nei testi con segnaposto`() {
        val placeholder = Regex("""\{\d+}""")
        for (name in listOf("SwapDeckBundle", "SwapDeckBundle_it")) {
            val props = load(name)
            for (key in props.stringPropertyNames()) {
                val value = props.getProperty(key)
                val single = Regex("(?<!')'(?!')").containsMatchIn(value)
                if (placeholder.containsMatchIn(value)) {
                    assertTrue(!single, "$name $key: apostrofo da raddoppiare")
                } else {
                    assertTrue(!value.contains("''"), "$name $key: apostrofo doppio senza segnaposto")
                }
            }
        }
    }

    @Test
    fun `lingua esplicita senza ripiego su quella della JVM`() {
        val previous = Locale.getDefault()
        try {
            Locale.setDefault(Locale.ITALIAN)
            assertEquals("Refresh", SwapDeckMessages.get("en", "action.swapdeck.refresh.text"))
            assertEquals("Aggiorna", SwapDeckMessages.get("it", "action.swapdeck.refresh.text"))
            assertEquals("Passare all'account #12345?", SwapDeckMessages.get("it", "switch.confirm.account", 12345L))
            assertEquals("Switch to account #12345?", SwapDeckMessages.translate("en").message("switch.confirm.account", 12345))
            assertEquals("Errore inatteso del plugin (dettagli nel log dell'IDE).", SwapDeckMessages.get("it", "action.internalError"))
        } finally {
            Locale.setDefault(previous)
        }
    }
}
