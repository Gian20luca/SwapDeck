package com.swapdeck.jetbrains.ui

import com.intellij.icons.AllIcons
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.application.ModalityState
import com.intellij.openapi.components.serviceIfCreated
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.IconLoader
import com.intellij.openapi.wm.CustomStatusBarWidget
import com.intellij.openapi.wm.StatusBar
import com.intellij.openapi.wm.StatusBarWidget
import com.intellij.openapi.wm.StatusBarWidgetFactory
import com.intellij.openapi.wm.ToolWindowManager
import com.intellij.ui.AnimatedIcon
import com.intellij.ui.components.JBLabel
import com.intellij.util.text.DateFormatUtil
import com.intellij.util.ui.JBUI
import com.swapdeck.jetbrains.IdeText
import com.swapdeck.jetbrains.SwapDeckBundle
import com.swapdeck.jetbrains.SwapDeckThreads
import com.swapdeck.jetbrains.state.IdeStatus
import com.swapdeck.jetbrains.state.StateService
import com.swapdeck.jetbrains.state.SwapDeckStateListener
import com.swapdeck.jetbrains.toolwindow.ViewSettingsService
import java.awt.Cursor
import java.awt.event.MouseAdapter
import java.awt.event.MouseEvent
import java.util.concurrent.RejectedExecutionException
import javax.swing.JComponent
import javax.swing.SwingUtilities

/** Widget «SwapDeck» nella barra di stato, uno per finestra (progetto). */
class SwapDeckStatusWidgetFactory : StatusBarWidgetFactory {
    override fun getId(): String = SwapDeckStatusWidget.ID

    override fun getDisplayName(): String = SwapDeckBundle.message("status.widget.name")

    override fun createWidget(project: Project): StatusBarWidget = SwapDeckStatusWidget(project)
}

/**
 * Account attivo con 5h e 7d, colore per livello e tooltip dal riepilogo `ide status`
 * ([SwapDeckStateListener.statusChanged]); testo e righe da [StatusFormatter]. Clic: apre la tool
 * window «SwapDeck» (mai un cambio di account). Si ridisegna anche al cambio di lingua.
 */
class SwapDeckStatusWidget(private val project: Project) : CustomStatusBarWidget {
    private val label = JBLabel().apply {
        border = JBUI.Borders.empty(0, 6)
        cursor = Cursor.getPredefinedCursor(Cursor.HAND_CURSOR)
    }

    @Volatile
    private var disposed = false

    /** Solo dall'EDT. */
    private var status: IdeStatus? = null

    init {
        label.addMouseListener(
            object : MouseAdapter() {
                override fun mouseClicked(e: MouseEvent) {
                    if (SwingUtilities.isLeftMouseButton(e)) openToolWindow()
                }
            },
        )
        ApplicationManager.getApplication().messageBus.connect(this).subscribe(
            SwapDeckStateListener.TOPIC,
            object : SwapDeckStateListener {
                override fun statusChanged(status: IdeStatus) = onEdt {
                    this@SwapDeckStatusWidget.status = status
                    render()
                }
            },
        )
        status = serviceIfCreated<StateService>()?.status
        render()
        try {
            // Il servizio della lingua fa una lettura della CLI: lo si crea fuori dall'EDT.
            SwapDeckThreads.io.execute {
                if (!disposed) ViewSettingsService.getInstance().addListener(this) { onEdt { render() } }
            }
        } catch (_: RejectedExecutionException) {
            // Plugin in chiusura.
        }
    }

    override fun ID(): String = ID

    override fun getComponent(): JComponent = label

    override fun install(statusBar: StatusBar) {}

    override fun dispose() {
        disposed = true
    }

    private fun onEdt(block: () -> Unit) {
        ApplicationManager.getApplication().invokeLater({ if (!disposed) block() }, ModalityState.any())
    }

    private fun render() {
        val view = StatusFormatter.of(status?.json, SwapDeckBundle.translate()) { ms -> DateFormatUtil.formatTime(ms) }
        label.text = swingPlain(view.text)
        label.icon = when (view.icon) {
            StatusIcon.LOADING -> AnimatedIcon.Default.INSTANCE
            StatusIcon.WARNING -> AllIcons.General.Warning
            StatusIcon.ACCOUNT -> ICON
        }
        label.toolTipText = "<html>" + view.tooltip.joinToString("<br>") { IdeText.html(it) } + "</html>"
        when (view.background) {
            "error" -> paint(JBUI.CurrentTheme.Banner.ERROR_BACKGROUND)
            "warning" -> paint(JBUI.CurrentTheme.Banner.WARNING_BACKGROUND)
            else -> {
                label.isOpaque = false
                label.background = null
                label.foreground = null
            }
        }
        label.revalidate()
        label.repaint()
    }

    private fun paint(background: java.awt.Color) {
        label.isOpaque = true
        label.background = background
        label.foreground = JBUI.CurrentTheme.Banner.FOREGROUND
    }

    private fun openToolWindow() {
        if (project.isDisposed) return
        ToolWindowManager.getInstance(project).getToolWindow(TOOL_WINDOW_ID)?.activate(null)
    }

    companion object {
        const val ID = "SwapDeckStatus"
        const val TOOL_WINDOW_ID = "SwapDeck"
        private val ICON = IconLoader.getIcon("/icons/swapdeck.svg", SwapDeckStatusWidget::class.java)
    }
}

/**
 * Testo per un componente Swing (`JLabel`, liste) che legge come HTML i testi che iniziano con
 * `<html>`: uno spazio davanti lo lascia testo semplice (alias e nomi delle mappe non sono fidati).
 */
fun swingPlain(text: String): String = if (javax.swing.plaf.basic.BasicHTML.isHTMLString(text)) " $text" else text
