package com.swapdeck.jetbrains

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
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
}
