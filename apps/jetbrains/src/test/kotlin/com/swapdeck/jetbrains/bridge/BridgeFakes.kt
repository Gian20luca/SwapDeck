package com.swapdeck.jetbrains.bridge

import com.swapdeck.jetbrains.cli.CliCaller
import com.swapdeck.jetbrains.cli.CliOptions
import com.swapdeck.jetbrains.cli.StdinChannel
import com.swapdeck.jetbrains.engine.EnginePause
import com.swapdeck.jetbrains.state.CliState
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import java.util.Collections
import java.util.concurrent.CompletableFuture

internal fun json(text: String): JsonElement = Json.parseToJsonElement(text)

internal fun obj(text: String): JsonObject = json(text).jsonObject

/** Lista di prova: account attivo 6. */
internal fun fixtureList(active: String = "6"): JsonObject =
    obj("""{"activeAccountNumber":$active,"accounts":[{"number":6,"active":true}]}""")

internal fun stateWith(list: JsonObject?, error: String? = null): CliState = CliState(
    obj(
        """{"list":${list ?: "null"},"error":${error ?: "null"},"updatedAtMs":1,
           "engine":{"enabled":false,"dryRun":false,"pollIntervalSeconds":60,"running":false,"lock":null}}""",
    ),
)

/** Finta CLI che registra argomenti e opzioni; [respond] decide l'esito (di default `null` subito). */
internal class FakeCli : CliCaller {
    class Call(val args: List<String>, val options: CliOptions, val future: CompletableFuture<JsonElement>)

    val calls: MutableList<Call> = Collections.synchronizedList(mutableListOf())

    /** Risposte per primo argomento (`status`, `action`, …). */
    val responses: MutableMap<String, JsonElement> = mutableMapOf()

    /** Esito personalizzato; `null` = [responses] o `null`. */
    var respond: ((List<String>, CliOptions) -> CompletableFuture<JsonElement>?)? = null

    override fun run(args: List<String>, options: CliOptions): CompletableFuture<JsonElement> {
        val future = respond?.invoke(args, options)
            ?: CompletableFuture.completedFuture(responses[args.first()] ?: JsonNull)
        calls += Call(args, options, future)
        return future
    }

    fun args(): List<List<String>> = synchronized(calls) { calls.map { it.args } }
}

/** Stdin interattivo registrato. */
internal class RecordingChannel : StdinChannel {
    val written: MutableList<String> = Collections.synchronizedList(mutableListOf())

    @Volatile
    var open = true

    override fun write(text: String): Boolean {
        if (!open) return false
        written += text
        return true
    }

    override fun end() {
        open = false
    }
}

internal class FakeHost : BridgeHost {
    @Volatile
    var last: CliState? = stateWith(fixtureList())
    var refreshed: CliState = stateWith(fixtureList())
    val refreshes = java.util.concurrent.atomic.AtomicInteger()
    val outcomes: MutableList<JsonObject> = Collections.synchronizedList(mutableListOf())
    val order: MutableList<String> = Collections.synchronizedList(mutableListOf())
    var preferences = 0

    /** Pausa del motore: di default motore fermo subito. */
    var pause: (String) -> EnginePause = { why -> pauseWith(why, CompletableFuture.completedFuture(Unit)) }

    /** Pausa registrata in [order] con l'arresto [stopped]. */
    fun pauseWith(why: String, stopped: CompletableFuture<Unit>): EnginePause {
        order += "pause:$why"
        return EnginePause(stopped) { order += "resume" }
    }

    override var clientInfo: String? = null

    override fun lastState(): CliState? = last

    override fun refresh(): CompletableFuture<CliState> {
        refreshes.incrementAndGet()
        return CompletableFuture.completedFuture(refreshed)
    }

    override fun afterAction(outcome: JsonObject): CompletableFuture<*> {
        outcomes += outcome
        return CompletableFuture.completedFuture(Unit)
    }

    override fun pauseEngine(why: String): EnginePause = pause(why)

    override fun changed() {
        order += "changed"
    }

    override fun preferencesChanged() {
        preferences++
    }
}

internal class FakeUi : BridgeUi {
    var confirmAnswer = true
    var setupAnswer = true
    var pickResult: String? = "/scelto"
    val confirmations: MutableList<Pair<String, Confirmation>> = Collections.synchronizedList(mutableListOf())
    val setups: MutableList<String> = Collections.synchronizedList(mutableListOf())
    val picks: MutableList<Pair<PickKind, PickOptions>> = Collections.synchronizedList(mutableListOf())
    val copied: MutableList<String> = Collections.synchronizedList(mutableListOf())
    val folders: MutableList<String> = Collections.synchronizedList(mutableListOf())
    val commands: MutableList<List<String>> = Collections.synchronizedList(mutableListOf())
    val warnings: MutableList<String> = Collections.synchronizedList(mutableListOf())

    override fun confirmAction(description: String, confirmation: Confirmation): Boolean {
        confirmations += description to confirmation
        return confirmAnswer
    }

    override fun confirmSetup(kind: String): Boolean {
        setups += kind
        return setupAnswer
    }

    override fun pick(kind: PickKind, options: PickOptions): String? {
        picks += kind to options
        return pickResult
    }

    override fun copyText(text: String) {
        copied += text
    }

    override fun openFolder(dir: String) {
        folders += dir
    }

    override fun openCommand(program: String, args: List<String>) {
        commands += listOf(program) + args
    }

    override fun warn(text: String) {
        warnings += text
    }
}
