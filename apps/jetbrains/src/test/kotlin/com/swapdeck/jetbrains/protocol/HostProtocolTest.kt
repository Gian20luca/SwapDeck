package com.swapdeck.jetbrains.protocol

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/** `protocol.json` di packages/host è nel jar e ha la forma attesa (la copertura del ponte arriva col ponte). */
class HostProtocolTest {
    @Test
    fun `protocol json nelle risorse con i 33 metodi`() {
        val methods = HostProtocol.json.getValue("requestMethods").jsonArray
        assertEquals(33, methods.size)
        assertEquals(33, HostProtocol.requestMethods.size, "metodi duplicati")
        assertTrue("listAccounts" in HostProtocol.requestMethods)
    }

    @Test
    fun `sezioni principali presenti`() {
        val json = HostProtocol.json
        assertTrue(json["actionKinds"] is JsonArray)
        assertTrue(json["safeClaudeArgs"] is JsonArray)
        assertTrue(json["limits"] is JsonObject)
        assertTrue(json["defaults"] is JsonObject)
        assertTrue(json["sync"] is JsonObject)
        assertEquals(json.getValue("actionKinds").jsonArray.size, HostProtocol.lists.actionKinds.size)
        assertTrue(HostProtocol.lists.actionKinds.isNotEmpty())
        assertTrue(HostProtocol.lists.safeClaudeArgs.isNotEmpty())
    }
}
