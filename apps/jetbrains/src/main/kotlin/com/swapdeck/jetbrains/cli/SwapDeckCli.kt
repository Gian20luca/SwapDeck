package com.swapdeck.jetbrains.cli

import com.intellij.ide.plugins.PluginManagerCore
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.diagnostic.logger
import com.intellij.openapi.extensions.PluginId
import com.swapdeck.jetbrains.settings.SwapDeckSettings
import kotlinx.serialization.json.JsonElement
import java.nio.file.Path
import java.util.concurrent.CompletableFuture

/**
 * `swapdeck-cli` per tutto il plugin (servizio dell'applicazione): percorso dalle impostazioni
 * (`cliPath`) o quello incluso nel plugin, a ogni comando (un cambio vale subito).
 */
@Service(Service.Level.APP)
class SwapDeckCli : CliCaller {
    private val pluginDir: Path? =
        PluginManagerCore.getPlugin(PluginId.getId(PLUGIN_ID))?.pluginPath

    private val runner = CliRunner(executable = ::executable)

    init {
        // Bit di esecuzione perso nell'estrazione dello zip: solo sul binario incluso.
        pluginDir?.let { dir ->
            val bundled = CliPaths.bundled(dir)
            if (!CliPaths.ensureExecutable(bundled)) LOG.info("swapdeck-cli incluso non presente o non eseguibile: $bundled")
        }
    }

    /** Percorso di `swapdeck-cli` da usare ora. */
    fun executable(): Path {
        val dir = pluginDir ?: Path.of(System.getProperty("user.dir"))
        return CliPaths.resolve(SwapDeckSettings.getInstance().snapshot.cliPath, dir)
    }

    override fun run(args: List<String>, options: CliOptions): CompletableFuture<JsonElement> = runner.run(args, options)

    companion object {
        const val PLUGIN_ID = "com.swapdeck.jetbrains"
        private val LOG = logger<SwapDeckCli>()

        fun getInstance(): SwapDeckCli = service()
    }
}
