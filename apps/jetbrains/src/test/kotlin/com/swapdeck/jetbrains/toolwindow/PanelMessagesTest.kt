package com.swapdeck.jetbrains.toolwindow

import com.swapdeck.jetbrains.state.CliState
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/** Messaggi al pannello, script di consegna e letterale JavaScript. */
class PanelMessagesTest {
    private fun obj(text: String) = Json.parseToJsonElement(text).jsonObject

    @Test
    fun `letterale JavaScript sicuro e reversibile`() {
        val tricky = "a\"b\\c'd\n\r\t\u0000\u001f\u007f</script><!-- -->&\u2028\u2029é😀"
        val literal = PanelMessages.jsStringLiteral(tricky)
        assertTrue(literal.startsWith("\"") && literal.endsWith("\""))
        val inner = literal.substring(1, literal.length - 1)
        for (bad in listOf("<", ">", "&", "'", "\n", "\r", "\u2028", "\u2029", "\u0000", "\u007f")) {
            assertFalse(inner.contains(bad), "carattere non escapato: ${bad.map { it.code }}")
        }
        // È anche JSON: si rilegge uguale.
        assertEquals(tricky, Json.decodeFromString(String.serializer(), literal))
        assertEquals("\"\"", PanelMessages.jsStringLiteral(""))
        assertEquals("\"\\u003c/script\\u003e\"", PanelMessages.jsStringLiteral("</script>"))
    }

    @Test
    fun `script di consegna con il JSON come stringa`() {
        val message = obj("""{"type":"state","list":{"name":"</script><script>alert(1)</script>"}}""")
        val script = PanelMessages.dispatchScript(message)
        val prefix = "window.dispatchEvent(new CustomEvent('swapdeck-host', { detail: "
        val suffix = " }));"
        assertTrue(script.startsWith(prefix) && script.endsWith(suffix), script)
        assertFalse(script.contains("</script>"))
        val literal = script.removePrefix(prefix).removeSuffix(suffix)
        assertEquals(message, obj(Json.decodeFromString(String.serializer(), literal)))
    }

    @Test
    fun `script di iniezione`() {
        val script = PanelMessages.injectScript("window.cefQuery_1({request: '' + json});")
        assertEquals(
            "window.__swapdeckHost = Object.freeze({ post: function (json) { window.cefQuery_1({request: '' + json}); } });" +
                "window.dispatchEvent(new Event('swapdeck-host-ready'));",
            script,
        )
    }

    @Test
    fun `lingua e tema`() {
        assertEquals("it", ViewSettings.preferredLocaleOf(JsonPrimitive("it")))
        assertEquals("en", ViewSettings.preferredLocaleOf(JsonPrimitive("en")))
        assertNull(ViewSettings.preferredLocaleOf(JsonPrimitive("de")))
        assertNull(ViewSettings.preferredLocaleOf(JsonPrimitive(1)))
        assertNull(ViewSettings.preferredLocaleOf(JsonNull))
        assertNull(ViewSettings.preferredLocaleOf(null))
        assertEquals("it", ViewSettings.localeOf("it", "en"))
        assertEquals("en", ViewSettings.localeOf("en", "it"))
        assertEquals("it", ViewSettings.localeOf(null, "it"))
        assertEquals("it", ViewSettings.localeOf(null, "IT"))
        assertEquals("en", ViewSettings.localeOf(null, "en"))
        assertEquals("en", ViewSettings.localeOf(null, "de"))
        assertEquals("en", ViewSettings.localeOf(null, ""))
        assertEquals("light", ViewSettings.themeOf(true))
        assertEquals("dark", ViewSettings.themeOf(false))
        assertEquals(
            obj("""{"type":"view","settings":{"locale":"it","theme":"dark"}}"""),
            PanelMessages.view(ViewSettings("it", "dark")),
        )
    }

    @Test
    fun `state ed engine dallo stato della CLI`() {
        val ok = CliState(obj("""{"list":{"accounts":[]},"error":null,"updatedAtMs":1,"engine":{"running":true,"suspended":"limits","suspendDetail":["a","b"]}}"""))
        assertEquals(obj("""{"type":"state","list":{"accounts":[]}}"""), PanelMessages.state(ok))
        assertEquals(
            obj("""{"type":"engine","suspended":{"reason":"limits","detail":["a","b"]}}"""),
            PanelMessages.engine(ok),
        )
        val failed = CliState(obj("""{"list":null,"error":{"kind":"failed","message":"x"},"updatedAtMs":1,"engine":{"running":false}}"""))
        assertNull(PanelMessages.state(failed))
        assertEquals(obj("""{"type":"engine","suspended":null}"""), PanelMessages.engine(failed))
        assertEquals(
            obj("""{"reason":"x","detail":[]}"""),
            PanelMessages.suspensionOf(obj("""{"suspended":"x"}""")),
        )
        assertEquals(JsonNull, PanelMessages.suspensionOf(obj("""{"suspended":3}""")))
        assertEquals(obj("""{"type":"configChanged"}"""), PanelMessages.configChanged)
    }

    @Test
    fun `risposta al ready`() {
        val view = ViewSettings("en", "light")
        assertEquals(listOf("view"), PanelMessages.handshake(view, null).map { it.type() })
        val ok = CliState(obj("""{"list":{"accounts":[]},"engine":{}}"""))
        assertEquals(listOf("view", "state", "engine"), PanelMessages.handshake(view, ok).map { it.type() })
        val failed = CliState(obj("""{"list":null,"engine":{}}"""))
        assertEquals(listOf("view", "engine"), PanelMessages.handshake(view, failed).map { it.type() })
    }

    @Test
    fun `tipo dei messaggi del pannello`() {
        assertEquals("ready", PanelMessages.typeOf("""{"type":"ready"}"""))
        assertEquals("request", PanelMessages.typeOf("""{"type":"request","id":1}"""))
        assertNull(PanelMessages.typeOf("""{"type":1}"""))
        assertNull(PanelMessages.typeOf("""["ready"]"""))
        assertNull(PanelMessages.typeOf("ready"))
        assertNull(PanelMessages.typeOf(""))
    }

    private fun JsonObject.type() = (this["type"] as JsonPrimitive).content
}
