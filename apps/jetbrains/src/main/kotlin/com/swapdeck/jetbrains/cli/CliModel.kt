package com.swapdeck.jetbrains.cli

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import java.util.concurrent.CompletableFuture

/** Timeout dei comandi di `swapdeck-cli` (gli stessi dell'estensione VS Code). */
object CliTimeouts {
    const val DEFAULT_MS: Long = 60_000

    /** Le azioni che scrivono hanno già un loro timeout in Rust (30 s, `upgrade` di più). */
    const val ACTION_MS: Long = 6 * 60_000

    /** `usage breakdown` legge a budget: margine per l'elenco dei file su dischi lenti. */
    const val USAGE_MS: Long = 2 * 60_000

    /** `setup install|upgrade`: script di uv e `uv tool` hanno 10 minuti ciascuno in Rust. */
    const val SETUP_MS: Long = 30 * 60_000

    /** «Rifai login»: 5 minuti di attesa del login più i passi di cswap prima e dopo. */
    const val RELOGIN_MS: Long = 10 * 60_000

    /** Dopo il timeout (o l'annullamento): attesa dell'uscita prima del kill, e poi del rifiuto. */
    const val KILL_GRACE_MS: Long = 2_000
}

/**
 * Errore di un comando, come `CswapError` del Rust (`kind`, `message`, `exitCode`, `errorType`,
 * `list`). [json] è l'oggetto intero, da passare tale e quale al pannello.
 */
class CswapError(val json: JsonObject) {
    val kind: String = json.str("kind") ?: "failed"
    val message: String = json.str("message") ?: ""
    val exitCode: Int? = (json["exitCode"] as? JsonPrimitive)?.intOrNull
    val errorType: String? = json.str("errorType")

    override fun toString(): String = "$kind: $message"

    companion object {
        fun of(kind: String, message: String): CswapError = CswapError(
            buildJsonObject {
                put("kind", JsonPrimitive(kind))
                put("message", JsonPrimitive(message))
            },
        )

        /** Oggetto con `kind` e `message` stringhe, altrimenti `null`. */
        fun parse(value: JsonElement?): CswapError? {
            val obj = value as? JsonObject ?: return null
            if (obj.str("kind") == null || obj.str("message") == null) return null
            return CswapError(obj)
        }

        private fun JsonObject.str(key: String): String? =
            (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
    }
}

/** Esito d'errore di `swapdeck-cli` (busta `{"error": …}`, timeout, processo non avviato). */
open class CliException(val error: CswapError) : RuntimeException(error.message) {
    constructor(kind: String, message: String) : this(CswapError.of(kind, message))
}

/** Il binario `swapdeck-cli` non esiste nel percorso scelto (plugin incompleto o `cliPath` sbagliato). */
class CliNotFoundException(message: String) : CliException("failed", message)

/** Errore come `CswapError`: quello della CLI, altrimenti `failed` col messaggio dell'eccezione. */
fun Throwable.toCswapError(): CswapError {
    val cause = unwrap()
    return if (cause is CliException) cause.error else CswapError.of("failed", cause.message ?: cause.toString())
}

/** Toglie gli strati di `CompletionException`/`ExecutionException` dei future. */
fun Throwable.unwrap(): Throwable {
    var t: Throwable = this
    while ((t is java.util.concurrent.CompletionException || t is java.util.concurrent.ExecutionException) &&
        t.cause != null
    ) {
        t = t.cause!!
    }
    return t
}

/** Stdin lasciato aperto per i comandi interattivi (`relogin start`): una riga per comando. */
interface StdinChannel {
    /** `false` se il processo è già chiuso o la scrittura fallisce. */
    fun write(text: String): Boolean

    fun end()
}

/**
 * Opzioni di un comando.
 *
 * @property stdin testo su stdin: le azioni passano da qui, un token non va mai negli argomenti.
 * @property env variabili aggiunte a quelle dell'IDE (dopo, vale sempre `SWAPDECK_LOG_SOURCE`).
 * @property onEvent comandi che scrivono un evento JSON per riga (`setup install|upgrade`): ogni
 *   riga con `event` stringa arriva qui appena completa (dal thread che legge stdout); l'esito è
 *   l'ultima riga.
 * @property channel stdin resta aperto (dopo [stdin], se c'è) e si passa al chiamante, prima che
 *   `run` restituisca; si chiude all'uscita del processo.
 */
data class CliOptions(
    val stdin: String? = null,
    val timeoutMs: Long = CliTimeouts.DEFAULT_MS,
    val env: Map<String, String> = emptyMap(),
    val onEvent: ((JsonObject) -> Unit)? = null,
    val channel: ((StdinChannel) -> Unit)? = null,
)

/**
 * Chi lancia `swapdeck-cli`: l'esito è il JSON di stdout. Il future fallisce con [CliException];
 * annullarlo (`cancel`) termina il processo.
 */
fun interface CliCaller {
    fun run(args: List<String>, options: CliOptions): CompletableFuture<JsonElement>

    fun run(vararg args: String): CompletableFuture<JsonElement> = run(args.toList(), CliOptions())
}

/**
 * Versione bloccante, per i thread in background: rilancia l'errore originale; un'interruzione
 * annulla il comando (e termina il processo).
 */
fun CliCaller.runBlocking(args: List<String>, options: CliOptions = CliOptions()): JsonElement {
    val future = run(args, options)
    try {
        return future.get()
    } catch (e: InterruptedException) {
        future.cancel(true)
        throw e
    } catch (e: java.util.concurrent.ExecutionException) {
        throw e.unwrap()
    }
}

/** Campo stringa di un oggetto JSON, se c'è. */
internal fun JsonObject.string(key: String): String? =
    (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

/** Campo intero di un oggetto JSON, se c'è. */
internal fun JsonObject.long(key: String): Long? =
    (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.content?.toLongOrNull()

/** Campo booleano di un oggetto JSON, se c'è. */
internal fun JsonObject.bool(key: String): Boolean? =
    (this[key] as? JsonPrimitive)?.takeIf { !it.isString }?.content?.toBooleanStrictOrNull()
