package com.swapdeck.jetbrains.toolwindow

import com.swapdeck.jetbrains.state.CliState
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/** Instradamento dei messaggi di un pannello con un finto ponte e un finto browser. */
class PanelConnectionTest {
    private val received = mutableListOf<String>()
    private val scripts = mutableListOf<String>()
    private var bridgeSend: ((JsonObject) -> Unit)? = null
    private var view = ViewSettings("it", "dark")
    private var state: CliState? = CliState(Json.parseToJsonElement("""{"list":{"accounts":[]},"engine":{}}""").jsonObject)

    /** Come `BridgeSession.receive`: prende solo le `request`. */
    private val connection = PanelConnection(
        receive = { text, send ->
            received += text
            bridgeSend = send
            PanelMessages.typeOf(text) == "request"
        },
        execute = { scripts += it },
        snapshot = { view to state },
    )

    /** Tipi dei messaggi consegnati, letti dagli script. */
    private fun delivered(): List<String> = scripts.map { script ->
        val literal = script.substringAfter("detail: ").removeSuffix(" }));")
        val message = Json.parseToJsonElement(Json.decodeFromString(String.serializer(), literal)).jsonObject
        (message["type"] as JsonPrimitive).content
    }

    private fun message(type: String) = buildJsonObject { put("type", JsonPrimitive(type)) }

    @Test
    fun `al ready view, state ed engine`() {
        assertFalse(connection.isLive)
        connection.fromPage("""{"type":"ready"}""")
        assertTrue(connection.isLive)
        assertEquals(listOf("view", "state", "engine"), delivered())
        assertTrue(scripts[0].contains("\\\"locale\\\":\\\"it\\\""))
    }

    @Test
    fun `senza stato solo view`() {
        state = null
        connection.fromPage("""{"type":"ready"}""")
        assertEquals(listOf("view"), delivered())
    }

    @Test
    fun `prima del ready nulla arriva al pannello`() {
        connection.post(message("configChanged"))
        assertEquals(emptyList<String>(), scripts)
        connection.fromPage("""{"type":"ready"}""")
        connection.post(message("configChanged"))
        assertEquals(listOf("view", "state", "engine", "configChanged"), delivered())
    }

    @Test
    fun `le richieste vanno al ponte e le risposte al pannello`() {
        connection.fromPage("""{"type":"ready"}""")
        scripts.clear()
        connection.fromPage("""{"type":"request","id":1,"method":"getStatus","params":{}}""")
        assertEquals(2, received.size)
        bridgeSend!!(message("response"))
        assertEquals(listOf("response"), delivered())
    }

    @Test
    fun `messaggi non validi ignorati`() {
        for (text in listOf("", "x", "[]", """{"type":"other"}""", """{"type":7}""")) connection.fromPage(text)
        assertEquals(emptyList<String>(), scripts)
        assertFalse(connection.isLive)
    }

    @Test
    fun `una pagina che si ricarica aspetta il nuovo ready`() {
        connection.fromPage("""{"type":"ready"}""")
        connection.pageLoading()
        scripts.clear()
        connection.post(message("configChanged"))
        assertEquals(emptyList<String>(), scripts)
        view = ViewSettings("en", "light")
        connection.fromPage("""{"type":"ready"}""")
        assertEquals(listOf("view", "state", "engine"), delivered())
        assertTrue(scripts[0].contains("\\\"theme\\\":\\\"light\\\""))
    }

    @Test
    fun `dopo la chiusura nessuno script né messaggio al ponte`() {
        connection.fromPage("""{"type":"ready"}""")
        connection.close()
        scripts.clear()
        received.clear()
        connection.post(message("configChanged"))
        bridgeSend!!(message("response"))
        connection.fromPage("""{"type":"ready"}""")
        connection.fromPage("""{"type":"request","id":2,"method":"getStatus","params":{}}""")
        assertEquals(emptyList<String>(), scripts)
        assertEquals(emptyList<String>(), received)
        assertFalse(connection.isLive)
    }
}
