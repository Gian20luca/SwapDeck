package com.swapdeck.jetbrains.ui

import com.intellij.notification.NotificationType
import com.swapdeck.jetbrains.engine.SwapDeckNotification
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

/** Gravità del Rust → tipo di notifica dell'IDE, lingua ed escape. */
class NotificationsTest {
    private fun notification(severity: String) = SwapDeckNotification.parse(
        Json.parseToJsonElement(
            """{"kind":"threshold","severity":"$severity","key":"k","account":2,
               "title":{"it":"Soglia <b>superata</b>","en":"Threshold & co"},
               "body":{"it":"Account \"casa\"\nal 95%","en":"Account <img src=x> at 95%"}}""",
        ).jsonObject,
    )!!

    @Test
    fun `gravità`() {
        assertEquals(NotificationType.ERROR, SwapDeckNotifications.typeOf("critical"))
        assertEquals(NotificationType.WARNING, SwapDeckNotifications.typeOf("warning"))
        assertEquals(NotificationType.INFORMATION, SwapDeckNotifications.typeOf("info"))
        assertEquals(NotificationType.INFORMATION, SwapDeckNotifications.typeOf("sconosciuta"))
    }

    @Test
    fun `testi nella lingua scelta, sotto escape`() {
        assertEquals(
            ShownNotification("Soglia &lt;b&gt;superata&lt;/b&gt;", "Account &quot;casa&quot;<br>al 95%", NotificationType.WARNING),
            SwapDeckNotifications.present(notification("warning"), "it"),
        )
        assertEquals(
            ShownNotification("Threshold &amp; co", "Account &lt;img src=x&gt; at 95%", NotificationType.ERROR),
            SwapDeckNotifications.present(notification("critical"), "en"),
        )
        assertEquals(ShownNotification("", "a &lt;b&gt;", NotificationType.INFORMATION), SwapDeckNotifications.plain("a <b>", NotificationType.INFORMATION))
    }
}
