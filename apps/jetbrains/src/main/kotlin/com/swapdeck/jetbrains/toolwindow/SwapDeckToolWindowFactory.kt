package com.swapdeck.jetbrains.toolwindow

import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.Project
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.ui.components.JBLabel
import com.intellij.ui.content.ContentFactory
import com.swapdeck.jetbrains.SwapDeckBundle
import javax.swing.SwingConstants

/** Tool window «SwapDeck». Per ora solo un segnaposto: il pannello JCEF arriva con il sottotask 9. */
class SwapDeckToolWindowFactory : ToolWindowFactory, DumbAware {
    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val label = JBLabel(SwapDeckBundle.message("toolwindow.placeholder"), SwingConstants.CENTER)
        val content = ContentFactory.getInstance().createContent(label, null, false)
        toolWindow.contentManager.addContent(content)
    }
}
