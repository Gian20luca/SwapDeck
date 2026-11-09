package com.swapdeck.jetbrains.ui

import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.project.Project
import com.swapdeck.jetbrains.IdeText
import com.swapdeck.jetbrains.SwapDeckBundle
import com.swapdeck.jetbrains.engine.SwapDeckEngineListener
import com.swapdeck.jetbrains.engine.SwapDeckNotification
import com.swapdeck.jetbrains.engine.SwapDeckNotificationListener

/** Titolo e contenuto (HTML, già sotto escape) e tipo di una notifica dell'IDE. */
data class ShownNotification(val title: String, val content: String, val type: NotificationType)

/** Notifiche del gruppo «SwapDeck»: testi sotto escape (titolo e contenuto sono HTML per l'IDE). */
object SwapDeckNotifications {
    const val GROUP = "SwapDeck"

    /** Gravità del Rust → tipo dell'IDE: `critical` errore, `warning` avviso, il resto informazione. */
    fun typeOf(severity: String): NotificationType = when (severity) {
        "critical" -> NotificationType.ERROR
        "warning" -> NotificationType.WARNING
        else -> NotificationType.INFORMATION
    }

    /** Notifica decisa dal Rust nella lingua [language] (`it` o `en`). */
    fun present(notification: SwapDeckNotification, language: String): ShownNotification {
        val italian = language == "it"
        return ShownNotification(
            title = IdeText.html(notification.title.of(italian)),
            content = IdeText.html(notification.body.of(italian)),
            type = typeOf(notification.severity),
        )
    }

    /** Testo semplice [text] (escape qui) con titolo vuoto. */
    fun plain(text: String, type: NotificationType): ShownNotification = ShownNotification("", IdeText.html(text), type)

    fun show(project: Project?, shown: ShownNotification) {
        NotificationGroupManager.getInstance().getNotificationGroup(GROUP)
            .createNotification(shown.title, shown.content, shown.type)
            .notify(project?.takeUnless { it.isDisposed })
    }

    fun info(project: Project?, text: String) = show(project, plain(text, NotificationType.INFORMATION))

    fun warning(project: Project?, text: String) = show(project, plain(text, NotificationType.WARNING))

    fun error(project: Project?, text: String) = show(project, plain(text, NotificationType.ERROR))
}

/**
 * Notifiche di SwapDeck (motore o `notifications poll`) come notifiche dell'IDE. Ascoltatore
 * dichiarato in `plugin.xml` (`applicationListeners`): la piattaforma lo crea alla prima notifica,
 * anche senza tool window aperta, e non avvia nessun servizio.
 */
class SwapDeckNotificationPresenter : SwapDeckNotificationListener {
    override fun notify(notification: SwapDeckNotification) {
        SwapDeckNotifications.show(null, SwapDeckNotifications.present(notification, SwapDeckBundle.language()))
    }
}

/** Motore fermo da solo: un avviso (uno per avvio, lo garantisce `EngineController`). */
class SwapDeckEngineStopPresenter : SwapDeckEngineListener {
    override fun engineStoppedUnexpectedly(exitCode: Int?) {
        SwapDeckNotifications.warning(null, SwapDeckBundle.message("engine.stopped", exitCode?.toString() ?: "-"))
    }
}
