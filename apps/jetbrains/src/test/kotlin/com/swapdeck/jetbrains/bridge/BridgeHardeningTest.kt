package com.swapdeck.jetbrains.bridge

import com.swapdeck.jetbrains.IdeText
import com.swapdeck.jetbrains.cli.CliException
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.util.Collections
import java.util.concurrent.CancellationException
import java.util.concurrent.ExecutionException
import java.util.concurrent.TimeUnit

/** Correzioni della revisione di sicurezza del ponte: numeri e messaggi enormi, testi nei dialoghi, percorsi. */
class BridgeHardeningTest {
    private val cli = FakeCli()
    private val handler = BridgeHandler(cli, FakeHost(), clock = { 1_000_000_000L })
    private val session = handler.openSession(FakeUi())
    private val sent: MutableList<JsonObject> = Collections.synchronizedList(mutableListOf())

    /** Esegue [block] e controlla che resti entro [maxMs] (prima della correzione: secondi). */
    private fun <T> fast(maxMs: Long = 500, block: () -> T): T {
        val start = System.nanoTime()
        val result = block()
        val ms = (System.nanoTime() - start) / 1_000_000
        assertTrue(ms < maxMs, "durata $ms ms")
        return result
    }

    @Test
    fun `id con un milione di cifre - scartato subito`() {
        val digits = "1".repeat(1_000_000)
        assertFalse(fast { session.receive("""{"type":"request","id":$digits,"method":"getStatus"}""") { sent += it } })
        assertFalse(fast { session.receive("""{"type":"request","id":1e999999999,"method":"getStatus"}""") { sent += it } })
        assertTrue(cli.calls.isEmpty())
    }

    @Test
    fun `parametro intero con un milione di cifre - invalidUsage subito, niente CLI`() {
        val digits = "9".repeat(1_000_000)
        val error = fast(2_000) {
            try {
                handler.handle(JsonPrimitive("getHistory"), json("""{"account":$digits}"""), session).get(5, TimeUnit.SECONDS)
                error("atteso un errore")
            } catch (e: ExecutionException) {
                BridgeSession.errorOf(e)
            }
        }
        assertEquals("invalidUsage", error.kind)
        assertTrue(cli.calls.isEmpty())
    }

    @Test
    fun `numeri entro il limite di lunghezza - accettati come prima`() {
        assertEquals(9_007_199_254_740_991L, BridgeValidation.safeInteger(JsonPrimitive(9_007_199_254_740_991L)))
        assertEquals(3L, BridgeValidation.safeInteger(json("3." + "0".repeat(27)) as JsonPrimitive))
        assertNull(BridgeValidation.safeInteger(json("3." + "0".repeat(31)) as JsonPrimitive))
    }

    @Test
    fun `messaggio oltre il limite - scartato senza leggerlo, niente CLI`() {
        val padding = "x".repeat(BridgeSession.MAX_MESSAGE_CHARS)
        val text = """{"type":"request","id":1,"method":"copyText","params":{"text":"$padding"}}"""
        assertFalse(fast(2_000) { session.receive(text) { sent += it } })
        assertTrue(cli.calls.isEmpty())
        assertTrue(sent.isEmpty())
    }

    @Test
    fun `errore interno - messaggio generico senza dettagli`() {
        val error = BridgeSession.errorOf(IllegalStateException("/home/utente/segreto non trovato"))
        assertEquals("failed", error.kind)
        assertFalse(error.message.contains("/home"))
        assertEquals("x", BridgeSession.errorOf(CliException("notFound", "x")).message)
        assertEquals("cancelled", BridgeSession.errorOf(CancellationException()).kind)
    }

    @Test
    fun `describeAction - controlli e direzione resi visibili, valori lunghi troncati`() {
        assertEquals(
            "map #1 /a\\u000A/b\\u202Etxt.exe",
            describeAction(obj("""{"kind":"map","account":1,"path":"/a\n/b\u202etxt.exe"}""")),
        )
        assertEquals("setAlias #\\u200E\\u2066x\\u2069", describeAction(obj("""{"kind":"setAlias","account":"\u200e\u2066x\u2069"}""")))
        assertEquals("configSet a\\u2028b\\u0000", describeAction(obj("""{"kind":"configSet","key":"a\u2028b\u0000"}""")))
        val long = describeAction(obj("""{"kind":"unmap","path":"${"p".repeat(500)}"}"""))
        assertEquals("unmap " + "p".repeat(DESCRIBE_MAX_VALUE_LENGTH) + "…", long)
        // Coppie surrogate intere, anche al taglio.
        assertEquals("😀".repeat(3) + "…", displaySafe("😀".repeat(10), max = 6))
        assertEquals("caffè ✓", displaySafe("caffè ✓"))
    }

    @Test
    fun `testo nelle notifiche e nei dialoghi - sotto escape`() {
        assertEquals("SwapDeck: &lt;img src=&quot;http://x/&quot;&gt; &amp; c", IdeText.html("SwapDeck: <img src=\"http://x/\"> & c"))
        assertEquals("<html>a<br>&lt;b&gt;</html>", IdeText.dialog("a\n<b>"))
    }

    @Test
    fun `percorso iniziale dei selettori - solo assoluto e locale`() {
        for (unix in listOf("/home/u/file.json", "/")) assertEquals(unix, LocalPaths.localAbsolute(unix, windows = false), unix)
        for (win in listOf("C:\\Users\\u\\a.json", "d:/dati")) assertEquals(win, LocalPaths.localAbsolute(win, windows = true), win)
        val rejected = listOf(
            null, "", "  ", "relativo/file", "./a", "~/a",
            "\\\\server\\share\\a", "//server/share/a", "\\/server/a", "/\\server\\a", "\\\\?\\C:\\a", "\\\\.\\pipe\\x",
            "/a\u0000b", "/a\nb",
        )
        for (path in rejected) {
            assertNull(LocalPaths.localAbsolute(path, windows = false), "unix: $path")
            assertNull(LocalPaths.localAbsolute(path, windows = true), "windows: $path")
        }
        for (path in listOf("/home/u", "\\Users\\u", "C:relativo", "C:\\a:stream", "file:///C:/a")) {
            assertNull(LocalPaths.localAbsolute(path, windows = true), "windows: $path")
        }
        assertNull(LocalPaths.localAbsolute("C:\\Users", windows = false))
    }
}
