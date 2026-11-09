package com.swapdeck.jetbrains.state

import com.swapdeck.jetbrains.cli.CliCaller
import com.swapdeck.jetbrains.cli.CliException
import com.swapdeck.jetbrains.cli.CliOptions
import com.swapdeck.jetbrains.cli.CswapError
import com.swapdeck.jetbrains.engine.SwapDeckNotification
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertSame
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.util.Collections
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

/** [StateController] con una finta CLI (nessun processo). */
class StateControllerTest {
    private class Call(val args: List<String>, val options: CliOptions, val result: CompletableFuture<JsonElement>)

    /** Finta CLI: ogni comando resta in sospeso finché il test non lo completa (o risponde subito). */
    private class FakeCli : CliCaller {
        val calls: MutableList<Call> = Collections.synchronizedList(mutableListOf())
        var auto: (List<String>) -> JsonElement? = { null }

        override fun run(args: List<String>, options: CliOptions): CompletableFuture<JsonElement> {
            val future = CompletableFuture<JsonElement>()
            calls += Call(args, options, future)
            auto(args)?.let { future.complete(it) }
            return future
        }

        fun named(vararg prefix: String) = calls.filter { it.args.take(prefix.size) == prefix.toList() }
    }

    private class FakeEngine : EngineView {
        override var running = false
        override var pid: Long? = null
        val lockStates: MutableList<Boolean?> = mutableListOf()

        override fun onLockState(lockHeldElsewhere: Boolean?) {
            lockStates += lockHeldElsewhere
        }
    }

    private class Recorder : StateEvents {
        val states: MutableList<CliState> = Collections.synchronizedList(mutableListOf())
        val statuses: MutableList<IdeStatus> = Collections.synchronizedList(mutableListOf())
        val notes: MutableList<SwapDeckNotification> = Collections.synchronizedList(mutableListOf())
        val warnings: MutableList<String> = Collections.synchronizedList(mutableListOf())

        @Volatile
        var failOnState = false

        override fun stateChanged(state: CliState) {
            states += state
            if (failOnState) throw IllegalStateException("vista rotta")
        }

        override fun statusChanged(status: IdeStatus) {
            statuses += status
        }

        override fun notification(notification: SwapDeckNotification) {
            notes += notification
        }

        override fun warn(message: String) {
            warnings += message
        }
    }

    private val cli = FakeCli()
    private val engine = FakeEngine()
    private val events = Recorder()
    private val reads = mutableListOf<CliState>()
    private var now = 1_000_000L
    private val controller = StateController(cli, engine, { listOf(80, 95) }, events, afterRead = { reads += it }, clock = { now })

    private fun json(text: String) = Json.parseToJsonElement(text)

    private fun state(active: Int, engineRunning: Boolean = false, problem: String? = null): JsonElement = json(
        """{"list":{"activeAccountNumber":$active,"accounts":[]},"error":null,"updatedAtMs":$now,""" +
            """"engine":{"enabled":true,"dryRun":false,"pollIntervalSeconds":60,"running":$engineRunning,"lock":null""" +
            (if (problem != null) ""","problem":"$problem"""" else "") + "}}",
    )

    private fun status(level: String) = json("""{"level":"$level"}""")

    private val note = """{"kind":"threshold","severity":"info","key":"k","title":{"it":"a","en":"b"},"body":{"it":"c","en":"d"}}"""

    private fun active(s: CliState?) = s?.list?.get("activeAccountNumber")?.jsonPrimitive?.long

    @Test
    fun `chi ascolta lo stato fallisce - motore, sincronia e barra aggiornati lo stesso`() {
        events.failOnState = true
        cli.auto = { if (it.first() == "ide") status("ok") else if (it.first() == "state") state(3) else null }
        val value = controller.refresh().get(5, TimeUnit.SECONDS)
        assertEquals(3L, active(value))
        assertEquals(1, reads.size, "afterRead chiamato")
        assertEquals(listOf<Boolean?>(false), engine.lockStates, "onLockState chiamato")
        assertEquals(listOf("ok"), events.statuses.map { it.level })
        assertTrue(events.warnings.any { it.startsWith("stateChanged:") })

        val applied = controller.afterAction(json("""{"list":{"activeAccountNumber":9,"accounts":[]}}""").jsonObject)
        assertEquals(9L, active(applied.get(5, TimeUnit.SECONDS)))
    }

    @Test
    fun `letture concorrenti condividono una sola lettura`() {
        cli.auto = { if (it.first() == "ide") status("ok") else null }
        val first = controller.refresh()
        val second = controller.refresh()
        assertSame(first, second)
        assertEquals(1, cli.named("state").size)
        cli.named("state").single().result.complete(state(3))
        val value = first.get(5, TimeUnit.SECONDS)
        assertEquals(3L, active(value))
        assertEquals(1, events.states.size)
        assertEquals(listOf("ok"), events.statuses.map { it.level })
        assertEquals(1, reads.size)

        // Finita la lettura, la prossima ne avvia un'altra.
        controller.refresh()
        assertEquals(2, cli.named("state").size)
    }

    @Test
    fun `una lettura partita prima di un'azione non sovrascrive l'esito`() {
        cli.auto = { if (it.first() == "ide") status("ok") else null }
        // Stato iniziale.
        val initial = controller.refresh()
        cli.named("state")[0].result.complete(state(1))
        initial.get(5, TimeUnit.SECONDS)

        val stale = controller.refresh()
        val outcome = json("""{"list":{"activeAccountNumber":7,"accounts":[]}}""").jsonObject
        val applied = controller.afterAction(outcome).get(5, TimeUnit.SECONDS)
        assertEquals(7L, active(applied))
        assertEquals(7L, active(controller.state))

        // La lettura vecchia arriva dopo con l'account di prima: si scarta.
        cli.named("state")[1].result.complete(state(2))
        val result = stale.get(5, TimeUnit.SECONDS)
        assertEquals(7L, active(result))
        assertEquals(7L, active(controller.state))
        assertFalse(events.states.any { active(it) == 2L }, "la lettura vecchia non si pubblica")
        assertEquals(listOf(1L, 7L), events.states.map { active(it) })
    }

    @Test
    fun `esito senza lista - si rilegge`() {
        cli.auto = { if (it.first() == "ide") status("ok") else if (it.first() == "state") state(4) else null }
        val result = controller.afterAction(json("""{"switch":null}""").jsonObject).get(5, TimeUnit.SECONDS)
        assertEquals(4L, active(result))
        assertEquals(1, cli.named("state").size)
    }

    @Test
    fun `lettura fallita - stato con errore e motore dell'ultimo stato`() {
        cli.auto = { if (it.first() == "ide") status("error") else null }
        val first = controller.refresh()
        cli.named("state")[0].result.complete(state(1, engineRunning = true))
        first.get(5, TimeUnit.SECONDS)
        val second = controller.refresh()
        cli.named("state")[1].result.completeExceptionally(CliException("notInstalled", "cswap non trovato"))
        val failed = second.get(5, TimeUnit.SECONDS)
        assertEquals(null, failed.list)
        assertEquals("notInstalled", failed.error?.kind)
        assertTrue(failed.engineRunning, "motore dall'ultimo stato noto")
        assertTrue(events.warnings.any { it.contains("cswap non trovato") })
    }

    @Test
    fun `riepilogo con soglie, PID e stato su stdin`() {
        engine.pid = 42
        cli.auto = { if (it.first() == "state") state(5) else null }
        val done = controller.refresh()
        val call = cli.named("ide").single()
        assertEquals(listOf("ide", "status", "--thresholds", "80,95", "--engine-pid", "42", "-"), call.args)
        assertEquals(controller.state!!.json, json(call.options.stdin!!))
        call.result.complete(status("warn"))
        done.get(5, TimeUnit.SECONDS)
        assertEquals("warn", controller.status?.level)

        // Stessa richiesta: nessun nuovo comando.
        controller.updateStatus()
        assertEquals(1, cli.named("ide").size)
    }

    @Test
    fun `riepilogo superato si scarta, errore della CLI diventa livello error`() {
        cli.auto = { if (it.first() == "state") state(5) else null }
        controller.refresh()
        val old = cli.named("ide").single()
        engine.pid = 9
        controller.updateStatus()
        val fresh = cli.named("ide")[1]
        fresh.result.completeExceptionally(CliException("failed", "boh"))
        old.result.complete(status("ok"))
        assertEquals(listOf("error"), events.statuses.map { it.level })
        assertEquals("error", controller.status?.level)
    }

    @Test
    fun `nuovo tentativo del motore - lock dallo stato`() {
        cli.auto = {
            when (it.first()) {
                "ide" -> status("ok")
                "notifications" -> json("""{"sender":"engine","notifications":[],"language":"it"}""")
                else -> null
            }
        }
        val a = controller.refresh()
        cli.named("state")[0].result.complete(state(1, engineRunning = false))
        a.get(5, TimeUnit.SECONDS)
        val b = controller.refresh()
        cli.named("state")[1].result.complete(state(1, engineRunning = true, problem = "lock illeggibile"))
        b.get(5, TimeUnit.SECONDS)
        assertEquals(listOf<Boolean?>(false, null), engine.lockStates)
    }

    private fun pollScenario(localRunning: Boolean, engineRunning: Boolean, problem: String? = null, sender: String = "caller"): Int {
        engine.running = localRunning
        cli.auto = {
            when (it.first()) {
                "ide" -> status("ok")
                "state" -> state(1, engineRunning, problem)
                "notifications" -> json("""{"sender":"$sender","notifications":[$note],"language":"it"}""")
                else -> null
            }
        }
        controller.refresh().get(5, TimeUnit.SECONDS)
        return cli.named("notifications", "poll", "-").size
    }

    @Test
    fun `notifiche - motore di questo processo, nessun poll`() {
        assertEquals(0, pollScenario(localRunning = true, engineRunning = true))
        assertTrue(events.notes.isEmpty())
    }

    @Test
    fun `notifiche - motore vivo altrove, nessun poll`() {
        assertEquals(0, pollScenario(localRunning = false, engineRunning = true))
        assertTrue(events.notes.isEmpty())
    }

    @Test
    fun `notifiche - nessun motore vivo, poll con la lista su stdin`() {
        assertEquals(1, pollScenario(localRunning = false, engineRunning = false))
        val call = cli.named("notifications").single()
        assertEquals(controller.state!!.list, json(call.options.stdin!!))
        assertEquals(listOf("k"), events.notes.map { it.key })
    }

    @Test
    fun `notifiche - lock non leggibile, decide la CLI`() {
        assertEquals(1, pollScenario(localRunning = false, engineRunning = true, problem = "x"))
        assertEquals(1, events.notes.size)
    }

    @Test
    fun `notifiche - mittente diverso da caller, niente da mostrare`() {
        assertEquals(1, pollScenario(localRunning = false, engineRunning = false, sender = "engine"))
        assertTrue(events.notes.isEmpty())
    }

    @Test
    fun `eta dell'ultima lettura`() {
        assertEquals(Long.MAX_VALUE, controller.ageMs())
        cli.auto = { if (it.first() == "ide") status("ok") else if (it.first() == "state") state(1) else null }
        controller.refresh().get(5, TimeUnit.SECONDS)
        now += 12_000
        assertEquals(12_000L, controller.ageMs())
    }

    @Test
    fun `stato con lista da un'azione e campi letti`() {
        val s = CliState(state(2, engineRunning = true).jsonObject)
        assertEquals(true, s.lockHeldElsewhere)
        val next = s.withList(json("""{"activeAccountNumber":3}""").jsonObject, 5)
        assertEquals(3L, active(next))
        assertEquals(5L, next.updatedAtMs)
        assertNotNull(next.engine["running"])
        assertEquals(listOf("ide", "status", "--thresholds", "", "-"), IdeStatus.args(emptyList(), null))
        assertEquals("error", IdeStatus.error(CswapError.of("failed", "x"), 1).level)
        assertEquals(JsonObject::class, IdeStatus.error(CswapError.of("failed", "x"), 1).json["content"]!!::class)
    }
}
