package com.swapdeck.jetbrains.cli

import com.intellij.openapi.application.PathManager
import java.nio.file.InvalidPathException
import java.nio.file.Path
import java.util.Properties

/**
 * Cartella e versione del plugin senza API interne della piattaforma (`PluginManagerCore.getPlugin`
 * e `PluginManager.findEnabledPlugin` sono `@ApiStatus.Internal` nelle IDE recenti).
 */
object PluginInfo {
    /** Cartella del plugin installato (`<plugin>/lib/<jar>`); `null` fuori da uno zip (test, classi sciolte). */
    val directory: Path? by lazy { directoryOf(PathManager.getJarPathForClass(PluginInfo::class.java)) }

    /** Versione da `swapdeck/plugin.properties`, scritto da Gradle (`pluginInfo`); `?` se manca. */
    val version: String by lazy {
        val props = Properties()
        PluginInfo::class.java.getResourceAsStream("/swapdeck/plugin.properties")?.use(props::load)
        props.getProperty("version")?.takeIf { it.isNotBlank() } ?: "?"
    }

    /** Cartella del plugin dal percorso del suo jar: il jar deve stare in una cartella `lib`. */
    fun directoryOf(jarPath: String?): Path? {
        if (jarPath.isNullOrBlank()) return null
        val jar = try {
            Path.of(jarPath)
        } catch (_: InvalidPathException) {
            return null
        }
        if (!jar.fileName.toString().endsWith(".jar", ignoreCase = true)) return null
        val lib = jar.parent ?: return null
        if (lib.fileName?.toString() != "lib") return null
        return lib.parent
    }
}
