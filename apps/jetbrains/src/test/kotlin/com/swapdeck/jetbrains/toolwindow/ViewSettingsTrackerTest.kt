package com.swapdeck.jetbrains.toolwindow

import com.swapdeck.jetbrains.cli.CliCaller
import com.swapdeck.jetbrains.cli.CliOptions
import com.swapdeck.jetbrains.cli.CswapError
import com.swapdeck.jetbrains.cli.CliException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test
import java.util.concurrent.CompletableFuture

/** Lingua e tema dei pannelli con una finta CLI. */
class ViewSettingsTrackerTest {
    private val calls = mutableListOf<Pair<List<String>, CompletableFuture<JsonElement>>>()
    private val cli = object : CliCaller {
        override fun run(args: List<String>, options: CliOptions): CompletableFuture<JsonElement> =
            CompletableFuture<JsonElement>().also { calls += args to it }
    }
    private var ideLanguage = "en"
    private var bright = false
    private val changes = mutableListOf<ViewSettings>()
    private val tracker = ViewSettingsTracker(cli, { ideLanguage }, { bright }) { changes += it }

    private fun prefs(json: String) = Json.parseToJsonElement(json)

    @Test
    fun `lingua dell'IDE finché le preferenze non dicono altro`() {
        assertEquals(ViewSettings("en", "dark"), tracker.current())
        tracker.reload()
        assertEquals(listOf("preferences", "get"), calls.single().first)
        calls.single().second.complete(prefs("""{"language":"it"}"""))
        assertEquals(ViewSettings("it", "dark"), tracker.current())
        assertEquals(listOf(ViewSettings("it", "dark")), changes)
    }

    @Test
    fun `nessun avviso se non cambia nulla`() {
        tracker.reload()
        calls[0].second.complete(prefs("""{"language":"en"}"""))
        tracker.reload()
        calls[1].second.complete(prefs("""{}"""))
        tracker.lookAndFeelChanged()
        assertEquals(emptyList<ViewSettings>(), changes)
    }

    @Test
    fun `preferenza tolta torna la lingua dell'IDE`() {
        ideLanguage = "it"
        val tracker = ViewSettingsTracker(cli, { ideLanguage }, { bright }) { changes += it }
        tracker.reload()
        calls[0].second.complete(prefs("""{"language":"en"}"""))
        tracker.reload()
        calls[1].second.complete(prefs("""{"language":null}"""))
        assertEquals(listOf(ViewSettings("en", "dark"), ViewSettings("it", "dark")), changes)
    }

    @Test
    fun `errore della CLI lascia la lingua attuale`() {
        tracker.reload()
        calls[0].second.complete(prefs("""{"language":"it"}"""))
        tracker.reload()
        calls[1].second.completeExceptionally(CliException(CswapError.of("failed", "no")))
        assertEquals(ViewSettings("it", "dark"), tracker.current())
        assertEquals(1, changes.size)
    }

    @Test
    fun `vale solo l'ultima rilettura`() {
        tracker.reload()
        tracker.reload()
        calls[1].second.complete(prefs("""{"language":"it"}"""))
        calls[0].second.complete(prefs("""{"language":"en"}"""))
        assertEquals(ViewSettings("it", "dark"), tracker.current())
        assertEquals(listOf(ViewSettings("it", "dark")), changes)
    }

    @Test
    fun `cambio del tema dell'IDE`() {
        bright = true
        // Prima dell'avviso del Look and Feel il valore attuale è già quello nuovo.
        assertEquals(ViewSettings("en", "light"), tracker.current())
        tracker.lookAndFeelChanged()
        tracker.lookAndFeelChanged()
        assertEquals(listOf(ViewSettings("en", "light")), changes)
    }
}
