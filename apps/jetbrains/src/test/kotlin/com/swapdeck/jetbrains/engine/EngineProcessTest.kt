package com.swapdeck.jetbrains.engine

import com.swapdeck.jetbrains.testing.FakeScripts
import com.swapdeck.jetbrains.testing.FakeScripts.waitUntil
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.condition.DisabledOnOs
import org.junit.jupiter.api.condition.OS
import org.junit.jupiter.api.io.TempDir
import java.io.InputStream
import java.io.OutputStream
import java.nio.file.Path
import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** [EngineProcess] da solo: righe, avvio fallito e arresto. */
@DisabledOnOs(OS.WINDOWS)
class EngineProcessTest {
    private class Recorder(private val failOn: String? = null) : EngineProcess.Events {
        val lines: MutableList<String> = Collections.synchronizedList(mutableListOf())
        val exits: MutableList<Pair<Long, Int?>> = Collections.synchronizedList(mutableListOf())

        @Volatile
        var exitThread: Thread? = null

        override fun onLine(line: String) {
            lines += line
            if (line == failOn) throw IllegalStateException("chi ascolta ha fallito")
        }

        override fun onUnexpectedExit(run: Long, code: Int?) {
            exitThread = Thread.currentThread()
            exits += run to code
        }

        override fun onLockBusy(run: Long) {}
    }

    /** Processo finto che ignora SIGTERM e muore solo [dieAfterKillMs] dopo il kill. */
    private class SlowDeathProcess(private val dieAfterKillMs: Long) : Process() {
        private val dead = CountDownLatch(1)

        private fun blocking() = object : InputStream() {
            override fun read(): Int {
                dead.await()
                return -1
            }
        }

        override fun getOutputStream(): OutputStream = OutputStream.nullOutputStream()

        override fun getInputStream(): InputStream = blocking()

        override fun getErrorStream(): InputStream = blocking()

        override fun waitFor(): Int {
            dead.await()
            return 137
        }

        override fun waitFor(timeout: Long, unit: TimeUnit): Boolean = dead.await(timeout, unit)

        override fun exitValue(): Int = if (dead.count == 0L) 137 else throw IllegalThreadStateException()

        override fun destroy() {}

        override fun destroyForcibly(): Process {
            Thread {
                Thread.sleep(dieAfterKillMs)
                dead.countDown()
            }.apply { isDaemon = true }.start()
            return this
        }

        override fun isAlive(): Boolean = dead.count > 0

        override fun pid(): Long = 4242
    }

    @Test
    fun `un errore di chi ascolta non ferma la lettura delle righe`(@TempDir dir: Path) {
        val script = FakeScripts.write(dir, "lines", "echo prima\necho seconda\necho terza\ncat > /dev/null")
        val events = Recorder(failOn = "prima")
        val process = EngineProcess(events)
        process.start(script, System.getenv(), 1)
        assertTrue(waitUntil { events.lines.size == 3 }, "righe ricevute: ${events.lines}")
        assertEquals(listOf("prima", "seconda", "terza"), events.lines.toList())
        process.stop(2_000)
        assertFalse(process.running)
    }

    @Test
    fun `avvio fallito - evento con il numero dell'avvio, mai dal thread del chiamante`(@TempDir dir: Path) {
        val events = Recorder()
        val process = EngineProcess(events)
        process.start(dir.resolve("manca"), System.getenv(), 7)
        assertTrue(waitUntil { events.exits.isNotEmpty() })
        assertEquals(listOf<Pair<Long, Int?>>(7L to null), events.exits.toList())
        assertNotEquals(Thread.currentThread(), events.exitThread)
        assertTrue(events.lines.single().startsWith("swapdeck-cli headless:"))
        assertFalse(process.running)
    }

    @Test
    fun `dopo il kill si attende l'uscita prima di dare il motore per spento`() {
        val fake = SlowDeathProcess(dieAfterKillMs = 300)
        val process = EngineProcess(Recorder(), starter = { fake })
        process.start(Path.of("swapdeck-cli"), emptyMap(), 1)
        assertTrue(process.running)
        process.stop(graceMs = 50)
        assertFalse(fake.isAlive, "il vecchio motore è ancora vivo quando risulta spento")
        assertFalse(process.running)
    }
}
