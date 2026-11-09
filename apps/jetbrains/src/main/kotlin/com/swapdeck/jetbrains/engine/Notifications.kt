package com.swapdeck.jetbrains.engine

import com.intellij.util.messages.Topic
import com.swapdeck.jetbrains.cli.long
import com.swapdeck.jetbrains.cli.string
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject

/** Testo in italiano e inglese (`LocalizedText` del Rust). */
data class LocalizedText(val it: String, val en: String) {
    fun of(italian: Boolean): String = if (italian) it else en

    companion object {
        fun parse(value: Any?): LocalizedText? {
            val obj = value as? JsonObject ?: return null
            return LocalizedText(obj.string("it") ?: return null, obj.string("en") ?: return null)
        }
    }
}

/**
 * Notifica decisa dal Rust (`swapdeck-core::notify`): il plugin la mostra soltanto. [severity] è
 * `info`, `warning` o `critical`; [json] è l'oggetto intero.
 */
data class SwapDeckNotification(
    val kind: String,
    val severity: String,
    val key: String,
    val account: Long?,
    val title: LocalizedText,
    val body: LocalizedText,
    val json: JsonObject,
) {
    companion object {
        fun parse(value: Any?): SwapDeckNotification? {
            val n = value as? JsonObject ?: return null
            return SwapDeckNotification(
                kind = n.string("kind") ?: return null,
                severity = n.string("severity") ?: return null,
                key = n.string("key") ?: "",
                account = n.long("account"),
                title = LocalizedText.parse(n["title"]) ?: return null,
                body = LocalizedText.parse(n["body"]) ?: return null,
                json = n,
            )
        }
    }
}

/** Notifiche da mostrare (bus dell'applicazione); il gruppo di notifiche dell'IDE arriva col sottotask 10. */
fun interface SwapDeckNotificationListener {
    fun notify(notification: SwapDeckNotification)

    companion object {
        @JvmField
        @Topic.AppLevel
        val TOPIC: Topic<SwapDeckNotificationListener> =
            Topic(SwapDeckNotificationListener::class.java, Topic.BroadcastDirection.NONE)
    }
}

/**
 * Mittente unico delle notifiche. Con il motore del plugin titolare del lock arrivano come eventi
 * JSON dalle righe di `headless`; senza motore vivo da `notifications poll`, che ricontrolla il
 * lock; con un motore vivo altrove (app desktop, VS Code, altra IDE) manda lui.
 */
object NotificationRules {
    private val json = Json { ignoreUnknownKeys = true }

    /** Notifica da una riga dell'output del motore; `null` per le righe di log. */
    fun parseEngineEvent(line: String): SwapDeckNotification? {
        if (!line.startsWith("{")) return null
        val value = try {
            json.parseToJsonElement(line) as? JsonObject
        } catch (_: Exception) {
            null
        } ?: return null
        if (value.string("event") != "notification") return null
        return SwapDeckNotification.parse(value["notification"])
    }

    /**
     * Le notifiche si chiedono solo se nessun motore è vivo: né il nostro (che le manda come
     * eventi) né un altro ([lockHeldElsewhere] dallo stato; `null` = non noto, decide la CLI).
     */
    fun shouldPoll(localEngineRunning: Boolean, lockHeldElsewhere: Boolean?): Boolean =
        !localEngineRunning && lockHeldElsewhere != true

    /** Notifiche da mostrare dall'esito di `notifications poll`: solo se il mittente è chi chiama. */
    fun fromPoll(poll: JsonObject): List<SwapDeckNotification> {
        if (poll.string("sender") != "caller") return emptyList()
        val list = poll["notifications"] as? JsonArray ?: return emptyList()
        return list.mapNotNull { SwapDeckNotification.parse(it) }
    }
}
