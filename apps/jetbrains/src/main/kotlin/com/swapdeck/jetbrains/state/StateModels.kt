package com.swapdeck.jetbrains.state

import com.swapdeck.jetbrains.cli.CswapError
import com.swapdeck.jetbrains.cli.bool
import com.swapdeck.jetbrains.cli.long
import com.swapdeck.jetbrains.cli.string
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject

/**
 * Esito di `swapdeck-cli state` (`CliState` della CLI): il JSON intero in [json] (per il pannello e
 * per `ide status`), più i pochi campi che servono al plugin.
 */
class CliState(val json: JsonObject) {
    /** `ListResponse`, `null` se la lettura è fallita. */
    val list: JsonObject? = json["list"] as? JsonObject
    val error: CswapError? = CswapError.parse(json["error"])
    val updatedAtMs: Long = json.long("updatedAtMs") ?: 0
    val engine: JsonObject = json["engine"] as? JsonObject ?: DEFAULT_ENGINE

    /** Un'istanza del motore tiene il lock ed è viva (può essere quella di questo processo). */
    val engineRunning: Boolean = engine.bool("running") ?: false

    /** Config o lock non leggibili. */
    val engineProblem: String? = engine.string("problem")

    /** Lock tenuto da un motore vivo; `null` se non si sa (config o lock non leggibili). */
    val lockHeldElsewhere: Boolean? = if (engineProblem != null) null else engineRunning

    /** Stesso stato con la lista prodotta da un'azione. */
    fun withList(list: JsonObject, nowMs: Long): CliState = CliState(
        JsonObject(json + mapOf("list" to list, "error" to JsonNull, "updatedAtMs" to JsonPrimitive(nowMs))),
    )

    override fun toString(): String = json.toString()

    companion object {
        private val DEFAULT_ENGINE = buildJsonObject {
            put("enabled", JsonPrimitive(false))
            put("dryRun", JsonPrimitive(false))
            put("pollIntervalSeconds", JsonPrimitive(60))
            put("running", JsonPrimitive(false))
            put("lock", JsonNull)
        }

        /** Stato di una lettura fallita: errore della CLI, motore dell'ultimo stato noto. */
        fun failed(error: CswapError, previous: CliState?, nowMs: Long): CliState = CliState(
            buildJsonObject {
                put("list", JsonNull)
                put("error", error.json)
                put("updatedAtMs", JsonPrimitive(nowMs))
                put("engine", previous?.engine ?: DEFAULT_ENGINE)
            },
        )
    }
}

/**
 * Riepilogo per la barra di stato (`swapdeck-cli ide status`, tipo `IdeStatus`): la logica sta nel
 * Rust, qui solo il JSON e il livello.
 */
class IdeStatus(val json: JsonObject) {
    /** `unknown`, `ok`, `warn`, `danger` o `error`. */
    val level: String = json.string("level") ?: "unknown"

    override fun toString(): String = json.toString()

    companion object {
        /** Riepilogo d'errore quando la CLI stessa non risponde (niente da chiedere al Rust). */
        fun error(error: CswapError, nowMs: Long): IdeStatus = IdeStatus(
            buildJsonObject {
                put("level", JsonPrimitive("error"))
                put(
                    "content",
                    buildJsonObject {
                        put("kind", JsonPrimitive("error"))
                        put("error", error.json)
                    },
                )
                put(
                    "engine",
                    buildJsonObject {
                        put("holder", JsonPrimitive("off"))
                        put("pid", JsonNull)
                        put("switching", JsonPrimitive(false))
                        put("dryRun", JsonPrimitive(false))
                        put("activeMap", JsonNull)
                    },
                )
                put("updatedAtMs", JsonPrimitive(nowMs))
                put(
                    "thresholds",
                    buildJsonObject {
                        put("warn", JsonPrimitive(80))
                        put("danger", JsonPrimitive(95))
                    },
                )
            },
        )

        /** Argomenti di `ide status` con lo stato su stdin: soglie e PID del motore di questo processo. */
        fun args(thresholds: List<Int>, enginePid: Long?): List<String> = buildList {
            add("ide")
            add("status")
            add("--thresholds")
            add(thresholds.joinToString(","))
            if (enginePid != null) {
                add("--engine-pid")
                add(enginePid.toString())
            }
            add("-")
        }
    }
}
