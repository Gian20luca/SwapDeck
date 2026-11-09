package com.swapdeck.jetbrains.cli

import com.swapdeck.jetbrains.PluginThreads
import com.swapdeck.jetbrains.testing.FakeScripts
import com.swapdeck.jetbrains.testing.FakeScripts.waitUntil
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertInstanceOf
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeAll
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.TestInstance
import org.junit.jupiter.api.condition.DisabledOnOs
import org.junit.jupiter.api.condition.OS
import org.junit.jupiter.api.io.TempDir
import java.nio.file.Files
import java.nio.file.Path
import java.util.Collections
import java.util.concurrent.CancellationException
import java.util.concurrent.CompletableFuture
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/** [CliRunner] contro finti `swapdeck-cli` (script POSIX). */
@DisabledOnOs(OS.WINDOWS)
@TestInstance(TestInstance.Lifecycle.PER_CLASS)
class CliRunnerTest {
    private lateinit var dir: Path

    private lateinit var ok: Path
    private lateinit var echoArgs: Path
    private lateinit var errorEnvelope: Path
    private lateinit var notJson: Path
    private lateinit var cat: Path
    private lateinit var env: Path
    private lateinit var events: Path
    private lateinit var interactive: Path
    private lateinit var slow: Path
    private lateinit var stubborn: Path
    private lateinit var big: Path
    private lateinit var errorNoEnvelope: Path

    @BeforeAll
    fun scripts(@TempDir temp: Path) {
        dir = temp
        ok = FakeScripts.write(dir, "ok", """echo '{"ok":true,"n":1}'""")
        echoArgs = FakeScripts.write(
            dir,
            "args",
            """
            printf '{"count":%d,"first":"%s","second":"%s"}\n' "${'$'}#" "${'$'}1" "${'$'}2"
            """,
        )
        errorEnvelope = FakeScripts.write(
            dir,
            "error",
            """
            echo '{"error":{"kind":"busy","message":"cswap occupato","exitCode":2}}'
            exit 1
            """,
        )
        errorNoEnvelope = FakeScripts.write(dir, "error2", "echo '{\"x\":1}'\nexit 4")
        notJson = FakeScripts.write(
            dir,
            "notjson",
            """
            echo 'non è JSON'
            echo 'dettaglio su stderr' >&2
            exit 1
            """,
        )
        cat = FakeScripts.write(dir, "cat", "exec cat")
        env = FakeScripts.write(dir, "env", """printf '{"src":"%s","extra":"%s"}\n' "${'$'}SWAPDECK_LOG_SOURCE" "${'$'}EXTRA"""")
        events = FakeScripts.write(
            dir,
            "events",
            """
            echo '{"event":"progress","step":1}'
            echo 'riga non JSON'
            echo '{"noevent":true}'
            echo '{"event":"progress","step":2}'
            echo '{"done":true}'
            """,
        )
        interactive = FakeScripts.write(
            dir,
            "interactive",
            """
            read first
            printf '{"event":"got","line":"%s"}\n' "${'$'}first"
            while read line; do
              printf '{"event":"got","line":"%s"}\n' "${'$'}line"
              [ "${'$'}line" = quit ] && break
            done
            echo '{"finished":true}'
            """,
        )
        slow = FakeScripts.write(dir, "slow", "echo $$ > \"\$PID_FILE\"\nexec sleep 30")
        stubborn = FakeScripts.write(dir, "stubborn", "trap '' TERM\necho $$ > \"\$PID_FILE\"\nexec sleep 30")
        big = FakeScripts.write(dir, "big", "head -c 200000 /dev/zero | tr '\\0' 'a'\necho")
    }

    private fun runner(file: Path, grace: Long = 200, max: Int = CliLimits.MAX_STDOUT_BYTES) =
        CliRunner(executable = { file }, killGraceMs = grace, maxStdoutBytes = max)

    private fun failure(future: CompletableFuture<*>): Throwable =
        assertThrows(Exception::class.java) { future.get(10, TimeUnit.SECONDS) }.unwrap()

    @Test
    fun `successo con JSON su stdout`() {
        val value = runner(ok).run("state").get(10, TimeUnit.SECONDS)
        assertEquals(JsonPrimitive(true), value.jsonObject["ok"])
    }

    @Test
    fun `argomenti passati senza shell`() {
        val value = runner(echoArgs).run(listOf("a b; rm -rf x", "\$HOME"), CliOptions()).get(10, TimeUnit.SECONDS)
        assertEquals("2", value.jsonObject["count"]!!.jsonPrimitive.content)
        assertEquals("a b; rm -rf x", value.jsonObject["first"]!!.jsonPrimitive.content)
        assertEquals("\$HOME", value.jsonObject["second"]!!.jsonPrimitive.content)
    }

    @Test
    fun `busta d'errore con uscita 1`() {
        val error = failure(runner(errorEnvelope).run("action", "-"))
        assertInstanceOf(CliException::class.java, error)
        val info = (error as CliException).error
        assertEquals("busy", info.kind)
        assertEquals("cswap occupato", info.message)
        assertEquals(2, info.exitCode)
        assertEquals("busy", error.toCswapError().kind)
    }

    @Test
    fun `uscita non zero senza busta`() {
        val error = failure(runner(errorNoEnvelope).run("x")) as CliException
        assertEquals("failed", error.error.kind)
        assertTrue(error.message!!.contains("uscita 4"))
    }

    @Test
    fun `risposta non JSON con coda di stderr`() {
        val error = failure(runner(notJson).run("state")) as CliException
        assertEquals("failed", error.error.kind)
        assertTrue(error.message!!.contains("risposta non JSON (uscita 1)"), error.message)
        assertTrue(error.message!!.contains("dettaglio su stderr"), error.message)
    }

    @Test
    fun `stdin consegnato anche grande`() {
        val payload = """{"text":"${"x".repeat(1_000_000)}"}"""
        val value = runner(cat).run(listOf("ide", "status", "-"), CliOptions(stdin = payload)).get(10, TimeUnit.SECONDS)
        assertEquals(1_000_000, value.jsonObject["text"]!!.jsonPrimitive.content.length)
    }

    @Test
    fun `SWAPDECK_LOG_SOURCE e variabili aggiunte`() {
        val value = runner(env).run(listOf("x"), CliOptions(env = mapOf("EXTRA" to "valore", "SWAPDECK_LOG_SOURCE" to "altro")))
            .get(10, TimeUnit.SECONDS).jsonObject
        assertEquals("jetbrains", value["src"]!!.jsonPrimitive.content)
        assertEquals("valore", value["extra"]!!.jsonPrimitive.content)
    }

    @Test
    fun `streaming di eventi con esito dall'ultima riga`() {
        val seen = Collections.synchronizedList(mutableListOf<JsonObject>())
        val value = runner(events).run(listOf("setup", "install"), CliOptions(onEvent = { seen += it }))
            .get(10, TimeUnit.SECONDS)
        assertEquals(listOf("1", "2"), seen.map { it["step"]!!.jsonPrimitive.content })
        assertEquals(JsonPrimitive(true), value.jsonObject["done"])
    }

    @Test
    fun `canale interattivo su stdin`() {
        val seen = Collections.synchronizedList(mutableListOf<String>())
        val handed = AtomicReference<StdinChannel>()
        val future = runner(interactive).run(
            listOf("relogin", "start"),
            CliOptions(stdin = "primo\n", onEvent = { seen += it["line"]!!.jsonPrimitive.content }, channel = { handed.set(it) }),
        )
        assertTrue(waitUntil { seen.size == 1 && handed.get() != null })
        val channel = handed.get()
        assertTrue(channel.write("codice\n"))
        assertTrue(waitUntil { seen.size == 2 })
        assertTrue(channel.write("quit\n"))
        val value = future.get(10, TimeUnit.SECONDS)
        assertEquals(listOf("primo", "codice", "quit"), seen.toList())
        assertEquals(JsonPrimitive(true), value.jsonObject["finished"])
        assertFalse(channel.write("tardi\n"), "dopo l'uscita il canale è chiuso")
    }

    @Test
    fun `timeout termina il processo`() {
        val pidFile = dir.resolve("slow.pid")
        val started = System.nanoTime()
        val future = runner(slow).run(listOf("state"), CliOptions(timeoutMs = 300, env = mapOf("PID_FILE" to pidFile.toString())))
        val error = failure(future) as CliException
        assertEquals("timeout", error.error.kind)
        assertTrue(error.message!!.contains("swapdeck-cli state"))
        assertTrue(TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - started) < 5_000)
        assertTrue(waitUntil { !FakeScripts.alive(pidFile) })
    }

    @Test
    fun `timeout con SIGTERM ignorato usa il kill`() {
        val pidFile = dir.resolve("stubborn.pid")
        val future = runner(stubborn).run(listOf("state"), CliOptions(timeoutMs = 300, env = mapOf("PID_FILE" to pidFile.toString())))
        val error = failure(future) as CliException
        assertEquals("timeout", error.error.kind)
        assertTrue(waitUntil { !FakeScripts.alive(pidFile) })
    }

    @Test
    fun `annullare il future termina il processo`() {
        val pidFile = dir.resolve("cancel.pid")
        val future = runner(slow).run(listOf("state"), CliOptions(env = mapOf("PID_FILE" to pidFile.toString())))
        assertTrue(waitUntil { Files.exists(pidFile) && Files.size(pidFile) > 0 })
        future.cancel(true)
        assertThrows(CancellationException::class.java) { future.get() }
        assertTrue(waitUntil { !FakeScripts.alive(pidFile) })
    }

    @Test
    fun `stdin pieno - lancio e annullamento non si bloccano`() {
        // Il processo non legge stdin: la prima riga (1 MB) riempie la pipe e la scrittura resta ferma.
        val pidFile = dir.resolve("blocked.pid")
        val options = CliOptions(stdin = "x".repeat(1_000_000) + "\n", channel = {}, env = mapOf("PID_FILE" to pidFile.toString()))
        val launched = CompletableFuture.supplyAsync { runner(slow).run(listOf("relogin", "start"), options) }
        val future = launched.get(3, TimeUnit.SECONDS)
        assertTrue(waitUntil { Files.exists(pidFile) && Files.size(pidFile) > 0 })
        Thread.sleep(200)
        val cancelling = CompletableFuture.runAsync { future.cancel(true) }
        cancelling.get(3, TimeUnit.SECONDS)
        assertTrue(future.isCancelled)
        assertTrue(waitUntil { !FakeScripts.alive(pidFile) })
    }

    @Test
    fun `close annulla i comandi in corso e rifiuta i nuovi`() {
        val pidFile = dir.resolve("close.pid")
        val launches = AtomicInteger()
        val runner = CliRunner(executable = { launches.incrementAndGet(); slow }, killGraceMs = 200)
        val future = runner.run(listOf("state"), CliOptions(env = mapOf("PID_FILE" to pidFile.toString())))
        assertTrue(waitUntil { Files.exists(pidFile) && Files.size(pidFile) > 0 })
        runner.close()
        assertTrue(future.isCancelled)
        assertTrue(waitUntil { !FakeScripts.alive(pidFile) })
        val error = failure(runner.run("state")) as CliException
        assertEquals("cancelled", error.error.kind)
        assertEquals(1, launches.get(), "dopo close nessun processo")
    }

    @Test
    fun `pool chiusi - nessun lavoro dopo lo shutdown, comando fallito`() {
        val threads = PluginThreads("Prova")
        val ran = AtomicInteger()
        threads.scheduler.schedule({ ran.incrementAndGet() }, 200, TimeUnit.MILLISECONDS)
        threads.shutdown()
        assertTrue(threads.isShutdown)
        assertThrows(RejectedExecutionException::class.java) { threads.io.execute { ran.incrementAndGet() } }
        Thread.sleep(300)
        assertEquals(0, ran.get(), "timer annullati")
        val runner = CliRunner(executable = { ok }, executor = threads.io, scheduler = threads.scheduler)
        val error = failure(runner.run("state")) as CliException
        assertEquals("cancelled", error.error.kind)
    }

    @Test
    fun `binario mancante`() {
        val error = failure(runner(dir.resolve("non-esiste")).run("state"))
        assertInstanceOf(CliNotFoundException::class.java, error)
        assertTrue(error.message!!.contains("non trovato"))
        assertEquals("failed", error.toCswapError().kind)
    }

    @Test
    fun `limite di stdout`() {
        val error = failure(runner(big, max = 50_000).run("history")) as CliException
        assertEquals("failed", error.error.kind)
        assertTrue(error.message!!.contains("troppo grande"))
    }

    @Test
    fun `versione bloccante rilancia l'errore della CLI`() {
        val error = assertThrows(CliException::class.java) { runner(errorEnvelope).runBlocking(listOf("action")) }
        assertEquals("busy", error.error.kind)
    }

    @Test
    fun `parseCliResult e lastLine`() {
        assertEquals(JsonPrimitive(1), parseCliResult(0, " 1 \n"))
        assertEquals("c", lastLine("a\nb\nc\n\n"))
        assertEquals("", lastLine(""))
    }
}
