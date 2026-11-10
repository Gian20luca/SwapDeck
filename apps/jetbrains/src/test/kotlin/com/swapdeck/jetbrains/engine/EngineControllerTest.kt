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
import java.util.concurrent.CompletableFuture
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

        /** Chiamato a ogni evento (prova del lock, errori di chi ascolta). */
        @Volatile
        var probe: () -> Unit = {}

        override fun log(message: String) {
            logs += message
            probe()
        }

        override fun warn(message: String) {
            warnings += message
            probe()
        }

        override fun notification(notification: SwapDeckNotification) {
            notes += notification
            probe()
        }

        override fun unexpectedExit(code: Int?) {
            exits += code
            probe()
        }

        override fun changed() {
            changes++
            probe()
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

    /** Eventi del processo visti dal controller (per simulare quelli di un avvio superato). */
    @Volatile
    private var processEvents: EngineProcess.Events? = null

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
        processFactory = { events -> processEvents = events; EngineProcess(events) },
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
        assertTrue(recorder.warnings.contains("[motore] avvio non riuscito"), recorder.warnings.toString())
    }

    @Test
    fun `pausa e ripresa`() {
        script = normal
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { engine.running })
        val pause = engine.pause("upgrade")
        assertTrue(engine.paused, "la pausa vale subito, prima dell'arresto")
        pause.stopped.get(10, TimeUnit.SECONDS)
        assertFalse(engine.running)
        engine.sync().get(5, TimeUnit.SECONDS)
        assertFalse(engine.running, "in pausa non riparte")
        pause.resume()
        assertTrue(waitUntil { engine.running })
        pause.resume()
        assertFalse(engine.paused)
        assertTrue(recorder.exits.isEmpty())
    }

    @Test
    fun `pausa - attesa dell'arresto annullata, la ripresa vale lo stesso`() {
        script = normal
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { engine.running })
        val pause = engine.pause("relogin")
        // Il chiamante rinuncia ad aspettare (annullato o eccezione) e riprende nel finally.
        pause.stopped.cancel(true)
        pause.resume()
        assertFalse(engine.paused)
        assertTrue(waitUntil { engine.running }, "il motore riparte")
        assertTrue(pause.isResumed)
    }

    @Test
    fun `eventi di un avvio superato - ignorati`() {
        script = busy
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { engine.waitingLock })
        script = normal
        engine.onLockState(false)
        assertTrue(waitUntil { engine.running })
        assertFalse(engine.waitingLock)
        // L'uscita 3 del primo avvio arriva in ritardo, quando il secondo è già partito.
        val stale = processEvents!!
        stale.onLockBusy(1)
        assertFalse(engine.waitingLock, "il lock occupato di un avvio vecchio non mette in attesa")
        stale.onUnexpectedExit(1, 9)
        assertTrue(recorder.exits.isEmpty(), "nessun avviso per un avvio vecchio")
        assertTrue(engine.running)
    }

    @Test
    fun `uscita inattesa dopo il lock occupato - fine dell'attesa`() {
        script = busy
        val engine = controller()
        engine.sync()
        assertTrue(waitUntil { engine.waitingLock })
        // Lo stesso avvio (numero 1) risulta poi uscito per errore: niente più nuovi tentativi.
        processEvents!!.onUnexpectedExit(1, 7)
        assertFalse(engine.waitingLock)
        engine.onLockState(false)
        Thread.sleep(200)
        assertEquals(1, recorder.starts(), "nessun riavvio a ogni lettura dello stato")
    }

    @Test
    fun `un errore di chi ascolta le notifiche non ferma le righe dopo`() {
        script = normal
        val engine = controller()
        recorder.probe = { if (recorder.notes.isNotEmpty() && recorder.logs.none { it == "[motore] giro completato" }) error("ascolto rotto") }
        engine.sync()
        assertTrue(waitUntil { recorder.logs.contains("[motore] giro completato") }, recorder.logs.toString())
        assertEquals(1, recorder.notes.size)
    }

    @Test
    fun `eventi sempre fuori dal lock del controller`() {
        // Chi ascolta aspetta un altro thread che legge il controller: col lock preso sarebbe un deadlock.
        lateinit var engine: EngineController
        val blocked: MutableList<String> = Collections.synchronizedList(mutableListOf())
        recorder.probe = {
            try {
                CompletableFuture.supplyAsync { engine.paused }.get(1, TimeUnit.SECONDS)
            } catch (e: Exception) {
                blocked += Thread.currentThread().name
            }
        }
        script = dir.resolve("manca")
        engine = controller()
        engine.sync()
        assertTrue(waitUntil { recorder.exits.isNotEmpty() && recorder.changes >= 2 })

        script = normal
        engine.sync().get(10, TimeUnit.SECONDS)
        assertTrue(waitUntil { engine.running })
        engine.pause("upgrade").also { it.stopped.get(10, TimeUnit.SECONDS) }.resume()
        assertTrue(waitUntil { engine.running })
        enabled = false
        engine.sync().get(10, TimeUnit.SECONDS)
        assertTrue(blocked.isEmpty(), "eventi col lock preso: $blocked")
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
