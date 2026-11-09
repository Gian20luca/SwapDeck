package com.swapdeck.jetbrains.toolwindow

import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.diagnostic.logger
import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Disposer
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.components.JBLabel
import com.intellij.ui.content.ContentFactory
import com.intellij.ui.jcef.JBCefApp
import com.intellij.util.ui.JBUI
import com.swapdeck.jetbrains.SwapDeckBundle
import com.swapdeck.jetbrains.state.StateService
import javax.swing.JComponent
import javax.swing.SwingConstants

/**
 * Tool window «SwapDeck»: il pannello in JCEF ([SwapDeckPanel]), smaltito con il suo contenuto.
 * Senza JCEF un messaggio: barra di stato, azioni e notifiche restano.
 */
class SwapDeckToolWindowFactory : ToolWindowFactory, DumbAware {
    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val app = ApplicationManager.getApplication()
        // Dopo l'installazione l'avvio dei progetti già aperti non gira: lo stato parte anche da qui.
        if (!app.isUnitTestMode) app.executeOnPooledThread { StateService.getInstance().start() }

        val panel = if (JBCefApp.isSupported()) {
            try {
                SwapDeckPanel(project)
            } catch (e: Exception) {
                LOG.warn("pannello JCEF non creato", e)
                null
            }
        } else {
            null
        }
        val component: JComponent = panel?.component ?: message()
        val content = ContentFactory.getInstance().createContent(component, null, false)
        if (panel != null) Disposer.register(content, panel)
        toolWindow.contentManager.addContent(content)
    }

    private fun message(): JComponent =
        JBLabel("<html><div style='text-align:center'>${SwapDeckBundle.message("toolwindow.jcef.unsupported")}</div></html>", SwingConstants.CENTER)
            .apply { border = JBUI.Borders.empty(12) }

    companion object {
        private val LOG = logger<SwapDeckToolWindowFactory>()
    }
}
