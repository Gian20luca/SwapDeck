package com.swapdeck.jetbrains.ui

import com.swapdeck.jetbrains.SwapDeckMessages
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

/** Testo, tooltip e colore del widget da `IdeStatus` di esempio (come `statusBar.test.ts` di VS Code). */
class StatusFormatterTest {
    private val en = SwapDeckMessages.translate("en")
    private val it = SwapDeckMessages.translate("it")
    private val time: (Long) -> String = { ms -> "T$ms" }

    private fun status(content: String, level: String = "ok", engine: String = ENGINE_OFF): JsonObject =
        Json.parseToJsonElement(
            """{"level":"$level","content":$content,"engine":$engine,"updatedAtMs":42,"thresholds":{"warn":80,"danger":95}}""",
        ).jsonObject

    private fun window(pct: String, countdown: String? = null, expected: String? = null, level: String = "ok") =
        """{"pct":$pct,"level":"$level","countdown":${countdown?.let { "\"$it\"" } ?: "null"},"resetsAt":null,
           "resetsInSecs":null,"expectedPct":${expected ?: "null"}}"""

    private fun account(alias: String? = "lavoro", lastKnown: Boolean = false, five: String = window("12", "1h 44m"), seven: String = window("40", expected = "35")) =
        """{"kind":"account","account":{"number":6,"alias":${alias?.let { "\"$it\"" } ?: "null"},"shortEmail":"mar…@example.com"},
           "fiveHour":$five,"sevenDay":$seven,"lastKnown":$lastKnown}"""

    @Test
    fun `prima della lettura - caricamento`() {
        val view = StatusFormatter.of(null, en, time)
        assertEquals("SwapDeck", view.text)
        assertEquals(listOf("SwapDeck: loading…"), view.tooltip)
        assertEquals(StatusIcon.LOADING, view.icon)
        assertNull(view.background)
    }

    @Test
    fun `account - testo, tooltip completo, motore qui con mappa`() {
        val engine = """{"holder":"here","pid":10,"switching":true,"dryRun":false,"activeMap":"Lavoro"}"""
        val view = StatusFormatter.of(status(account(), engine = engine), en, time)
        assertEquals("lavoro 5h 12% · 7d 40%", view.text)
        assertEquals(
            listOf(
                "SwapDeck — lavoro (mar…@example.com)",
                "5h: 12% (resets in 1h 44m)",
                "7d: 40% (expected 35%)",
                "Engine: running in this IDE · automatic switching on",
                "Map: Lavoro",
                "Updated at T42",
            ),
            view.tooltip,
        )
        assertEquals(StatusIcon.ACCOUNT, view.icon)
    }

    @Test
    fun `account - ultimo dato noto, senza alias, senza reset e atteso, motore altrove`() {
        val engine = """{"holder":"other","pid":12345,"switching":false,"dryRun":false,"activeMap":null}"""
        val content = account(alias = null, lastKnown = true, five = window("null"), seven = window("99.0", level = "danger"))
        val view = StatusFormatter.of(status(content, level = "danger", engine = engine), it, time)
        assertEquals("#6 5h ~— · 7d ~99%", view.text)
        assertEquals(
            listOf(
                "SwapDeck — #6 (mar…@example.com)",
                "5h: —",
                "7g: 99%",
                "Ultimo dato noto (consumi attuali non disponibili)",
                "Motore: attivo in un'altra IDE, finestra o app (PID 12345) · cambio automatico spento",
                "Aggiornato alle T42",
            ),
            view.tooltip,
        )
        assertEquals("error", view.background)
    }

    @Test
    fun `motore altrove senza pid e fermo`() {
        val other = """{"holder":"other","pid":null,"switching":true,"dryRun":false,"activeMap":null}"""
        assertEquals(
            "Engine: running in another IDE, window or app · automatic switching on",
            StatusFormatter.of(status(account(), engine = other), en, time).tooltip[3],
        )
        assertEquals("Engine: stopped · automatic switching off", StatusFormatter.of(status(account()), en, time).tooltip[3])
    }

    @Test
    fun `nessun account attivo`() {
        val view = StatusFormatter.of(status("""{"kind":"noAccount"}""", level = "unknown"), en, time)
        assertEquals("SwapDeck: —", view.text)
        assertEquals(listOf("No active account", "Engine: stopped · automatic switching off", "Updated at T42"), view.tooltip)
        assertNull(view.background)
    }

    @Test
    fun `errore - avviso col messaggio di cswap o generico`() {
        val view = StatusFormatter.of(
            status("""{"kind":"error","error":{"kind":"notInstalled","message":"cswap non trovato"}}""", level = "error"),
            it,
            time,
        )
        assertEquals("SwapDeck", view.text)
        assertEquals(StatusIcon.WARNING, view.icon)
        assertEquals(listOf("SwapDeck: cswap non disponibile", "cswap non trovato", "Motore: fermo · cambio automatico spento"), view.tooltip)
        assertEquals("error", view.background)
        val unknown = StatusFormatter.of(status("""{"kind":"error","error":null}""", level = "error"), en, time)
        assertEquals("unknown error", unknown.tooltip[1])
        // Forma sconosciuta: come un errore.
        assertEquals(StatusIcon.WARNING, StatusFormatter.of(status("""{"kind":"boh"}"""), en, time).icon)
    }

    @Test
    fun `colore per livello`() {
        val levels = mapOf("ok" to null, "unknown" to null, "warn" to "warning", "danger" to "error", "error" to "error")
        for ((level, background) in levels) {
            assertEquals(background, StatusFormatter.of(status(account(), level = level), en, time).background, level)
        }
    }

    @Test
    fun `alias non fidato - controlli visibili, troncato, mai HTML nel componente`() {
        val view = StatusFormatter.of(status(account(alias = "a\\u202Eb")), en, time)
        assertEquals("a\\u202Eb 5h 12% · 7d 40%", view.text)
        assertEquals(40 + 1, StatusFormatter.accountLabel("x".repeat(100), 1).length)
        assertEquals(" <html><img src=x>", swingPlain("<html><img src=x>"))
        assertEquals("<b>", swingPlain("<b>"))
    }

    private companion object {
        const val ENGINE_OFF = """{"holder":"off","pid":null,"switching":false,"dryRun":false,"activeMap":null}"""
    }
}
