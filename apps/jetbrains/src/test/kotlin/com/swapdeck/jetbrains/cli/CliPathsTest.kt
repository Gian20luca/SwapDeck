package com.swapdeck.jetbrains.cli

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.condition.DisabledOnOs
import org.junit.jupiter.api.condition.OS
import org.junit.jupiter.api.io.TempDir
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.attribute.PosixFilePermissions

@DisabledOnOs(OS.WINDOWS)
class CliPathsTest {
    @TempDir
    lateinit var dir: Path

    private fun file(name: String, perms: String): Path {
        val f = dir.resolve(name)
        Files.writeString(f, "#!/bin/sh\n")
        Files.setPosixFilePermissions(f, PosixFilePermissions.fromString(perms))
        return f
    }

    @Test
    fun `binario incluso per piattaforma`() {
        val plugin = Path.of("/opt/plugin")
        assertEquals(plugin.resolve("bin/swapdeck-cli"), CliPaths.bundled(plugin, windows = false))
        assertEquals(plugin.resolve("bin").resolve("swapdeck-cli.exe"), CliPaths.bundled(plugin, windows = true))
    }

    @Test
    fun `cliPath assoluto ed eseguibile, altrimenti quello incluso`() {
        val plugin = dir.resolve("plugin")
        val bundled = CliPaths.bundled(plugin, windows = false)
        val custom = file("custom", "rwxr-xr-x")
        val notExec = file("noexec", "rw-r--r--")
        assertEquals(custom, CliPaths.resolve("  $custom  ", plugin, windows = false))
        assertEquals(bundled, CliPaths.resolve("", plugin, windows = false))
        assertEquals(bundled, CliPaths.resolve(null, plugin, windows = false))
        assertEquals(bundled, CliPaths.resolve("relativo/swapdeck-cli", plugin, windows = false))
        assertEquals(bundled, CliPaths.resolve(notExec.toString(), plugin, windows = false))
        assertEquals(bundled, CliPaths.resolve(dir.resolve("manca").toString(), plugin, windows = false))
        assertEquals(bundled, CliPaths.resolve(dir.toString(), plugin, windows = false), "una cartella non vale")
    }

    @Test
    fun `ripristina il bit di esecuzione`() {
        val f = file("swapdeck-cli", "rw-r--r--")
        assertTrue(CliPaths.ensureExecutable(f, windows = false))
        assertTrue(Files.isExecutable(f))
        assertEquals("rwxr-xr-x", PosixFilePermissions.toString(Files.getPosixFilePermissions(f)))
    }

    @Test
    fun `non segue i link e non inventa file`() {
        val target = file("target", "rw-r--r--")
        val link = Files.createSymbolicLink(dir.resolve("link"), target)
        assertFalse(CliPaths.ensureExecutable(link, windows = false))
        assertFalse(Files.isExecutable(target))
        assertFalse(CliPaths.ensureExecutable(dir.resolve("manca"), windows = false))
    }
}
