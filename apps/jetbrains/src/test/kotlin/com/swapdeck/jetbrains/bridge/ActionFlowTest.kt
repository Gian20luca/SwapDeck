package com.swapdeck.jetbrains.bridge

import com.swapdeck.jetbrains.cli.CliTimeouts
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

/** Passi comuni di un'azione, condivisi da ponte e azioni dell'IDE (cambio di account). */
class ActionFlowTest {
    private val cli = FakeCli()
    private val flow = ActionFlow(cli)
    private val action = obj("""{"kind":"switch","target":2}""")

    @Test
    fun `conferma con l'attivo della lista, senza lista niente --active`() {
        cli.responses["confirm"] = obj("""{"kind":"simple"}""")
        assertEquals(Confirmation.Simple, flow.required(action, fixtureList("6")))
        cli.responses["confirm"] = obj("""{"kind":"none"}""")
        assertNull(flow.required(action, null))
        assertEquals(listOf(listOf("confirm", "-", "--active", "6"), listOf("confirm", "-")), cli.args())
        assertEquals(action.toString(), cli.calls[0].options.stdin)
    }

    @Test
    fun `azione da stdin con il timeout lungo`() {
        cli.responses["action"] = obj("""{"switch":{"switched":true}}""")
        assertEquals(obj("""{"switch":{"switched":true}}"""), flow.run(action))
        assertEquals(listOf("action", "-"), cli.calls.single().args)
        assertEquals(CliTimeouts.ACTION_MS, cli.calls.single().options.timeoutMs)
    }

    @Test
    fun `esito - cambio non riuscito come avviso`() {
        val ui = FakeUi()
        ActionFlow.showOutcome(obj("""{"switch":{"switched":false,"message":"nessun account"}}"""), ui)
        assertEquals(listOf("SwapDeck: nessun account"), ui.warnings.toList())
    }
}
