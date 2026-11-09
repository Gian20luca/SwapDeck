package com.swapdeck.jetbrains.bridge

import com.swapdeck.jetbrains.cli.CliTimeouts
import com.swapdeck.jetbrains.cli.CswapError
import com.swapdeck.jetbrains.protocol.HostProtocol
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Nested
import org.junit.jupiter.api.Test
import java.util.Collections
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ExecutionException
import java.util.concurrent.TimeUnit

/** [BridgeHandler] con CLI, host e dialoghi finti (porting di `bridge.test.ts` di VS Code). */
class BridgeHandlerTest {
    private val cli = FakeCli()
    private val host = FakeHost()
    private val ui = FakeUi()
    private val handler = BridgeHandler(cli, host, clock = { 1_000_000_000L })
    private val session = handler.openSession(ui)
    private val events: MutableList<JsonObject> = Collections.synchronizedList(mutableListOf())

    private fun start(method: String, params: String = "{}", from: BridgeSession = session): CompletableFuture<JsonElement> =
        handler.handle(JsonPrimitive(method), json(params), from) { events += it }

    private fun call(method: String, params: String = "{}", from: BridgeSession = session): JsonElement =
        start(method, params, from).get(5, TimeUnit.SECONDS)

    private fun failure(method: String, params: String = "{}", from: BridgeSession = session): CswapError = try {
        call(method, params, from)
        error("$method $params: atteso un errore")
    } catch (e: ExecutionException) {
        BridgeSession.errorOf(e)
    }

    private fun assertInvalid(method: String, params: String = "{}") {
        assertEquals("invalidUsage", failure(method, params).kind, "$method $params")
    }

    private fun waitFor(condition: () -> Boolean) {
        val until = System.currentTimeMillis() + 5_000
        while (!condition()) {
            check(System.currentTimeMillis() < until) { "condizione non raggiunta" }
            Thread.sleep(5)
        }
    }

    @Test
    fun `il ponte gestisce esattamente i metodi di protocol json`() {
        assertEquals(HostProtocol.requestMethods, handler.handledMethods)
        assertEquals(HostProtocol.requestMethods, BridgeRules.DEFAULT.methods)
        assertEquals(33, handler.handledMethods.size)
    }

    @Test
    fun `le espressioni di protocol json sono ancorate`() {
        val limits = HostProtocol.json.getValue("limits") as JsonObject
        val pick = limits.getValue("pick") as JsonObject
        for (pattern in listOf(limits["configKeyPattern"], limits["versionPattern"], pick["extensionPattern"])) {
            val text = (pattern as JsonPrimitive).content
            assertTrue(text.startsWith("^") && text.endsWith("$"), text)
        }
        // A capo finale: con `$` di Java passerebbe, con `matches` no.
        assertFalse(BridgeRules.DEFAULT.version.matches("1.0\n"))
    }

    @Test
    fun `listAccounts usa l'ultimo stato, con refresh rilegge`() {
        assertEquals(fixtureList(), call("listAccounts"))
        assertEquals(0, host.refreshes.get())
        call("listAccounts", """{"refresh":true}""")
        assertEquals(1, host.refreshes.get())
        assertTrue(cli.calls.isEmpty())
    }

    @Test
    fun `listAccounts senza lista rifiuta con l'errore dello stato`() {
        host.last = stateWith(null)
        host.refreshed = stateWith(null, """{"kind":"notInstalled","message":"cswap non trovato"}""")
        assertEquals("notInstalled", failure("listAccounts").kind)
    }

    @Test
    fun `metodi e parametri non validi non chiamano la CLI`() {
        assertInvalid("rm")
        assertEquals("invalidUsage", BridgeSession.errorOf(assertThrows { handler.handle(JsonPrimitive(1), null, session).get() }).kind)
        assertInvalid("getConfigValue", """{"key":"a b"}""")
        assertInvalid("getConfigValue", """{"key":"a\n"}""")
        assertInvalid("getConfigValue", """{"key":1}""")
        assertInvalid("getHistory", """{"account":-1}""")
        assertInvalid("getHistory", """{"account":1.5}""")
        assertInvalid("getHistory", """{"account":"3"}""")
        assertInvalid("getHistory", """{"account":9007199254740992}""")
        assertInvalid("runAction", """{"action":"switch"}""")
        assertInvalid("runAction", """{"action":["switch"]}""")
        assertInvalid("runAction", """{"action":{"kind":"rmrf"}}""")
        assertInvalid("confirmationFor", """{"action":{"kind":1}}""")
        for (claudeArgs in listOf("""["--dangerously-skip-permissions"]""", """["--continue",1]""", "\"x\"", "null")) {
            assertInvalid("runAction", """{"action":{"kind":"run","options":{"claudeArgs":$claudeArgs}}}""")
        }
        assertInvalid("dismissCswapUpdate", """{"version":"--force"}""")
        assertInvalid("submitReloginCode", """{"code":"a"}""")
        assertInvalid("startRelogin", """{"account":0}""")
        assertInvalid("startRelogin", "{}")
        assertInvalid("reloginAccounts", """{"list":1}""")
        assertInvalid("copyText", """{"text":1}""")
        assertTrue(cli.calls.isEmpty())
        assertTrue(ui.confirmations.isEmpty() && ui.copied.isEmpty())
    }

    private fun assertThrows(block: () -> Unit): Throwable = try {
        block()
        error("atteso un errore")
    } catch (e: ExecutionException) {
        e
    }

    @Test
    fun `parametri non oggetto valgono come vuoti`() {
        cli.responses["status"] = JsonPrimitive("ok")
        assertEquals(JsonPrimitive("ok"), handler.handle(JsonPrimitive("getStatus"), JsonArray(emptyList()), session).get())
        assertEquals(listOf(listOf("status")), cli.args())
    }

    @Test
    fun `run - claudeArgs assenti o dell'elenco chiuso`() {
        cli.responses["confirm"] = obj("""{"kind":"none"}""")
        cli.responses["action"] = obj("{}")
        call("runAction", """{"action":{"kind":"run","options":{}}}""")
        call("runAction", """{"action":{"kind":"run","options":{"claudeArgs":["--continue","--verbose"]}}}""")
        call("runAction", """{"action":{"kind":"run"}}""")
        assertEquals(3, cli.args().count { it.first() == "action" })
        assertTrue(ui.confirmations.isEmpty())
    }

    @Test
    fun `la conferma dovuta la chiede l'host - annullata, l'azione non parte`() {
        cli.responses["confirm"] = obj("""{"kind":"typed","word":"purge"}""")
        cli.responses["action"] = obj("""{"message":"ok"}""")
        ui.confirmAnswer = false
        assertEquals("cancelled", failure("runAction", """{"action":{"kind":"purge"}}""").kind)
        assertEquals(listOf("purge" to Confirmation.Typed("purge")), ui.confirmations.toList())
        assertEquals(listOf("confirm"), cli.args().map { it.first() })
        assertTrue(host.outcomes.isEmpty())

        ui.confirmAnswer = true
        assertEquals(obj("""{"message":"ok"}"""), call("runAction", """{"action":{"kind":"purge"}}"""))
        assertEquals(listOf("confirm", "confirm", "action"), cli.args().map { it.first() })
        assertEquals(1, host.outcomes.size)
    }

    @Test
    fun `conferma semplice e risposta di confirm non valida`() {
        cli.responses["confirm"] = obj("""{"kind":"simple"}""")
        call("runAction", """{"action":{"kind":"remove","account":3}}""")
        assertEquals("remove #3" to Confirmation.Simple, ui.confirmations.single())
        // Conferma con parola senza parola: l'azione non parte.
        cli.responses["confirm"] = obj("""{"kind":"typed"}""")
        assertEquals("failed", failure("runAction", """{"action":{"kind":"purge"}}""").kind)
        assertEquals(1, cli.args().count { it.first() == "action" })
    }

    @Test
    fun `azioni e conferme passano da stdin, con l'account attivo noto`() {
        cli.responses["confirm"] = obj("""{"kind":"none"}""")
        cli.responses["action"] = obj("""{"message":"ok"}""")
        val action = """{"kind":"addToken","token":"segreto"}"""
        call("confirmationFor", """{"action":$action}""")
        assertEquals(obj("""{"message":"ok"}"""), call("runAction", """{"action":$action}"""))
        assertEquals(listOf("confirm", "-", "--active", "6"), cli.calls[0].args)
        assertEquals(action, cli.calls[0].options.stdin)
        assertEquals(cli.calls[0].args, cli.calls[1].args)
        assertEquals(listOf("action", "-"), cli.calls[2].args)
        assertEquals(action, cli.calls[2].options.stdin)
        assertEquals(CliTimeouts.ACTION_MS, cli.calls[2].options.timeoutMs)
        assertFalse(cli.args().flatten().joinToString(" ").contains("segreto"))
        assertEquals(listOf(obj("""{"message":"ok"}""")), host.outcomes.toList())
    }

    @Test
    fun `confirm senza stato non passa l'attivo, senza attivo passa none`() {
        cli.responses["confirm"] = obj("""{"kind":"none"}""")
        host.last = null
        call("confirmationFor", """{"action":{"kind":"switch","target":2}}""")
        host.last = stateWith(fixtureList(active = "null"))
        call("confirmationFor", """{"action":{"kind":"switch","target":2}}""")
        assertEquals(listOf(listOf("confirm", "-"), listOf("confirm", "-", "--active", "none")), cli.args())
    }

    @Test
    fun `esito - terminale per run, avvisi e cambio non riuscito`() {
        cli.responses["confirm"] = obj("""{"kind":"none"}""")
        cli.responses["action"] = obj(
            """{"command":{"program":"cswap","args":["run","2","--continue"]},"warning":"attenzione",
               "switch":{"switched":false,"message":"nessun account"}}""",
        )
        call("runAction", """{"action":{"kind":"run","options":{"claudeArgs":["--continue"]}}}""")
        assertEquals(listOf(listOf("cswap", "run", "2", "--continue")), ui.commands.toList())
        assertEquals(listOf("SwapDeck: attenzione", "SwapDeck: nessun account"), ui.warnings.toList())

        cli.responses["action"] = obj("""{"switch":{"switched":true,"message":"ok"}}""")
        call("runAction", """{"action":{"kind":"switch","strategy":"best"}}""")
        assertEquals(2, ui.warnings.size)
        assertEquals(1, ui.commands.size)
    }

    @Test
    fun `getHistory - 7 giorni di default, filtro e punti dalla CLI`() {
        cli.responses["history"] = json("""[{"ts":1}]""")
        assertEquals(json("""[{"ts":1}]"""), call("getHistory", """{"account":3}"""))
        val from = (1_000_000L - 7 * 86400).toString()
        assertEquals(listOf("history", "--from", from, "--to", "1000000", "--account", "3", "--points"), cli.calls[0].args)
        call("getHistory", """{"sinceSeconds":500,"account":null}""")
        assertEquals(listOf("history", "--from", "500", "--to", "1000000", "--points"), cli.calls[1].args)
        assertEquals(CliTimeouts.DEFAULT_MS, cli.calls[1].options.timeoutMs)
    }

    @Test
    fun `letture di cswap`() {
        call("getStatus")
        call("getConfig")
        call("getConfigValue", """{"key":"auto.threshold"}""")
        assertEquals(listOf(listOf("status"), listOf("cswap-config"), listOf("cswap-config", "auto.threshold")), cli.args())
    }

    @Test
    fun `gestione - comandi, scritture da stdin e parametri validati`() {
        call("listMappings")
        call("getEngineState")
        call("setEngineRules", """{"rules":{"enabled":false,"primary":6}}""")
        call("getPreferences")
        call("setPreferences", """{"prefs":{"theme":"dark"}}""")
        call("getHistoryEvents")
        call("getHistoryEvents", """{"sinceSeconds":100,"account":3}""")
        assertEquals(
            listOf(
                listOf("mappings"),
                listOf("engine", "state"),
                listOf("engine", "rules", "set", "-"),
                listOf("preferences", "get"),
                listOf("preferences", "set", "-"),
                listOf("history", "events"),
                listOf("history", "events", "--since", "100", "--account", "3"),
            ),
            cli.args(),
        )
        assertEquals("""{"enabled":false,"primary":6}""", cli.calls[2].options.stdin)
        assertEquals("""{"theme":"dark"}""", cli.calls[4].options.stdin)
        assertEquals(null, cli.calls[0].options.stdin)
        for (rules in listOf("", ""","rules":null""", ""","rules":"x"""", ""","rules":[1]""")) {
            assertInvalid("setEngineRules", """{"x":0$rules}""")
        }
        assertInvalid("setPreferences", """{"prefs":1}""")
        assertInvalid("getHistoryEvents", """{"sinceSeconds":1.5}""")
        assertEquals(7, cli.calls.size)
    }

    @Test
    fun `chi consuma - argomenti, timeout lungo e richieste non valide`() {
        call("getUsageBreakdown", """{"query":{"window":"7d","groupBy":"agent","account":3}}""")
        call("getUsageBreakdown", """{"query":{"window":"custom","groupBy":"model","fromSeconds":10,"toSeconds":20}}""")
        assertEquals(
            listOf(
                listOf("usage", "breakdown", "--window", "7d", "--group-by", "agent", "--account", "3"),
                listOf("usage", "breakdown", "--window", "custom", "--group-by", "model", "--from", "10", "--to", "20"),
            ),
            cli.args(),
        )
        assertEquals(CliTimeouts.USAGE_MS, cli.calls.last().options.timeoutMs)
        for (query in listOf(
            """{"window":"1h","groupBy":"project"}""",
            """{"window":"5h","groupBy":"--evil"}""",
            """{"window":"custom","groupBy":"project"}""",
            """{"window":"5h","groupBy":"project","account":-1}""",
            "\"x\"",
        )) {
            assertInvalid("getUsageBreakdown", """{"query":$query}""")
        }
        assertEquals(2, cli.calls.size)
    }

    @Test
    fun `mappe e notifiche - comando e patch da stdin, validati`() {
        call("getEngineProfiles")
        call("applyEngineProfile", """{"command":{"op":"select","id":"eco"}}""")
        call("getNotificationSettings")
        call("setNotificationSettings", """{"patch":{"switched":{"enabled":false}}}""")
        assertEquals(
            listOf(
                listOf("engine", "profiles"),
                listOf("engine", "profiles", "-"),
                listOf("notifications", "get"),
                listOf("notifications", "set", "-"),
            ),
            cli.args(),
        )
        assertEquals("""{"op":"select","id":"eco"}""", cli.calls[1].options.stdin)
        assertEquals("""{"switched":{"enabled":false}}""", cli.calls[3].options.stdin)
        for (bad in listOf("{}", """{"command":"x"}""", """{"command":[1]}""")) assertInvalid("applyEngineProfile", bad)
        assertInvalid("setNotificationSettings", """{"patch":"x"}""")
        assertEquals(4, cli.calls.size)
    }

    @Test
    fun `setPreferences avvisa le viste solo se riesce`() {
        call("setPreferences", """{"prefs":{"language":"it"}}""")
        assertEquals(1, host.preferences)
        cli.respond = { _, _ -> CompletableFuture.failedFuture(com.swapdeck.jetbrains.cli.CliException("failed", "no")) }
        assertEquals("failed", failure("setPreferences", """{"prefs":{"language":"en"}}""").kind)
        assertEquals(1, host.preferences)
        assertInvalid("setPreferences", """{"prefs":"x"}""")
        assertEquals(1, host.preferences)
    }

    @Nested
    inner class Selettori {
        @Test
        fun `inoltra il tipo di selettore e solo le opzioni valide`() {
            val filters = listOf(PickFilter("JSON", listOf("json")))
            assertEquals(JsonPrimitive("/scelto"), call("pickOpenFile", """{"title":"T","filters":[{"name":"JSON","extensions":["json"]}],"extra":1}"""))
            assertEquals(PickKind.OPEN_FILE to PickOptions(title = "T", filters = filters), ui.picks.last())
            call("pickSaveFile", """{"defaultPath":"a.json"}""")
            assertEquals(PickKind.SAVE_FILE to PickOptions(defaultPath = "a.json"), ui.picks.last())
            call("pickFolder")
            assertEquals(PickKind.FOLDER to PickOptions(), ui.picks.last())
            assertTrue(cli.calls.isEmpty())
        }

        @Test
        fun `annullato - null, opzioni non valide - nessun selettore`() {
            ui.pickResult = null
            assertEquals(JsonNull, call("pickFolder"))
            ui.picks.clear()
            val eleven = (1..11).joinToString(",") { """{"name":"x","extensions":[]}""" }
            for (bad in listOf(
                """{"title":1}""",
                """{"title":"${"x".repeat(1001)}"}""",
                """{"filters":"x"}""",
                """{"filters":[{"name":"x","extensions":["a/b"]}]}""",
                """{"filters":[{"name":"x","extensions":[".json"]}]}""",
                """{"filters":[null]}""",
                """{"filters":[{"name":"${"n".repeat(101)}","extensions":[]}]}""",
                """{"filters":[$eleven]}""",
            )) {
                assertInvalid("pickOpenFile", bad)
            }
            assertTrue(ui.picks.isEmpty())
        }
    }

    @Nested
    inner class Setup {
        @Test
        fun `letture e chiusura dell'avviso con la versione validata`() {
            call("diagnoseSetup")
            call("checkCswapUpdate")
            call("checkCswapUpdate", """{"force":true}""")
            call("checkCswapUpdate", """{"force":"true"}""")
            assertEquals(JsonNull, call("dismissCswapUpdate", """{"version":"0.28.0rc1"}"""))
            assertEquals(
                listOf(
                    listOf("setup", "diagnose"),
                    listOf("setup", "check-update"),
                    listOf("setup", "check-update", "--force"),
                    listOf("setup", "check-update"),
                    listOf("setup", "dismiss-update", "0.28.0rc1"),
                ),
                cli.args(),
            )
            for (version in listOf("\"--force\"", "\"../x\"", "1", "\"${"a".repeat(80)}\"")) {
                assertInvalid("dismissCswapUpdate", """{"version":$version}""")
            }
            assertEquals(5, cli.calls.size)
        }

        @Test
        fun `aggiornamento - conferma, motore fermato e ripreso, eventi inoltrati`() {
            cli.respond = { _, options ->
                options.onEvent?.invoke(obj("""{"event":"stepStarted","step":"upgradeCswap"}"""))
                CompletableFuture.completedFuture(obj("""{"ok":true}"""))
            }
            assertEquals(obj("""{"ok":true}"""), call("upgradeCswap"))
            assertEquals(listOf("upgrade"), ui.setups.toList())
            assertEquals(listOf("pause:upgrade", "resume", "changed"), host.order.toList())
            assertEquals(listOf("setup", "upgrade"), cli.calls.single().args)
            assertEquals(CliTimeouts.SETUP_MS, cli.calls.single().options.timeoutMs)
            assertEquals(listOf(obj("""{"event":"stepStarted","step":"upgradeCswap"}""")), events.toList())
            assertEquals(1, host.refreshes.get())
        }

        @Test
        fun `installazione - senza pausa del motore, annullata non lancia nulla`() {
            call("installCswap")
            assertEquals(listOf("setup", "install"), cli.calls.single().args)
            assertEquals(listOf("changed"), host.order.toList())
            ui.setupAnswer = false
            assertEquals("cancelled", failure("installCswap").kind)
            assertEquals("cancelled", failure("upgradeCswap").kind)
            assertEquals(1, cli.calls.size)
            assertEquals(listOf("changed"), host.order.toList())
        }

        @Test
        fun `errore della CLI - motore ripreso lo stesso`() {
            cli.respond = { _, _ -> CompletableFuture.failedFuture(com.swapdeck.jetbrains.cli.CliException("failed", "uv")) }
            assertEquals("failed", failure("upgradeCswap").kind)
            assertEquals(listOf("pause:upgrade", "resume", "changed"), host.order.toList())
        }

        @Test
        fun `pannello chiuso durante l'aggiornamento - il comando arriva in fondo, poi il motore riparte`() {
            val pending = CompletableFuture<JsonElement>()
            cli.respond = { _, _ -> pending }
            val running = start("upgradeCswap")
            waitFor { cli.calls.size == 1 }
            session.dispose()
            assertFalse(pending.isCancelled, "uv tool upgrade non si interrompe a metà")
            Thread.sleep(100)
            assertFalse(running.isDone)
            assertEquals(listOf("pause:upgrade"), host.order.toList(), "motore fermo finché il comando non finisce")
            pending.complete(obj("""{"ok":true}"""))
            assertEquals(obj("""{"ok":true}"""), running.get(5, TimeUnit.SECONDS))
            assertEquals(listOf("pause:upgrade", "resume", "changed"), host.order.toList())
        }

        @Test
        fun `attesa dell'arresto del motore fallita - motore ripreso, nessun aggiornamento`() {
            host.pause = { why -> host.pauseWith(why, CompletableFuture.failedFuture(IllegalStateException("arresto"))) }
            assertEquals("failed", failure("upgradeCswap").kind)
            assertTrue(cli.calls.isEmpty())
            assertEquals(listOf("pause:upgrade", "resume", "changed"), host.order.toList())
        }

        @Test
        fun `pannello già chiuso - nessuna installazione`() {
            session.dispose()
            assertEquals("cancelled", failure("installCswap").kind)
            assertTrue(cli.calls.isEmpty())
        }
    }

    @Test
    fun `diagnostica, cartella dei log e appunti`() {
        cli.responses["diagnostics"] = obj("""{"text":"Diagnostica di SwapDeck"}""")
        cli.responses["logs"] = obj("""{"dir":"/tmp/logs"}""")
        assertEquals(JsonPrimitive("Diagnostica di SwapDeck"), call("getDiagnostics"))
        assertEquals(listOf("diagnostics", "--json"), cli.calls.last().args)
        host.clientInfo = "IntelliJ IDEA 2024.3.7, plugin 0.2.4"
        call("getDiagnostics")
        assertEquals(listOf("diagnostics", "--json", "--client", "IntelliJ IDEA 2024.3.7, plugin 0.2.4"), cli.calls.last().args)
        assertEquals(JsonNull, call("openLogsFolder"))
        assertEquals(listOf("logs", "dir"), cli.calls.last().args)
        assertEquals(listOf("/tmp/logs"), ui.folders.toList())
        assertEquals(JsonNull, call("copyText", """{"text":"abc"}"""))
        assertEquals(listOf("abc"), ui.copied.toList())
        assertInvalid("copyText", """{"text":"${"x".repeat(BridgeRules.DEFAULT.copyTextMaxLength + 1)}"}""")
        assertEquals(1, ui.copied.size)
    }

    @Nested
    inner class RifaiLogin {
        private val channel = RecordingChannel()
        private val finish = CompletableFuture<JsonElement>()

        init {
            cli.respond = { args, options ->
                if (args.getOrNull(1) == "candidates") {
                    CompletableFuture.completedFuture(obj("""{"accounts":[2,"x",5,1.5]}"""))
                } else {
                    options.channel?.invoke(channel)
                    options.onEvent?.invoke(obj("""{"event":"link","url":"https://claude.ai/x","timeoutSeconds":300}"""))
                    finish
                }
            }
        }

        @Test
        fun `candidati dalla CLI con la lista su stdin`() {
            val list = fixtureList()
            assertEquals(json("[2,5]"), call("reloginAccounts", """{"list":$list}"""))
            assertEquals(listOf("relogin", "candidates", "-"), cli.calls[0].args)
            assertEquals(list.toString(), cli.calls[0].options.stdin)
        }

        @Test
        fun `motore in pausa, codice e annulla solo su stdin, uno alla volta`() {
            val running = start("startRelogin", """{"account":2}""")
            waitFor { cli.calls.size == 1 }
            assertEquals(listOf("relogin", "start", "2"), cli.calls[0].args)
            assertEquals(CliTimeouts.RELOGIN_MS, cli.calls[0].options.timeoutMs)
            assertEquals("busy", failure("startRelogin", """{"account":3}""").kind)
            // Da un altro pannello niente codice: il «Rifai login» è del primo.
            val other = handler.openSession(FakeUi())
            assertInvalid2(other, "submitReloginCode", """{"code":"x"}""")
            call("submitReloginCode", """{"code":"codice#1"}""")
            call("cancelRelogin")
            assertEquals(listOf("{\"code\":\"codice#1\"}\n", "{\"cancel\":true}\n"), channel.written.toList())
            assertInvalid("submitReloginCode", """{"code":"a\nb"}""")
            assertInvalid("submitReloginCode", """{"code":"a\rb"}""")
            assertInvalid("submitReloginCode", """{"code":"${"c".repeat(4097)}"}""")
            assertFalse(cli.args().flatten().any { it.contains("codice") })
            finish.complete(obj("""{"result":"cancelled","account":2,"restored":true}"""))
            assertEquals("cancelled", (running.get(5, TimeUnit.SECONDS) as JsonObject).textOrNull("result"))
            assertEquals(listOf(obj("""{"event":"link","url":"https://claude.ai/x","timeoutSeconds":300}""")), events.toList())
            assertEquals(listOf("pause:relogin", "resume", "changed"), host.order.toList())
            assertInvalid("cancelRelogin")
            // Finito: se ne può avviare un altro.
            cli.respond = { _, _ -> CompletableFuture.completedFuture(obj("""{"result":"ok"}""")) }
            call("startRelogin", """{"account":3}""")
        }

        @Test
        fun `attesa dell'arresto del motore annullata - motore ripreso, nessun login`() {
            host.pause = { why -> host.pauseWith(why, CompletableFuture<Unit>().apply { cancel(true) }) }
            assertEquals("cancelled", failure("startRelogin", """{"account":2}""").kind)
            assertTrue(cli.calls.isEmpty())
            assertEquals(listOf("pause:relogin", "resume", "changed"), host.order.toList())
            // Il «Rifai login» non resta occupato.
            host.pause = { why -> host.pauseWith(why, CompletableFuture.completedFuture(Unit)) }
            finish.complete(obj("""{"result":"ok"}"""))
            call("startRelogin", """{"account":2}""")
        }

        private fun assertInvalid2(from: BridgeSession, method: String, params: String) {
            assertEquals("invalidUsage", failure(method, params, from).kind)
        }

        @Test
        fun `pannello chiuso durante l'operazione - annulla, motore ripreso`() {
            val running = start("startRelogin", """{"account":2}""")
            waitFor { cli.calls.size == 1 }
            session.dispose()
            assertEquals(listOf("{\"cancel\":true}\n"), channel.written.toList())
            finish.complete(obj("""{"result":"cancelled","account":2,"previous":6,"restored":true}"""))
            running.get(5, TimeUnit.SECONDS)
            assertEquals(listOf("pause:relogin", "resume", "changed"), host.order.toList())
        }

        @Test
        fun `pannello chiuso prima - nessuna CLI, durante la pausa - annulla appena c'è stdin`() {
            val closed = handler.openSession(FakeUi())
            closed.dispose()
            assertEquals("cancelled", failure("startRelogin", """{"account":2}""", closed).kind)
            assertTrue(cli.calls.isEmpty())

            val stopped = CompletableFuture<Unit>()
            host.pause = { why -> host.pauseWith(why, stopped) }
            val running = start("startRelogin", """{"account":2}""")
            waitFor { host.order.isNotEmpty() }
            session.dispose()
            stopped.complete(Unit)
            waitFor { cli.calls.size == 1 }
            assertEquals(listOf("{\"cancel\":true}\n"), channel.written.toList())
            finish.complete(obj("""{"result":"cancelled","account":2,"restored":true}"""))
            running.get(5, TimeUnit.SECONDS)
            assertEquals(listOf("pause:relogin", "resume", "changed"), host.order.toList())
        }
    }
}
