package com.swapdeck.jetbrains.bridge

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.util.Collections
import java.util.concurrent.CompletableFuture
import java.util.concurrent.TimeUnit

/** Messaggi `request` → `progress`/`response` di [BridgeSession], [AbortScope] e testi per l'utente. */
class BridgeSessionTest {
    private val cli = FakeCli()
    private val handler = BridgeHandler(cli, FakeHost())
    private val session = handler.openSession(FakeUi())
    private val sent: MutableList<JsonObject> = Collections.synchronizedList(mutableListOf())

    private fun waitSent(count: Int) {
        val until = System.currentTimeMillis() + 5_000
        while (sent.size < count) {
            check(System.currentTimeMillis() < until) { "messaggi attesi: $count, arrivati: ${sent.size}" }
            Thread.sleep(5)
        }
    }

    @Test
    fun `richiesta riuscita - risposta con id e result`() {
        cli.responses["status"] = obj("""{"ok":1}""")
        assertTrue(session.receive("""{"type":"request","id":7,"method":"getStatus","params":{}}""") { sent += it })
        waitSent(1)
        assertEquals(obj("""{"type":"response","id":7,"ok":true,"result":{"ok":1}}"""), sent.single())
    }

    @Test
    fun `richiesta non valida - risposta d'errore invalidUsage senza CLI`() {
        session.receive(obj("""{"type":"request","id":8,"method":"rm","params":{}}""")) { sent += it }
        waitSent(1)
        assertEquals(
            obj("""{"type":"response","id":8,"ok":false,"error":{"kind":"invalidUsage","message":"metodo sconosciuto"}}"""),
            sent.single(),
        )
        assertTrue(cli.calls.isEmpty())
    }

    @Test
    fun `avanzamento con l'id della richiesta, poi la risposta`() {
        cli.respond = { _, options ->
            options.onEvent?.invoke(obj("""{"event":"stepStarted","step":"installUv"}"""))
            CompletableFuture.completedFuture(obj("{}"))
        }
        session.request(3, json("\"installCswap\""), json("{}")) { sent += it }.get(5, TimeUnit.SECONDS)
        assertEquals(
            listOf(
                obj("""{"type":"progress","id":3,"event":{"event":"stepStarted","step":"installUv"}}"""),
                obj("""{"type":"response","id":3,"ok":true,"result":{}}"""),
            ),
            sent.toList(),
        )
    }

    @Test
    fun `messaggi che non sono richieste valide - ignorati`() {
        val send: (JsonObject) -> Unit = { sent += it }
        assertFalse(session.receive("""{"type":"ready"}""", send))
        assertFalse(session.receive("non json", send))
        assertFalse(session.receive("[1]", send))
        for (id in listOf("1.5", "\"1\"", "null", "9007199254740992")) {
            assertFalse(session.receive("""{"type":"request","id":$id,"method":"getStatus"}""", send), id)
        }
        assertTrue(session.receive("""{"type":"request","id":-2.0,"method":"getStatus"}""", send))
        waitSent(1)
        assertEquals(-2L, (sent.single()["id"] as kotlinx.serialization.json.JsonPrimitive).content.toLong())
        assertEquals(1, cli.calls.size)
    }

    @Test
    fun `pannello chiuso - nessun messaggio dopo`() {
        val pending = CompletableFuture<JsonElement>()
        cli.respond = { _, _ -> pending }
        val done = session.request(1, json("\"getStatus\""), null) { sent += it }
        session.dispose()
        pending.complete(obj("{}"))
        done.get(5, TimeUnit.SECONDS)
        assertTrue(sent.isEmpty())
        assertTrue(session.disposed)
    }

    @Test
    fun `pannello chiuso durante l'installazione - niente più avanzamento, il comando continua`() {
        val pending = CompletableFuture<JsonElement>()
        var onEvent: ((JsonObject) -> Unit)? = null
        cli.respond = { _, options ->
            onEvent = options.onEvent
            options.onEvent?.invoke(obj("""{"event":"stepStarted","step":"installUv"}"""))
            pending
        }
        val done = session.request(4, json("\"installCswap\""), json("{}")) { sent += it }
        waitSent(1)
        session.dispose()
        onEvent!!.invoke(obj("""{"event":"stepStarted","step":"installCswap"}"""))
        assertFalse(pending.isCancelled)
        pending.complete(obj("{}"))
        done.get(5, TimeUnit.SECONDS)
        assertEquals(listOf(obj("""{"type":"progress","id":4,"event":{"event":"stepStarted","step":"installUv"}}""")), sent.toList())
    }

    @Test
    fun `AbortScope - una volta sola, registrazioni tolte, dopo la chiusura subito`() {
        val scope = AbortScope()
        val calls = mutableListOf<String>()
        scope.onAbort { calls += "a" }
        scope.onAbort { calls += "b" }.close()
        scope.abort()
        scope.abort()
        assertEquals(listOf("a"), calls)
        scope.onAbort { calls += "c" }
        assertEquals(listOf("a", "c"), calls)
    }

    @Test
    fun `describeAction - testo breve, mai il token`() {
        assertEquals("remove #3", describeAction(obj("""{"kind":"remove","account":3}""")))
        assertEquals("swap #1 ↔ #2", describeAction(obj("""{"kind":"swap","a":1,"b":2}""")))
        assertEquals("move #1 → 4", describeAction(obj("""{"kind":"move","account":1,"slot":4}""")))
        assertEquals("map #1 /p", describeAction(obj("""{"kind":"map","account":1,"path":"/p"}""")))
        assertEquals("unmap /p", describeAction(obj("""{"kind":"unmap","path":"/p"}""")))
        assertEquals("export → f", describeAction(obj("""{"kind":"export","path":"f"}""")))
        assertEquals("export #2 → f", describeAction(obj("""{"kind":"export","path":"f","account":2}""")))
        assertEquals("unclaimed --purge x", describeAction(obj("""{"kind":"unclaimedPurge","id":"x"}""")))
        assertEquals("configUnset k", describeAction(obj("""{"kind":"configUnset","key":"k"}""")))
        assertEquals("addToken", describeAction(obj("""{"kind":"addToken","token":"segreto"}""")))
    }

    @Test
    fun `riga del comando da copiare`() {
        assertEquals("cswap run 2 --continue", commandLine("cswap", listOf("run", "2", "--continue")))
        assertEquals("\"C:\\Program Files\\cswap.exe\" \"\" \"a\\\"b\"", commandLine("C:\\Program Files\\cswap.exe", listOf("", "a\"b")))
    }
}
