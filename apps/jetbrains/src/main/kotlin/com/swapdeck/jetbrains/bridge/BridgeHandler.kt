package com.swapdeck.jetbrains.bridge

import com.swapdeck.jetbrains.SwapDeckThreads
import com.swapdeck.jetbrains.cli.CliCaller
import com.swapdeck.jetbrains.cli.CliException
import com.swapdeck.jetbrains.cli.CliOptions
import com.swapdeck.jetbrains.cli.CliTimeouts
import com.swapdeck.jetbrains.cli.CswapError
import com.swapdeck.jetbrains.cli.StdinChannel
import com.swapdeck.jetbrains.cli.unwrap
import com.swapdeck.jetbrains.engine.EngineResume
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import java.util.concurrent.CancellationException
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ExecutionException
import java.util.concurrent.Executor
import java.util.concurrent.Future

/** Una richiesta in corso: parametri, avanzamento e pannello da cui arriva. */
private class Call(
    val params: JsonObject,
    val progress: (JsonObject) -> Unit,
    val session: BridgeSession,
)

/**
 * Richieste del pannello tradotte in comandi di `swapdeck-cli` (porting di `bridge.ts` di VS Code).
 * Uno per processo IDE ([BridgeService]); ogni pannello apre la sua [BridgeSession].
 *
 * Metodi, `kind` ammessi e limiti vengono da `protocol.json` ([BridgeRules]). Ogni richiesta gira su
 * un thread del plugin ([executor]) e si blocca sui comandi e sui dialoghi; l'esito è il `result`
 * della risposta, l'errore un [CliException] (`invalidUsage` per i parametri non validi, senza CLI).
 */
class BridgeHandler(
    private val cli: CliCaller,
    private val host: BridgeHost,
    private val rules: BridgeRules = BridgeRules.DEFAULT,
    private val executor: Executor = SwapDeckThreads.io,
    private val clock: () -> Long = System::currentTimeMillis,
) {
    private val check = BridgeValidation(rules)

    /** «Rifai login» in corso (uno solo per processo IDE): pannello e stdin della CLI. */
    private class Relogin(val owner: BridgeSession) {
        @Volatile
        var channel: StdinChannel? = null

        /** Pannello chiuso: annulla appena c'è stdin. */
        @Volatile
        var abandoned = false
    }

    private val reloginLock = Any()
    private var relogin: Relogin? = null

    private val handlers: Map<String, (Call) -> JsonElement> = mapOf(
        "listAccounts" to ::listAccounts,
        "getStatus" to { _ -> cli(listOf("status")) },
        "getConfig" to { _ -> cli(listOf("cswap-config")) },
        "getConfigValue" to { c ->
            val key = check.matching(c.params["key"], rules.configKey, "chiave non valida")
            cli(listOf("cswap-config", key))
        },
        "getHistory" to ::history,
        "confirmationFor" to { c -> confirmationFor(check.action(c.params["action"])) },
        "runAction" to ::runAction,
        "listMappings" to { _ -> cli(listOf("mappings")) },
        "getEngineState" to { _ -> cli(listOf("engine", "state")) },
        "setEngineRules" to { c -> withStdin(listOf("engine", "rules", "set", "-"), check.plainObject(c.params["rules"], "regole")) },
        "getEngineProfiles" to { _ -> cli(listOf("engine", "profiles")) },
        "applyEngineProfile" to { c -> withStdin(listOf("engine", "profiles", "-"), check.plainObject(c.params["command"], "mappa")) },
        "getNotificationSettings" to { _ -> cli(listOf("notifications", "get")) },
        "setNotificationSettings" to { c ->
            withStdin(listOf("notifications", "set", "-"), check.plainObject(c.params["patch"], "notifiche"))
        },
        "getPreferences" to { _ -> cli(listOf("preferences", "get")) },
        "setPreferences" to { c ->
            val prefs = check.plainObject(c.params["prefs"], "preferenze")
            withStdin(listOf("preferences", "set", "-"), prefs).also { host.preferencesChanged() }
        },
        "getHistoryEvents" to ::historyEvents,
        "getUsageBreakdown" to ::usageBreakdown,
        "pickOpenFile" to { c -> pick(PickKind.OPEN_FILE, c) },
        "pickSaveFile" to { c -> pick(PickKind.SAVE_FILE, c) },
        "pickFolder" to { c -> pick(PickKind.FOLDER, c) },
        "diagnoseSetup" to { _ -> cli(listOf("setup", "diagnose")) },
        "installCswap" to { c -> setupRun("install", c) },
        "upgradeCswap" to { c -> setupRun("upgrade", c) },
        "checkCswapUpdate" to { c ->
            val force = (c.params["force"] as? JsonPrimitive)?.takeIf { !it.isString }?.booleanOrNull == true
            cli(if (force) listOf("setup", "check-update", "--force") else listOf("setup", "check-update"))
        },
        "dismissCswapUpdate" to { c ->
            val version = check.matching(c.params["version"], rules.version, "versione non valida")
            cli(listOf("setup", "dismiss-update", version))
            JsonNull
        },
        "getDiagnostics" to ::diagnostics,
        "openLogsFolder" to { c ->
            val dir = (cli(listOf("logs", "dir")) as? JsonObject)?.textOrNull("dir")
                ?: throw CliException("failed", "logs dir: risposta non valida")
            c.session.ui.openFolder(dir)
            JsonNull
        },
        "copyText" to { c ->
            val text = c.params["text"].textOrNull()
            if (text == null || text.length > rules.copyTextMaxLength) throw invalid("testo non valido")
            c.session.ui.copyText(text)
            JsonNull
        },
        "reloginAccounts" to ::reloginAccounts,
        "startRelogin" to ::startRelogin,
        "submitReloginCode" to { c ->
            val code = c.params["code"].textOrNull()
            // Una riga, lunghezza limitata; il valore va solo su stdin della CLI, mai nei log.
            if (code == null || code.length > rules.reloginCodeMaxLength || code.contains('\r') || code.contains('\n')) {
                throw invalid("codice non valido")
            }
            reloginWrite(c.session, buildJsonObject { put("code", JsonPrimitive(code)) })
        },
        "cancelRelogin" to { c -> reloginWrite(c.session, CANCEL) },
    )

    /** Metodi che il ponte sa gestire (un test controlla che siano quelli di `protocol.json`). */
    val handledMethods: Set<String> get() = handlers.keys

    /** Nuovo pannello: chiuderlo annulla le sue richieste lunghe. */
    fun openSession(ui: BridgeUi): BridgeSession = BridgeSession(this, ui)

    /**
     * Gestisce una richiesta del pannello [session]: metodo fuori elenco o parametri non validi
     * = `invalidUsage` senza chiamare la CLI. [progress] riceve gli eventi delle richieste lunghe.
     */
    fun handle(
        method: JsonElement?,
        params: JsonElement?,
        session: BridgeSession,
        progress: (JsonObject) -> Unit = {},
    ): CompletableFuture<JsonElement> {
        val name = method.textOrNull()
        val handler = name?.takeIf { it in rules.methods }?.let { handlers[it] }
            ?: return CompletableFuture.failedFuture(invalid("metodo sconosciuto"))
        val call = Call(params as? JsonObject ?: JsonObject(emptyMap()), progress, session)
        return CompletableFuture.supplyAsync({ handler(call) }, executor)
    }

    // --- comandi -------------------------------------------------------------------------------

    private fun cli(args: List<String>, options: CliOptions = CliOptions()): JsonElement = await(cli.run(args, options))

    private fun withStdin(args: List<String>, value: JsonElement): JsonElement = cli(args, CliOptions(stdin = value.toString()))

    /** Attesa di un future: errore originale, annullamento = `cancelled`. */
    private fun <T> await(future: Future<T>): T = try {
        future.get()
    } catch (e: CancellationException) {
        throw cancelled()
    } catch (e: ExecutionException) {
        throw e.unwrap()
    }

    private fun listAccounts(c: Call): JsonElement {
        val refresh = (c.params["refresh"] as? JsonPrimitive)?.takeIf { !it.isString }?.booleanOrNull == true
        val last = host.lastState()
        if (!refresh && last?.list != null) return last.list
        val state = await(host.refresh())
        return state.list ?: throw CliException(state.error ?: CswapError.of("failed", "lista non disponibile"))
    }

    private fun history(c: Call): JsonElement {
        val account = check.optionalInt(c.params["account"], "account")
        val nowSec = Math.floorDiv(clock(), 1000L)
        val since = check.optionalInt(c.params["sinceSeconds"], "sinceSeconds")
        val args = mutableListOf("history", "--from", (since ?: (nowSec - rules.historySeconds)).toString())
        args += listOf("--to", nowSec.toString())
        if (account != null) args += listOf("--account", account.toString())
        // Punti dei grafici già pronti (e sfoltiti) dalla CLI, come nell'app.
        args += "--points"
        return cli(args)
    }

    private fun historyEvents(c: Call): JsonElement {
        val args = mutableListOf("history", "events")
        val since = check.optionalInt(c.params["sinceSeconds"], "sinceSeconds")
        val account = check.optionalInt(c.params["account"], "account")
        if (since != null) args += listOf("--since", since.toString())
        if (account != null) args += listOf("--account", account.toString())
        return cli(args)
    }

    /** Lettura a budget: ogni chiamata dura pochi secondi, la UI richiama finché c'è `partial`. */
    private fun usageBreakdown(c: Call): JsonElement {
        val query = check.plainObject(c.params["query"], "richiesta dei consumi")
        val window = check.oneOf(query["window"], rules.usageWindows, "finestra non valida")
        val groupBy = check.oneOf(query["groupBy"], rules.usageGroups, "raggruppamento non valido")
        val account = check.optionalInt(query["account"], "account")
        val args = mutableListOf("usage", "breakdown", "--window", window, "--group-by", groupBy)
        if (account != null) args += listOf("--account", account.toString())
        if (window == "custom") {
            val from = check.optionalInt(query["fromSeconds"], "fromSeconds")
            val to = check.optionalInt(query["toSeconds"], "toSeconds")
            if (from == null) throw invalid("fromSeconds richiesto")
            args += listOf("--from", from.toString())
            if (to != null) args += listOf("--to", to.toString())
        }
        return cli(args, CliOptions(timeoutMs = CliTimeouts.USAGE_MS))
    }

    private fun diagnostics(@Suppress("UNUSED_PARAMETER") c: Call): JsonElement {
        val args = mutableListOf("diagnostics", "--json")
        host.clientInfo?.let { args += listOf("--client", it) }
        return (cli(args) as? JsonObject)?.get("text") ?: JsonNull
    }

    private fun pick(kind: PickKind, c: Call): JsonElement {
        val options = check.pick(c.params)
        return c.session.ui.pick(kind, options)?.let(::JsonPrimitive) ?: JsonNull
    }

    // --- azioni --------------------------------------------------------------------------------

    /** Conferma dovuta (regola del Rust), con l'account attivo se lo stato è noto. */
    private fun confirmationFor(action: JsonObject): JsonElement {
        val list = host.lastState()?.list
        val args = mutableListOf("confirm", "-")
        if (list != null) {
            val active = list["activeAccountNumber"]
            args += listOf("--active", if (active is JsonPrimitive && active !is JsonNull) active.content else "none")
        }
        return cli(args, CliOptions(stdin = action.toString()))
    }

    private fun runAction(c: Call): JsonElement {
        val action = check.action(c.params["action"])
        // La conferma la impone l'host: il pannello non è fidato.
        val needed = confirmationOf(confirmationFor(action))
        if (needed != null) {
            if (!c.session.ui.confirmAction(describeAction(action), needed)) throw cancelled("azione annullata")
        }
        val result = cli(listOf("action", "-"), CliOptions(stdin = action.toString(), timeoutMs = CliTimeouts.ACTION_MS))
        val outcome = result as? JsonObject ?: return result
        showOutcome(outcome, c.session.ui)
        // L'azione è fatta: un errore nell'aggiornare lo stato non la rende fallita.
        runCatching { await(host.afterAction(outcome)) }
        return outcome
    }

    /** `null` = nessuna conferma; `kind` sconosciuto o senza parola = conferma con parola impossibile. */
    private fun confirmationOf(value: JsonElement): Confirmation? {
        val obj = value as? JsonObject ?: throw CliException("failed", "confirm: risposta non valida")
        return when (obj.textOrNull("kind")) {
            "none" -> null
            "simple" -> Confirmation.Simple
            else -> Confirmation.Typed(obj.textOrNull("word") ?: throw CliException("failed", "confirm: risposta non valida"))
        }
    }

    /** Terminale per `run`, avvisi dell'esito e cambio di account non riuscito. */
    private fun showOutcome(outcome: JsonObject, ui: BridgeUi) {
        (outcome["command"] as? JsonObject)?.let { command ->
            val program = command.textOrNull("program")
            val args = (command["args"] as? JsonArray)?.mapNotNull { it.textOrNull() } ?: emptyList()
            if (program != null) ui.openCommand(program, args)
        }
        outcome.textOrNull("warning")?.let { ui.warn("SwapDeck: $it") }
        (outcome["switch"] as? JsonObject)?.let { sw ->
            val switched = (sw["switched"] as? JsonPrimitive)?.takeIf { !it.isString }?.booleanOrNull
            if (switched == false) ui.warn("SwapDeck: ${sw.textOrNull("message") ?: ""}")
        }
    }

    // --- setup di cswap ------------------------------------------------------------------------

    /** Installazione o aggiornamento: conferma dell'host, eventi in streaming, poi nuova lettura. */
    private fun setupRun(kind: String, c: Call): JsonElement {
        if (!c.session.ui.confirmSetup(kind)) throw cancelled()
        if (c.session.scope.aborted) throw cancelled()
        val resume: EngineResume? = if (kind == "upgrade") await(host.pauseEngine("upgrade")) else null
        try {
            val future = cli.run(listOf("setup", kind), CliOptions(timeoutMs = CliTimeouts.SETUP_MS, onEvent = c.progress))
            // Pannello chiuso: il comando si interrompe (il processo termina).
            c.session.scope.onAbort { future.cancel(true) }.use { return await(future) }
        } finally {
            resume?.resume()
            afterChange()
        }
    }

    private fun afterChange() {
        host.changed()
        host.refresh().exceptionally { null }
    }

    // --- «Rifai login» -------------------------------------------------------------------------

    private fun reloginAccounts(c: Call): JsonElement {
        val list = c.params["list"]
        if (list !is JsonObject && list !is JsonArray) throw invalid("lista non valida")
        val result = cli(listOf("relogin", "candidates", "-"), CliOptions(stdin = list.toString())) as? JsonObject
        val accounts = result?.get("accounts") as? JsonArray ?: return JsonArray(emptyList())
        return JsonArray(
            accounts.filter { it is JsonPrimitive && BridgeValidation.safeInteger(it) != null },
        )
    }

    /**
     * Motore di questo processo fermo per tutta l'operazione, eventi in streaming. Chiudere il
     * pannello = «Annulla» (anche se succede prima che la CLI parta).
     */
    private fun startRelogin(c: Call): JsonElement {
        val account = check.optionalInt(c.params["account"], "account")
        if (account == null || account < 1) throw invalid("account non valido")
        val run = Relogin(c.session)
        synchronized(reloginLock) {
            if (relogin != null) throw CliException("busy", "un «Rifai login» è già in corso")
            if (c.session.scope.aborted) throw cancelled()
            relogin = run
        }
        val cancelLine = "$CANCEL\n"
        val registration = c.session.scope.onAbort {
            run.abandoned = true
            run.channel?.write(cancelLine)
        }
        var resume: EngineResume? = null
        try {
            resume = await(host.pauseEngine("relogin"))
            return cli(
                listOf("relogin", "start", account.toString()),
                CliOptions(
                    timeoutMs = CliTimeouts.RELOGIN_MS,
                    onEvent = c.progress,
                    channel = { stdin ->
                        run.channel = stdin
                        // Chiuso prima che la CLI partisse: annulla subito.
                        if (run.abandoned) stdin.write(cancelLine)
                    },
                ),
            )
        } finally {
            registration.close()
            run.channel = null
            synchronized(reloginLock) { if (relogin === run) relogin = null }
            resume?.resume()
            afterChange()
        }
    }

    /** Riga per stdin di `relogin start` del pannello [session]; errore se non ce n'è uno in corso. */
    private fun reloginWrite(session: BridgeSession, line: JsonObject): JsonElement {
        val run = synchronized(reloginLock) { relogin }
        val channel = run?.takeIf { it.owner === session }?.channel
        if (channel == null || !channel.write("$line\n")) throw invalid("nessun «Rifai login» in corso")
        return JsonNull
    }

    private companion object {
        val CANCEL = buildJsonObject { put("cancel", JsonPrimitive(true)) }
    }
}
