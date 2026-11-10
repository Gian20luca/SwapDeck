package com.swapdeck.jetbrains.actions

import com.swapdeck.jetbrains.bridge.FakeCli
import com.swapdeck.jetbrains.bridge.obj
import kotlinx.serialization.json.JsonElement
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import java.util.concurrent.CompletableFuture

/** «Avvia/Ferma motore»: prima accensione con dialogo, annullamento, solo osservazione. */
class EngineToggleTest {
    private val cli = FakeCli()
    private val toggle = EngineToggle(cli)
    private var asked = 0

    private fun respond(vararg answers: Pair<List<String>, String>) {
        val map = answers.toMap()
        cli.respond = { args, _ -> CompletableFuture.completedFuture<JsonElement>(obj(map[args] ?: error("inatteso: $args"))) }
    }

    private fun ask(answer: FirstEnableAnswer?): () -> FirstEnableAnswer? = {
        asked++
        answer
    }

    @Test
    fun `già confermato - acceso senza dialogo`() {
        respond(listOf("engine", "enable") to ENABLED_ON)
        assertEquals(EngineToggleResult.Changed(enabled = true, dryRun = false), toggle.enable(ask(null)))
        assertEquals(0, asked)
        assertEquals(listOf(listOf("engine", "enable")), cli.args())
    }

    @Test
    fun `prima accensione - Accendi passa --confirm`() {
        respond(listOf("engine", "enable") to NEEDS, listOf("engine", "enable", "--confirm") to ENABLED_ON)
        assertEquals(EngineToggleResult.Changed(true, false), toggle.enable(ask(FirstEnableAnswer.ON)))
        assertEquals(1, asked)
        assertEquals(listOf(listOf("engine", "enable"), listOf("engine", "enable", "--confirm")), cli.args())
    }

    @Test
    fun `prima accensione - Solo osservazione passa --observe`() {
        respond(listOf("engine", "enable") to NEEDS, listOf("engine", "enable", "--confirm", "--observe") to ENABLED_OBSERVE)
        assertEquals(EngineToggleResult.Changed(true, true), toggle.enable(ask(FirstEnableAnswer.OBSERVE)))
    }

    @Test
    fun `prima accensione - Annulla non cambia nulla`() {
        respond(listOf("engine", "enable") to NEEDS)
        assertEquals(EngineToggleResult.Unchanged, toggle.enable(ask(null)))
        assertEquals(1, cli.calls.size)
    }

    @Test
    fun `risposta inattesa - nulla cambia`() {
        respond(listOf("engine", "enable") to """{"kind":"boh"}""")
        assertEquals(EngineToggleResult.Unchanged, toggle.enable(ask(FirstEnableAnswer.ON)))
    }

    @Test
    fun `spegnimento`() {
        respond(listOf("engine", "disable") to """{"rules":{"enabled":false,"dryRun":true},"view":null,"conflicts":[]}""")
        assertEquals(EngineToggleResult.Changed(enabled = false, dryRun = true), toggle.disable())
    }

    @Test
    fun `scelta del dialogo - Annulla è ogni altro indice`() {
        assertEquals(FirstEnableAnswer.ON, EngineToggle.answerOf(0))
        assertEquals(FirstEnableAnswer.OBSERVE, EngineToggle.answerOf(1))
        assertEquals(null, EngineToggle.answerOf(2))
        assertEquals(null, EngineToggle.answerOf(-1))
    }

    private companion object {
        const val NEEDS = """{"kind":"needsConfirmation"}"""
        const val ENABLED_ON = """{"kind":"enabled","state":{"rules":{"enabled":true,"dryRun":false},"view":null,"conflicts":[]}}"""
        const val ENABLED_OBSERVE = """{"kind":"enabled","state":{"rules":{"enabled":true,"dryRun":true},"view":null,"conflicts":[]}}"""
    }
}
