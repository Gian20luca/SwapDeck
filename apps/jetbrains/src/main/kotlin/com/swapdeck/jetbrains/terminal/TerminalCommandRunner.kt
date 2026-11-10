package com.swapdeck.jetbrains.terminal

import com.intellij.openapi.project.Project
import com.swapdeck.jetbrains.bridge.CommandRunner
import org.jetbrains.plugins.terminal.TerminalTabState
import org.jetbrains.plugins.terminal.TerminalToolWindowManager

/**
 * `run` in una nuova scheda della tool window Terminal: il comando è il processo della scheda
 * (`myShellCommand`, niente shell in mezzo), come `createTerminal({shellPath, shellArgs})` di VS
 * Code. Classe usata solo da `swapdeck-terminal.xml`: senza il plugin Terminal non si carica.
 */
class TerminalCommandRunner : CommandRunner {
    override fun run(project: Project, program: String, args: List<String>): Boolean {
        val manager = TerminalToolWindowManager.getInstance(project)
        val tab = TerminalTabState().apply {
            myTabName = TAB_NAME
            myIsUserDefinedTabTitle = true
            myShellCommand = listOf(program) + args
        }
        manager.createNewSession(manager.terminalRunner, tab)
        return true
    }

    private companion object {
        const val TAB_NAME = "cswap run"
    }
}
