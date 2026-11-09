package com.swapdeck.jetbrains.toolwindow

import com.intellij.DynamicBundle
import com.intellij.ide.ui.LafManagerListener
import com.intellij.openapi.Disposable
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.util.Disposer
import com.intellij.ui.JBColor
import com.swapdeck.jetbrains.SwapDeckLifecycle
import com.swapdeck.jetbrains.bridge.SwapDeckViewListener
import com.swapdeck.jetbrains.cli.SwapDeckCli
import com.swapdeck.jetbrains.state.SwapDeckStateListener
import java.util.concurrent.CopyOnWriteArrayList

/**
 * Lingua e tema di tutti i pannelli (servizio dell'applicazione: una sola rilettura delle preferenze
 * per cambio, anche con più progetti aperti). Rilegge la lingua a `preferencesChanged` e
 * `configChanged`, il tema a ogni cambio del Look and Feel; avvisa i pannelli solo se cambia qualcosa.
 */
@Service(Service.Level.APP)
class ViewSettingsService : Disposable {
    init {
        SwapDeckLifecycle.ensure()
    }

    private val listeners = CopyOnWriteArrayList<(ViewSettings) -> Unit>()

    private val tracker = ViewSettingsTracker(
        cli = SwapDeckCli.getInstance(),
        ideLanguage = { DynamicBundle.getLocale().language },
        bright = { JBColor.isBright() },
        changed = { settings -> listeners.forEach { it(settings) } },
    )

    init {
        val connection = ApplicationManager.getApplication().messageBus.connect(this)
        connection.subscribe(LafManagerListener.TOPIC, LafManagerListener { tracker.lookAndFeelChanged() })
        connection.subscribe(SwapDeckViewListener.TOPIC, SwapDeckViewListener { tracker.reload() })
        connection.subscribe(
            SwapDeckStateListener.TOPIC,
            object : SwapDeckStateListener {
                override fun configChanged() {
                    tracker.reload()
                }
            },
        )
        tracker.reload()
    }

    /** Lingua e tema attuali. */
    fun current(): ViewSettings = tracker.current()

    /** [listener] riceve le nuove impostazioni a ogni cambio, finché [parent] non si smaltisce. */
    fun addListener(parent: Disposable, listener: (ViewSettings) -> Unit) {
        listeners += listener
        Disposer.register(parent) { listeners -= listener }
    }

    override fun dispose() = listeners.clear()

    companion object {
        fun getInstance(): ViewSettingsService = service()
    }
}
