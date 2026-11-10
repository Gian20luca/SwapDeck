package com.swapdeck.jetbrains.protocol

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

/** Parte di `protocol.json` letta in modo tipizzato; il resto (limiti, sincronia) resta in [HostProtocol.json]. */
@Serializable
data class ProtocolLists(
    val requestMethods: List<String>,
    val actionKinds: List<String>,
    val safeClaudeArgs: List<String>,
)

/**
 * Segnale di sincronia tra gli host (sezione `sync` di `protocol.json`, formato in
 * `packages/host/README.md`).
 */
@Serializable
data class SyncRules(
    /** Segnale scritto dopo le proprie azioni (`ide-sync.json`). */
    val file: String,
    /** Nomi vecchi ancora osservati, mai scritti (`vscode-sync.json`). */
    val legacyFiles: List<String>,
    /** Config comune (`config.json`). */
    val configFile: String,
    val debounceMs: Long,
    val maxSignalBytes: Int,
)

/**
 * Elenchi e limiti del protocollo del pannello (`packages/host/protocol.json`, copiato nel jar dalla
 * build in `swapdeck/protocol.json`). Fonte unica con l'estensione VS Code: il ponte li legge da qui.
 */
object HostProtocol {
    const val RESOURCE = "/swapdeck/protocol.json"

    private val format = Json { ignoreUnknownKeys = true }

    private val text: String by lazy {
        val stream = HostProtocol::class.java.getResourceAsStream(RESOURCE)
            ?: error("$RESOURCE non trovato nel plugin")
        stream.use { it.readBytes().toString(Charsets.UTF_8) }
    }

    /** Il JSON intero; lancia se manca dal jar (errore di build). */
    val json: JsonObject by lazy { Json.parseToJsonElement(text).jsonObject }

    /** Metodi, `kind` delle azioni e argomenti di `claude` ammessi. */
    val lists: ProtocolLists by lazy { format.decodeFromString(ProtocolLists.serializer(), text) }

    /** Segnale di sincronia (`sync`). */
    val sync: SyncRules by lazy { format.decodeFromJsonElement(SyncRules.serializer(), json.getValue("sync")) }

    /** Metodi che il pannello può chiedere (`requestMethods`). */
    val requestMethods: Set<String> by lazy { lists.requestMethods.toSet() }
}
