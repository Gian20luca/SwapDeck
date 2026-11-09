package com.swapdeck.jetbrains.cli

import java.io.IOException
import java.nio.file.Files
import java.nio.file.InvalidPathException
import java.nio.file.LinkOption
import java.nio.file.Path
import java.nio.file.attribute.PosixFilePermissions

/** Dove sta `swapdeck-cli`: quello incluso nel plugin (`bin/`) o il `cliPath` delle impostazioni. */
object CliPaths {
    val isWindows: Boolean = System.getProperty("os.name").orEmpty().startsWith("Windows")

    fun executableName(windows: Boolean = isWindows): String = if (windows) "swapdeck-cli.exe" else "swapdeck-cli"

    /** `swapdeck-cli` incluso nello zip del plugin. */
    fun bundled(pluginDir: Path, windows: Boolean = isWindows): Path =
        pluginDir.resolve("bin").resolve(executableName(windows))

    /**
     * Percorso effettivo: l'impostazione se è un percorso assoluto di un file eseguibile, altrimenti
     * quello incluso nel plugin.
     */
    fun resolve(setting: String?, pluginDir: Path, windows: Boolean = isWindows): Path {
        val custom = setting?.trim().orEmpty()
        if (custom.isNotEmpty()) {
            val path = try {
                Path.of(custom)
            } catch (_: InvalidPathException) {
                null
            }
            if (path != null && path.isAbsolute && Files.isRegularFile(path) && Files.isExecutable(path)) return path
        }
        return bundled(pluginDir, windows)
    }

    /**
     * Su Linux e macOS il bit di esecuzione può perdersi nell'estrazione dello zip: lo ripristina
     * (0755), ma solo su un file regolare (un link non si segue) e mai su un percorso scelto
     * dall'utente. Restituisce `false` se il file non c'è o i permessi non si possono cambiare.
     */
    fun ensureExecutable(file: Path, windows: Boolean = isWindows): Boolean {
        if (windows) return true
        if (!Files.isRegularFile(file, LinkOption.NOFOLLOW_LINKS)) return false
        if (Files.isExecutable(file)) return true
        return try {
            Files.setPosixFilePermissions(file, PosixFilePermissions.fromString("rwxr-xr-x"))
            true
        } catch (_: IOException) {
            false
        } catch (_: UnsupportedOperationException) {
            false
        }
    }
}
