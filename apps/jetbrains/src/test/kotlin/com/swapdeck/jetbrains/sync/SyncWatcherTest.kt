package com.swapdeck.jetbrains.sync

import com.swapdeck.jetbrains.protocol.HostProtocol
import com.swapdeck.jetbrains.protocol.SyncRules
import com.swapdeck.jetbrains.testing.FakeScripts.waitUntil
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.condition.DisabledOnOs
import org.junit.jupiter.api.condition.OS
import org.junit.jupiter.api.io.TempDir
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.attribute.PosixFilePermissions
import java.util.concurrent.atomic.AtomicInteger
import kotlin.io.path.listDirectoryEntries
import kotlin.io.path.name

/** [SyncWatcher] su una cartella temporanea vera (inotify su Linux). */
@DisabledOnOs(OS.WINDOWS)
class SyncWatcherTest {
    @TempDir
    lateinit var dir: Path

    /** Regole di `protocol.json` con un debounce breve per i test. */
    private val rules: SyncRules = HostProtocol.sync.copy(debounceMs = 150)
    private val watchers = mutableListOf<SyncWatcher>()

    private fun watcher(counter: AtomicInteger, source: String = SyncWatcher.newSource()) =
        SyncWatcher(dir, onChange = { counter.incrementAndGet() }, source = source, rules = rules).also { watchers += it }

    @AfterEach
    fun close() = watchers.forEach { it.close() }

    /** Lascia passare debounce ed eventi in ritardo. */
    private fun settle() = Thread.sleep(rules.debounceMs + 600)

    @Test
    fun `costanti da protocol json`() {
        val sync = HostProtocol.sync
        assertEquals("ide-sync.json", sync.file)
        assertEquals(listOf("vscode-sync.json"), sync.legacyFiles)
        assertEquals("config.json", sync.configFile)
        assertEquals(750L, sync.debounceMs)
        assertEquals(4096, sync.maxSignalBytes)
    }

    @Test
    fun `due istanze vedono il segnale dell'altra e ignorano il proprio`() {
        val a = AtomicInteger()
        val b = AtomicInteger()
        val wa = watcher(a)
        val wb = watcher(b)
        assertTrue(wa.start())
        assertTrue(wb.start())
        assertTrue(wa.watching)

        assertTrue(wa.signal())
        assertTrue(waitUntil { b.get() == 1 })
        settle()
        assertEquals(0, a.get(), "il proprio segnale non fa rileggere")
        assertEquals(1, b.get())

        assertTrue(wb.signal())
        assertTrue(waitUntil { a.get() == 1 })
        settle()
        assertEquals(1, b.get())
    }

    @Test
    fun `segnale atomico, privato e senza temporanei`() {
        val w = watcher(AtomicInteger(), source = "123-abc")
        assertTrue(w.signal())
        val file = dir.resolve("ide-sync.json")
        val body = Json.parseToJsonElement(Files.readString(file)).jsonObject
        assertEquals(setOf("source", "at"), body.keys)
        assertEquals("123-abc", body["source"]!!.jsonPrimitive.content)
        assertTrue(body["at"]!!.jsonPrimitive.long > 0)
        assertEquals("rw-------", PosixFilePermissions.toString(Files.getPosixFilePermissions(file)))
        assertTrue(w.isOwnSignal())
        assertTrue(w.signal(), "riscrittura sopra il precedente")
        assertEquals(listOf("ide-sync.json"), dir.listDirectoryEntries().map { it.name })
    }

    @Test
    fun `nome vecchio e config fanno rileggere, altri file no`() {
        val count = AtomicInteger()
        assertTrue(watcher(count).start())
        Files.writeString(dir.resolve("altro.json"), "{}")
        Files.writeString(dir.resolve(".ide-sync.json.x.tmp"), "{}")
        settle()
        assertEquals(0, count.get())
        Files.writeString(dir.resolve("vscode-sync.json"), """{"source":"vecchio","at":1}""")
        assertTrue(waitUntil { count.get() == 1 })
        settle()
        Files.writeString(dir.resolve("config.json"), "{}")
        assertTrue(waitUntil { count.get() == 2 })
    }

    @Test
    fun `eventi raggruppati`() {
        val count = AtomicInteger()
        assertTrue(watcher(count).start())
        repeat(5) {
            Files.writeString(dir.resolve("config.json"), "{\"n\":$it}")
            Thread.sleep(20)
        }
        assertTrue(waitUntil { count.get() >= 1 })
        settle()
        assertEquals(1, count.get())
    }

    @Test
    fun `segnale come link - non si segue e vale come altrui`() {
        val own = "999-own"
        val target = dir.resolve("bersaglio.txt")
        Files.writeString(target, """{"source":"$own","at":1}""")
        val count = AtomicInteger()
        val w = watcher(count, source = own)
        assertTrue(w.start())
        Files.createSymbolicLink(dir.resolve("ide-sync.json"), target)
        assertTrue(waitUntil { count.get() == 1 }, "il link non è il proprio segnale")
        assertFalse(w.isOwnSignal())

        // Scrivere il segnale sostituisce il link, non il file a cui punta.
        assertTrue(w.signal())
        assertFalse(Files.isSymbolicLink(dir.resolve("ide-sync.json")))
        assertEquals("""{"source":"$own","at":1}""", Files.readString(target))
        assertTrue(w.isOwnSignal())
    }

    @Test
    fun `segnale troppo grande o non JSON vale come altrui`() {
        val w = watcher(AtomicInteger(), source = "1-a")
        val file = dir.resolve("ide-sync.json")
        Files.writeString(file, """{"source":"1-a","at":1,"pad":"${"x".repeat(5000)}"}""")
        assertFalse(w.isOwnSignal())
        Files.writeString(file, "non json")
        assertFalse(w.isOwnSignal())
        Files.writeString(file, """{"source":"1-a","at":1}""")
        assertTrue(w.isOwnSignal())
    }

    @Test
    fun `cartella mancante - spento, poi creata privata`() {
        val missing = dir.resolve("sub/swapdeck")
        val w = SyncWatcher(missing, onChange = {}, rules = rules).also { watchers += it }
        assertFalse(w.start())
        assertFalse(w.signal())
        SyncWatcher.ensurePrivateDir(missing, windows = false)
        assertEquals("rwx------", PosixFilePermissions.toString(Files.getPosixFilePermissions(missing)))
        assertTrue(w.start())
    }

    @Test
    fun `cartella dalla risposta di config get`() {
        val view = Json.parseToJsonElement("""{"path":"/home/u/.config/swapdeck/config.json","fromFile":false}""").jsonObject
        assertEquals(Path.of("/home/u/.config/swapdeck"), SyncWatcher.configDir(view))
    }

    @Test
    fun `dopo la chiusura niente avvisi`() {
        val count = AtomicInteger()
        val w = watcher(count)
        assertTrue(w.start())
        Files.writeString(dir.resolve("config.json"), "{}")
        w.close()
        assertFalse(w.watching)
        settle()
        assertEquals(0, count.get())
    }

    @Test
    fun `sorgente con PID`() {
        val source = SyncWatcher.newSource()
        assertTrue(source.startsWith("${ProcessHandle.current().pid()}-"))
        assertEquals(12, source.substringAfter('-').length)
    }
}
