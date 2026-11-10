package com.swapdeck.jetbrains.bridge

import com.intellij.openapi.extensions.ExtensionPointName
import com.intellij.openapi.project.Project

/**
 * Apre il comando di `run` (`outcome.command`) in un terminale dell'IDE. Punto di estensione
 * `com.swapdeck.jetbrains.commandRunner`: l'unica implementazione (`terminal/TerminalCommandRunner`)
 * è dichiarata in `swapdeck-terminal.xml`, caricato solo se c'è il plugin Terminal. Senza, il
 * ponte mostra il comando da copiare.
 */
interface CommandRunner {
    /** Dall'EDT. `program` e `args` vanno al processo così come sono (niente shell). `false` = non riuscito. */
    fun run(project: Project, program: String, args: List<String>): Boolean

    companion object {
        val EP: ExtensionPointName<CommandRunner> = ExtensionPointName.create("com.swapdeck.jetbrains.commandRunner")
    }
}

/**
 * Riga del comando da mostrare (e copiare): argomenti vuoti, con spazi o virgolette tra virgolette
 * doppie (con `\"` dentro); le barre rovesciate restano com'erano (percorsi di Windows).
 */
fun commandLine(program: String, args: List<String>): String =
    (listOf(program) + args).joinToString(" ") { arg ->
        if (arg.isNotEmpty() && arg.none { it.isWhitespace() || it == '"' || it == '\'' }) {
            arg
        } else {
            "\"" + arg.replace("\"", "\\\"") + "\""
        }
    }
