package com.swapdeck.jetbrains.engine

import com.swapdeck.jetbrains.cli.CliEnv
import com.swapdeck.jetbrains.testing.FakeScripts
import com.swapdeck.jetbrains.testing.FakeScripts.waitUntil
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.BeforeAll
import org.junit.jupiter.api.BeforeEach
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.TestInstance
import org.junit.jupiter.api.condition.DisabledOnOs
import org.junit.jupiter.api.condition.OS
import org.junit.jupiter.api.io.TempDir
import java.nio.file.Files
import java.nio.file.Path
import java.util.Collections
import java.util.concurrent.TimeUnit

/** [EngineController] con [EngineProcess] reale contro finti `swapdeck-cli headless`. */
@DisabledOnOs(OS.WINDOWS)
@TestInstance(TestInstance.Lifecycle.PER_CLASS)
class EngineControllerTest {
    private lateinit var dir: Path
    private lateinit var busy: Path
    private lateinit var normal: Path
    private lateinit var crash: Path
    private lateinit var deaf: Path

    private val notification =
        """{"event":"notification","notification":{"kind":"threshold","severity":"warning","key":"k1","account":2,""" +
            """"title":{"it":"Soglia","en":"Threshold"},"body":{"it":"Corpo","en":"Body"}}}"""

    @BeforeAll
    fun scripts(@TempDir temp: Path) {
        dir = temp
        busy = FakeScripts.write(dir, "busy", "echo 'lock occupato' >&2\nexit 3")
        normal = FakeScripts.write(
            dir,
            "normal",
            """
            echo "${'$'}* ${'$'}SWAPDECK_LOG_SOURCE" > "${'$'}ARGS_FILE"
            echo '$notification'
            echo 'giro completato'
            cat > /dev/null
            echo 'fermo'
            """,
        )
        crash = FakeScripts.write(dir, "crash", "echo 'config non valida' >&2\nexit 7")
        deaf = FakeScripts.write(dir, "deaf", "trap '' TERM\necho $$ > \"\$PID_FILE\"\nexec sleep 30")
    }

    private class Recorder : EngineControllerEvents {
        val logs: MutableList<String> = Collections.synchronizedList(mutableListOf())
        val warnings: MutableList<String> = Collections.synchronizedList(mutableListOf())
        val notes: MutableList<SwapDeckNotification> = Collections.synchronizedList(mutableListOf())
        val exits: MutableList<Int?> = Collections.synchronizedList(mutableListOf())

        @Volatile
        var changes = 0

        override fun log(message: String) {
            logs += message
        }

        override fun warn(message: String) {
            warnings += message
        }

        override fun notification(notification: SwapDeckNotification) {
            notes += notification
        }

        override fun unexpectedExit(code: Int?) {
            exits += code
        }

        override fun changed() {
            changes++
        }

        fun starts() = logs.count { it.startsWith("[motore] avvio") }
    }

    @Volatile
    private var enabled = true

    @Volatile
    private lateinit var script: Path
    private var recorder = Recorder()
    private val controllers = mutableListOf<EngineController>()

    @BeforeEach
    fun reset() {
        recorder = Recorder()
        enabled = true
    }

    private fun controller(grace: Long = 5_000): EngineController = EngineController(
        enabled = { enabled },
        cliPath = { script },
        env = {
            CliEnv.of(
                System.getenv(),
                mapOf("ARGS_FILE" to dir.resolve("args").toString(), "PID_FILE" to dir.resolve("deaf.pid").toString()),
            )
        },
        events = recorder,
        stopGraceMs = grace,
    ).also { controllers += it }

    @AfterEach
    fun cleanup() {
        controllers.forEach { it.dispose(500) }
        controllers.clear()
    }

    @Test
    fun `lock occupato - attesa silenziosa e nuovo tentativo a lock libero`() {
        script = busy
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { engine.waitingLock })
        assertFalse(engine.running)
        assertTrue(recorder.exits.isEmpty(), "nessun avviso per il lock occupato")
        assertTrue(recorder.logs.any { it.contains("lock occupato") })

        engine.onLockState(true)
        engine.onLockState(null)
        assertEquals(1, recorder.starts(), "lock ancora altrui o non noto: niente nuovo avvio")

        script = normal
        engine.onLockState(false)
        assertTrue(waitUntil { engine.running })
        assertEquals(2, recorder.starts())
        assertFalse(engine.waitingLock)

        enabled = false
        engine.sync().get(10, TimeUnit.SECONDS)
        assertFalse(engine.running)
        assertTrue(recorder.exits.isEmpty())
    }

    @Test
    fun `argomenti, notifiche e arresto chiudendo stdin`() {
        script = normal
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { recorder.notes.isNotEmpty() && recorder.logs.any { it == "[motore] giro completato" } })
        assertTrue(engine.running)
        assertTrue(engine.pid != null)
        assertEquals("headless --stop-on-stdin-eof --notify=stdout jetbrains", Files.readString(dir.resolve("args")).trim())
        val note = recorder.notes.single()
        assertEquals("threshold", note.kind)
        assertEquals("Soglia", note.title.it)
        assertEquals(2L, note.account)
        assertFalse(recorder.logs.any { it.contains("\"notification\"") }, "la notifica non va nel log")

        enabled = false
        val started = System.nanoTime()
        engine.sync().get(10, TimeUnit.SECONDS)
        assertTrue(TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - started) < 4_000, "uscita pulita, non dopo l'attesa")
        assertFalse(engine.running)
        assertTrue(waitUntil { recorder.logs.contains("[motore] fermo") }, "il motore ha visto la fine di stdin")
        assertTrue(recorder.exits.isEmpty(), "arresto chiesto: nessun avviso")
    }

    @Test
    fun `uscita inattesa - un solo avviso e nessun riavvio`() {
        script = crash
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { recorder.exits.isNotEmpty() })
        assertEquals(listOf<Int?>(7), recorder.exits.toList())
        assertTrue(recorder.warnings.any { it.contains("codice 7") })
        assertFalse(engine.running)
        engine.onLockState(false)
        Thread.sleep(200)
        assertEquals(1, recorder.starts(), "non era in attesa del lock: nessun nuovo tentativo")
        assertEquals(1, recorder.exits.size)
    }

    @Test
    fun `binario mancante - avviso con codice nullo`() {
        script = dir.resolve("manca")
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { recorder.exits.isNotEmpty() })
        assertEquals(listOf<Int?>(null), recorder.exits.toList())
        assertTrue(recorder.logs.any { it.startsWith("swapdeck-cli headless:") || it.contains("headless") })
    }

    @Test
    fun `pausa e ripresa`() {
        script = normal
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { engine.running })
        val resume = engine.pause("upgrade").get(10, TimeUnit.SECONDS)
        assertFalse(engine.running)
        assertTrue(engine.paused)
        engine.sync().get(5, TimeUnit.SECONDS)
        assertFalse(engine.running, "in pausa non riparte")
        resume.resume()
        assertTrue(waitUntil { engine.running })
        resume.resume()
        assertFalse(engine.paused)
        assertTrue(recorder.exits.isEmpty())
    }

    @Test
    fun `arresto di un motore che ignora stdin e SIGTERM`() {
        script = deaf
        val engine = controller(grace = 200)
        engine.sync()
        val pidFile = dir.resolve("deaf.pid")
        assertTrue(waitUntil { Files.exists(pidFile) && Files.size(pidFile) > 0 })
        enabled = false
        engine.sync().get(10, TimeUnit.SECONDS)
        assertFalse(engine.running)
        assertTrue(waitUntil { !FakeScripts.alive(pidFile) })
        assertTrue(recorder.exits.isEmpty())
    }

    @Test
    fun `cambio del binario - riavvio`() {
        script = normal
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { engine.running })
        val first = engine.pid
        engine.restart().get(10, TimeUnit.SECONDS)
        assertTrue(waitUntil { engine.running })
        assertTrue(first != engine.pid)
        assertEquals(2, recorder.starts())
    }

    @Test
    fun `chiusura dell'IDE - niente più avvii`() {
        script = normal
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { engine.running })
        engine.dispose()
        assertFalse(engine.running)
        engine.sync().get(5, TimeUnit.SECONDS)
        assertFalse(engine.running)
    }

    @Test
    fun `regola del nuovo tentativo`() {
        assertTrue(EngineController.shouldRetry(true, false, true, false))
        assertFalse(EngineController.shouldRetry(false, false, true, false))
        assertFalse(EngineController.shouldRetry(true, true, true, false))
        assertFalse(EngineController.shouldRetry(true, false, false, false))
        assertFalse(EngineController.shouldRetry(true, false, true, true))
        assertFalse(EngineController.shouldRetry(true, false, true, null))
    }
}
