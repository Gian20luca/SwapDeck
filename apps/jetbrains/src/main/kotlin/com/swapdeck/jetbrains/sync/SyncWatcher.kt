package com.swapdeck.jetbrains.sync

import com.swapdeck.jetbrains.SwapDeckThreads
import com.swapdeck.jetbrains.cli.CliPaths
import com.swapdeck.jetbrains.cli.string
import com.swapdeck.jetbrains.protocol.HostProtocol
import com.swapdeck.jetbrains.protocol.SyncRules
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import java.io.IOException
import java.io.InterruptedIOException
import java.nio.ByteBuffer
import java.nio.file.AccessDeniedException
import java.nio.file.ClosedWatchServiceException
import java.nio.file.FileSystemException
import java.nio.file.Files
import java.nio.file.LinkOption
import java.nio.file.NoSuchFileException
import java.nio.file.Path
import java.nio.file.StandardCopyOption
import java.nio.file.StandardOpenOption
import java.nio.file.StandardWatchEventKinds
import java.nio.file.WatchEvent
import java.nio.file.WatchService
import java.nio.file.attribute.BasicFileAttributes
import java.nio.file.attribute.FileAttribute
import java.nio.file.attribute.PosixFilePermissions
import java.security.SecureRandom
import java.util.HexFormat
import java.util.concurrent.ExecutorService
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

/**
 * Sincronia con gli altri host sulla stessa cartella di SwapDeck (finestre e istanze di VS Code,
 * altre IDE JetBrains, app desktop), porting di `sync.ts`.
 *
 * Osserva la cartella della config con `java.nio.file.WatchService` e reagisce solo a
 * `config.json`, al segnale `ide-sync.json` e ai nomi vecchi (`legacyFiles`); gli eventi si
 * raggruppano ([SyncRules.debounceMs]) e chiamano [onChange] su [executor]. Il proprio segnale
 * (stesso [source]) non fa rileggere. Nomi e limiti da `protocol.json` ([HostProtocol.sync]).
 *
 * Perché `WatchService` e non il file watcher della piattaforma: la cartella sta fuori dai progetti
 * (il VFS la vedrebbe solo con `addRootToWatch` e un refresh, e la terrebbe in cache), servono solo
 * tre nomi in una cartella sola, e così la logica si prova senza IDE. Su macOS il JDK osserva per
 * polling: si chiede la sensibilità alta dove c'è; il polling dello stato resta la rete di sicurezza.
 */
class SyncWatcher(
    private val dir: Path,
    private val onChange: () -> Unit,
    val source: String = newSource(),
    private val rules: SyncRules = HostProtocol.sync,
    private val log: (String) -> Unit = {},
    private val scheduler: ScheduledExecutorService = SwapDeckThreads.scheduler,
    private val executor: ExecutorService = SwapDeckThreads.io,
    private val clock: () -> Long = System::currentTimeMillis,
    private val windows: Boolean = CliPaths.isWindows,
) : AutoCloseable {
    private val lock = Any()
    private var service: WatchService? = null
    private var pending: ScheduledFuture<*>? = null
    private var closed = false

    val watching: Boolean get() = synchronized(lock) { service != null }

    /** Avvia l'osservazione; senza cartella (o con errore) resta spento e si può ritentare. */
    fun start(): Boolean {
        synchronized(lock) {
            if (closed) return false
            if (service != null) return true
            if (!Files.isDirectory(dir)) return false
            val ws = try {
                dir.fileSystem.newWatchService()
            } catch (e: IOException) {
                log("sync: watcher non disponibile (${e.javaClass.simpleName})")
                return false
            }
            try {
                register(ws)
            } catch (e: Exception) {
                runCatching { ws.close() }
                if (e !is NoSuchFileException) log("sync: watcher non disponibile (${e.javaClass.simpleName})")
                return false
            }
            try {
                executor.execute { loop(ws) }
            } catch (_: RejectedExecutionException) {
                // Pool chiusi: plugin in chiusura.
                runCatching { ws.close() }
                return false
            }
            service = ws
            return true
        }
    }

    private fun register(ws: WatchService) {
        val kinds = arrayOf(
            StandardWatchEventKinds.ENTRY_CREATE,
            StandardWatchEventKinds.ENTRY_MODIFY,
            StandardWatchEventKinds.ENTRY_DELETE,
        )
        val modifiers = HIGH_SENSITIVITY
        if (modifiers.isNotEmpty()) {
            try {
                dir.register(ws, kinds, *modifiers)
                return
            } catch (_: UnsupportedOperationException) {
                // Modificatore non accettato: registrazione semplice.
            }
        }
        dir.register(ws, *kinds)
    }

    private fun loop(ws: WatchService) {
        try {
            while (true) {
                val key = ws.take()
                for (event in key.pollEvents()) {
                    if (event.kind() == StandardWatchEventKinds.OVERFLOW) {
                        onEvent(null)
                    } else {
                        onEvent((event.context() as? Path)?.fileName?.toString())
                    }
                }
                if (!key.reset()) {
                    log("sync: cartella non più osservabile, resta il polling")
                    break
                }
            }
        } catch (_: ClosedWatchServiceException) {
        } catch (_: InterruptedException) {
        } finally {
            synchronized(lock) { if (service === ws) service = null }
            runCatching { ws.close() }
        }
    }

    /** Il nome dell'evento riguarda la sincronia? `null` (eventi persi): sì, per prudenza. */
    fun isSyncEvent(name: String?): Boolean =
        name == null || name == rules.configFile || name == rules.file || name in rules.legacyFiles

    internal fun onEvent(name: String?) {
        if (!isSyncEvent(name)) return
        if (name == rules.file && isOwnSignal()) return
        synchronized(lock) {
            if (closed) return
            pending?.cancel(false)
            pending = try {
                scheduler.schedule(
                    {
                        try {
                            executor.execute { if (!synchronized(lock) { closed }) onChange() }
                        } catch (_: RejectedExecutionException) {
                        }
                    },
                    rules.debounceMs,
                    TimeUnit.MILLISECONDS,
                )
            } catch (_: RejectedExecutionException) {
                null // pool chiusi: plugin in chiusura
            }
        }
    }

    /**
     * Il segnale attuale l'ha scritto questo processo? Link (non si seguono), file non regolari,
     * troppo grandi, illeggibili o non JSON: no (si rilegge).
     */
    fun isOwnSignal(): Boolean {
        val path = dir.resolve(rules.file)
        return try {
            val attrs = Files.readAttributes(path, BasicFileAttributes::class.java, LinkOption.NOFOLLOW_LINKS)
            if (!attrs.isRegularFile || attrs.size() > rules.maxSignalBytes) return false
            val bytes = Files.newInputStream(path, LinkOption.NOFOLLOW_LINKS).use { it.readNBytes(rules.maxSignalBytes + 1) }
            if (bytes.size > rules.maxSignalBytes) return false
            val value = Json.parseToJsonElement(bytes.toString(Charsets.UTF_8)) as? JsonObject ?: return false
            value.string("source") == source
        } catch (_: Exception) {
            false
        }
    }

    /**
     * Avvisa gli altri host: segnale `{"source", "at"}` riscritto in modo atomico (temporaneo
     * creato in modo esclusivo con permessi 0600, poi rinominato sopra il precedente, senza seguire
     * link). Un errore non ferma nulla: resta il polling. Bloccante (breve): da un thread in
     * background. Restituisce `true` se il segnale è scritto.
     */
    fun signal(): Boolean {
        if (synchronized(lock) { closed }) return false
        val target = dir.resolve(rules.file)
        val tmp = dir.resolve(".${rules.file}.${randomHex(6)}.tmp")
        val body = buildJsonObject {
            put("source", JsonPrimitive(source))
            put("at", JsonPrimitive(clock()))
        }.toString().toByteArray(Charsets.UTF_8)
        return try {
            val attrs: Array<FileAttribute<*>> =
                if (windows) emptyArray() else arrayOf(PosixFilePermissions.asFileAttribute(PosixFilePermissions.fromString("rw-------")))
            Files.newByteChannel(
                tmp,
                setOf(StandardOpenOption.CREATE_NEW, StandardOpenOption.WRITE, LinkOption.NOFOLLOW_LINKS),
                *attrs,
            ).use { it.write(ByteBuffer.wrap(body)) }
            try {
                moveWithRetry(tmp, target)
            } catch (e: IOException) {
                runCatching { Files.deleteIfExists(tmp) }
                throw e
            }
            true
        } catch (_: NoSuchFileException) {
            false
        } catch (e: IOException) {
            log("sync: segnale non scritto (${e.javaClass.simpleName})")
            false
        }
    }

    /** Su Windows la rinomina sopra un file appena letto da un altro host può fallire per poco. */
    private fun moveWithRetry(from: Path, to: Path, attempts: Int = 3) {
        var attempt = 1
        while (true) {
            try {
                Files.move(from, to, StandardCopyOption.ATOMIC_MOVE)
                return
            } catch (e: FileSystemException) {
                val transient = windows && (e is AccessDeniedException || e.javaClass == FileSystemException::class.java)
                if (attempt >= attempts || !transient) throw e
                try {
                    Thread.sleep(50L * attempt)
                } catch (_: InterruptedException) {
                    // Il chiamante cancella il temporaneo; l'interruzione resta per chi la aspetta.
                    Thread.currentThread().interrupt()
                    throw InterruptedIOException("sync: segnale interrotto")
                }
                attempt++
            }
        }
    }

    override fun close() {
        val ws = synchronized(lock) {
            closed = true
            pending?.cancel(false)
            pending = null
            service.also { service = null }
        }
        runCatching { ws?.close() }
    }

    companion object {
        private val random = SecureRandom()

        /** Identifica questo processo nel segnale: PID e parte casuale. */
        fun newSource(): String = "${ProcessHandle.current().pid()}-${randomHex(6)}"

        private fun randomHex(bytes: Int): String =
            HexFormat.of().formatHex(ByteArray(bytes).also(random::nextBytes))

        /** Sensibilità alta del `WatchService` a polling (macOS), se il JDK la offre. */
        private val HIGH_SENSITIVITY: Array<WatchEvent.Modifier> = try {
            val cls = Class.forName("com.sun.nio.file.SensitivityWatchEventModifier")
            arrayOf(cls.getField("HIGH").get(null) as WatchEvent.Modifier)
        } catch (_: Throwable) {
            emptyArray()
        }

        /** Cartella della config da `swapdeck-cli config get` (campo `path`, rispetta `SWAPDECK_HOME`). */
        fun configDir(configView: JsonObject): Path {
            val path = configView.string("path") ?: throw IOException("config get: percorso mancante")
            return Path.of(path).toAbsolutePath().parent ?: throw IOException("config get: percorso senza cartella")
        }

        /**
         * Su Linux la cartella della config nasce al primo salvataggio: si crea privata (0700), così
         * anche il segnale dei cambi di account funziona da subito.
         */
        fun ensurePrivateDir(dir: Path, windows: Boolean = CliPaths.isWindows) {
            if (Files.isDirectory(dir)) return
            if (windows) {
                Files.createDirectories(dir)
            } else {
                Files.createDirectories(dir, PosixFilePermissions.asFileAttribute(PosixFilePermissions.fromString("rwx------")))
            }
        }
    }
}
