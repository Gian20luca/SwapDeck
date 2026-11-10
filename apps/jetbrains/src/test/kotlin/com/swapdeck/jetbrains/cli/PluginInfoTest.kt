package com.swapdeck.jetbrains.cli

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test
import java.nio.file.Path

class PluginInfoTest {
    @Test
    fun `cartella del plugin dal jar in lib`() {
        val plugin = Path.of("plugins", "swapdeck-jetbrains").toAbsolutePath()
        val jar = plugin.resolve("lib").resolve("swapdeck-jetbrains-1.0.0.jar").toString()
        assertEquals(plugin, PluginInfo.directoryOf(jar))
        assertEquals(CliPaths.bundled(plugin), CliPaths.bundled(PluginInfo.directoryOf(jar)!!))
    }

    @Test
    fun `niente cartella fuori da uno zip installato`() {
        assertNull(PluginInfo.directoryOf(null))
        assertNull(PluginInfo.directoryOf(""))
        // Classi sciolte (test, Gradle) o jar fuori da `lib/`.
        assertNull(PluginInfo.directoryOf(Path.of("build", "classes", "kotlin", "main").toAbsolutePath().toString()))
        assertNull(PluginInfo.directoryOf(Path.of("x", "swapdeck.jar").toAbsolutePath().toString()))
    }

    @Test
    fun `versione da plugin properties`() {
        val expected = Regex("""(?m)^pluginVersion\s*=\s*(\S+)""")
            .find(Path.of("gradle.properties").toFile().readText())!!.groupValues[1]
        assertEquals(expected, PluginInfo.version)
    }
}
