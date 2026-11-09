package com.swapdeck.jetbrains.engine

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class NotificationRulesTest {
    private val note =
        """{"kind":"reloginRequired","severity":"critical","key":"r1","title":{"it":"Titolo","en":"Title"},""" +
            """"body":{"it":"Testo","en":"Text"}}"""

    @Test
    fun `riga del motore con notifica`() {
        val n = NotificationRules.parseEngineEvent("""{"event":"notification","notification":$note}""")!!
        assertEquals("reloginRequired", n.kind)
        assertEquals("critical", n.severity)
        assertEquals("r1", n.key)
        assertNull(n.account)
        assertEquals("Title", n.title.of(italian = false))
        assertEquals("Testo", n.body.of(italian = true))
    }

    @Test
    fun `righe di log o notifiche incomplete`() {
        assertNull(NotificationRules.parseEngineEvent("2026-11-09 giro completato"))
        assertNull(NotificationRules.parseEngineEvent("{non json"))
        assertNull(NotificationRules.parseEngineEvent("""{"event":"other","notification":$note}"""))
        assertNull(NotificationRules.parseEngineEvent("""{"event":"notification"}"""))
        assertNull(
            NotificationRules.parseEngineEvent(
                """{"event":"notification","notification":{"kind":"x","severity":"info","title":{"it":"a"},"body":{"it":"b","en":"c"}}}""",
            ),
        )
        assertNull(NotificationRules.parseEngineEvent(""" {"event":"notification","notification":$note}"""))
    }

    @Test
    fun `chi chiede le notifiche`() {
        assertTrue(NotificationRules.shouldPoll(localEngineRunning = false, lockHeldElsewhere = false))
        assertTrue(NotificationRules.shouldPoll(localEngineRunning = false, lockHeldElsewhere = null))
        assertFalse(NotificationRules.shouldPoll(localEngineRunning = false, lockHeldElsewhere = true))
        assertFalse(NotificationRules.shouldPoll(localEngineRunning = true, lockHeldElsewhere = false))
    }

    @Test
    fun `esito di notifications poll`() {
        fun poll(sender: String) =
            Json.parseToJsonElement("""{"sender":"$sender","notifications":[$note, {"kind":"bad"}],"language":"it"}""").jsonObject
        assertEquals(listOf("r1"), NotificationRules.fromPoll(poll("caller")).map { it.key })
        assertTrue(NotificationRules.fromPoll(poll("engine")).isEmpty())
        assertTrue(NotificationRules.fromPoll(poll("busy")).isEmpty())
    }
}

class LineSplitterTest {
    private fun split(vararg chunks: ByteArray, max: Int = 4096): List<String> {
        val lines = mutableListOf<String>()
        val splitter = LineSplitter(lines::add, max)
        chunks.forEach { splitter.push(it) }
        splitter.end()
        return lines
    }

    @Test
    fun `righe a cavallo dei blocchi, CRLF e righe vuote`() {
        assertEquals(listOf("uno", "due", "tre"), split("u".toByteArray(), "no\r\n\n  \ndue\nt".toByteArray(), "re".toByteArray()))
    }

    @Test
    fun `UTF-8 spezzato tra due blocchi`() {
        val bytes = "perché\n".toByteArray()
        val cut = bytes.indexOfFirst { it < 0 } + 1
        assertEquals(listOf("perché"), split(bytes.copyOfRange(0, cut), bytes.copyOfRange(cut, bytes.size)))
    }

    @Test
    fun `riga troppo lunga tagliata`() {
        val lines = split(("a".repeat(10) + "\nb\n").toByteArray(), max = 4)
        assertEquals(listOf("aaaa…", "b"), lines)
    }
}
