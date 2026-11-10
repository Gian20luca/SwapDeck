package com.swapdeck.jetbrains.testing

import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.attribute.PosixFilePermissions

/**
 * Finti `swapdeck-cli`: script POSIX scritti in una cartella temporanea. Si scrivono tutti prima di
 * lanciare processi (un file aperto in scrittura durante un `fork` darebbe `ETXTBSY`).
 */
object FakeScripts {
    fun write(dir: Path, name: String, body: String): Path {
        val file = dir.resolve(name)
        Files.writeString(file, "#!/bin/sh\n" + body.trimIndent() + "\n")
        Files.setPosixFilePermissions(file, PosixFilePermissions.fromString("rwxr-xr-x"))
        return file
    }

    /** Attende che [condition] diventi vera (al massimo [timeoutMs]). */
    fun waitUntil(timeoutMs: Long = 5_000, condition: () -> Boolean): Boolean {
        val end = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < end) {
            if (condition()) return true
            Thread.sleep(20)
        }
        return condition()
    }

    /** Il processo con quel PID (letto da un file scritto dallo script) è ancora vivo? */
    fun alive(pidFile: Path): Boolean {
        val pid = Files.readString(pidFile).trim().toLong()
        return ProcessHandle.of(pid).map { it.isAlive }.orElse(false)
    }
}
