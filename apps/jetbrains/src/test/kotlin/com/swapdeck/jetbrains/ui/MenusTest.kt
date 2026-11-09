package com.swapdeck.jetbrains.ui

import com.swapdeck.jetbrains.SwapDeckMessages
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/** Voci dei popup «Cambia account» e «Cambia mappa». */
class MenusTest {
    private val en = SwapDeckMessages.translate("en")

    private fun obj(text: String): JsonObject = Json.parseToJsonElement(text).jsonObject

    private fun list(vararg accounts: String) = obj("""{"activeAccountNumber":1,"accounts":[${accounts.joinToString(",")}]}""")

    private val active = """{"number":1,"email":"uno@example.com","active":true,"usage":{"fiveHour":{"pct":12.4},"sevenDay":{"pct":40.6}}}"""
    private val other = """{"number":2,"alias":"casa","email":"due@example.com","active":false,
        "usage":null,"lastGoodUsage":{"fiveHour":{"pct":150},"sevenDay":{"pct":-3}}}"""
    private val disabled = """{"number":3,"email":"tre@example.com","active":false,"disabled":true,"usage":null}"""

    @Test
    fun `nessun account - solo il messaggio`() {
        assertEquals(SwitchMenu.NoAccounts, SwitchMenu.of(list(), en))
    }

    @Test
    fun `un account - niente strategie, l'attivo non si sceglie`() {
        val items = (SwitchMenu.of(list(active), en) as SwitchMenu.Items).items
        assertEquals(1, items.size)
        assertEquals("#1 #1  5h 12% · 7d 41% · active  uno@example.com", items[0].text)
        assertFalse(items[0].selectable)
        assertTrue(items[0].checked)
        assertEquals(null, items[0].separatorAbove)
    }

    @Test
    fun `due o più account - strategie, separatore, disattivati visibili ma non selezionabili`() {
        val items = (SwitchMenu.of(list(active, other, disabled), en) as SwitchMenu.Items).items
        assertEquals(listOf("Best account", "Next available"), items.take(2).map { it.text })
        assertEquals(listOf(SwitchTarget.Strategy("best"), SwitchTarget.Strategy("next-available")), items.take(2).map { it.value })
        assertEquals("Accounts", items[2].separatorAbove)
        assertEquals(listOf(null, null), items.drop(3).map { it.separatorAbove })
        assertEquals("#2 casa  5h 100% · 7d 0%  due@example.com", items[3].text)
        assertEquals(SwitchTarget.Account(2), items[3].value)
        assertTrue(items[3].selectable)
        assertEquals("#3 #3  5h — · 7d — · disabled  tre@example.com", items[4].text)
        assertFalse(items[4].selectable)
    }

    @Test
    fun `lista non disponibile - solo le strategie`() {
        val items = (SwitchMenu.of(null, en) as SwitchMenu.Items).items
        assertEquals(listOf("best", "next-available"), items.map { (it.value as SwitchTarget.Strategy).name })
    }

    @Test
    fun `azioni di switch`() {
        assertEquals(obj("""{"kind":"switch","target":4}"""), SwitchTarget.Account(4).action())
        assertEquals(obj("""{"kind":"switch","strategy":"best"}"""), SwitchTarget.Strategy("best").action())
    }

    @Test
    fun `mappe - personali, poi i preset, l'attiva spuntata`() {
        val profiles = obj(
            """{"activeId":"p2","profiles":[
                {"id":"preset-a","name":"Bilanciato","preset":"balanced","rules":{}},
                {"id":"p1","name":"Lavoro","rules":{}},
                {"id":"p2","name":"Casa‮","rules":{}},
                {"id":"preset-b","name":"Risparmio","preset":"saver","rules":{}}]}""",
        )
        val items = MapMenu.of(profiles, en)
        assertEquals(listOf("p1", "p2", "preset-a", "preset-b"), items.map { it.value })
        assertEquals(listOf("Lavoro", "Casa\\u202E (active)", "Bilanciato", "Risparmio"), items.map { it.text })
        assertEquals(listOf(null, null, "preset", null), items.map { it.separatorAbove })
        assertEquals(listOf(false, true, false, false), items.map { it.checked })
        assertEquals("Lavoro", MapMenu.nameOf(profiles, "p1"))
        assertEquals("sconosciuta", MapMenu.nameOf(profiles, "sconosciuta"))
        assertEquals(emptyList<MenuItem<String>>(), MapMenu.of(obj("""{"activeId":"x"}"""), en))
        assertEquals(JsonPrimitive("p2"), profiles["activeId"])
    }

    @Test
    fun `email accorciate e percentuali come l'interfaccia`() {
        assertEquals("corta@example.com", shortEmail("corta@example.com"))
        assertEquals("lungonomeut…@example.com", shortEmail("lungonomeutente@example.com"))
        assertEquals("senzachiocc…", shortEmail("senzachiocciolaperniente", 12))
        assertEquals(0, displayPct(-1.0))
        assertEquals(100, displayPct(250.0))
        assertEquals(13, displayPct(12.5))
        assertEquals(0, displayPct(Double.NaN))
    }
}
