package com.swapdeck.jetbrains.actions

import com.intellij.ide.actions.RevealFileAction
import com.intellij.openapi.actionSystem.ActionUpdateThread
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.application.ModalityState
import com.intellij.openapi.diagnostic.logger
import com.intellij.openapi.ide.CopyPasteManager
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.ProgressManager
import com.intellij.openapi.progress.Task
import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.Project
import com.intellij.openapi.ui.Messages
import com.swapdeck.jetbrains.IdeText
import com.swapdeck.jetbrains.SwapDeckBundle
import com.swapdeck.jetbrains.SwapDeckThreads
import com.swapdeck.jetbrains.bridge.ActionFlow
import com.swapdeck.jetbrains.bridge.IdeBridgeHost
import com.swapdeck.jetbrains.bridge.IdeBridgeUi
import com.swapdeck.jetbrains.bridge.textOrNull
import com.swapdeck.jetbrains.cli.CliException
import com.swapdeck.jetbrains.cli.CliOptions
import com.swapdeck.jetbrains.cli.SwapDeckCli
import com.swapdeck.jetbrains.cli.runBlocking
import com.swapdeck.jetbrains.cli.unwrap
import com.swapdeck.jetbrains.engine.EngineService
import com.swapdeck.jetbrains.settings.SwapDeckSettings
import com.swapdeck.jetbrains.state.StateService
import com.swapdeck.jetbrains.ui.MapMenu
import com.swapdeck.jetbrains.ui.SwapDeckNotifications
import com.swapdeck.jetbrains.ui.SwitchMenu
import com.swapdeck.jetbrains.ui.SwitchTarget
import com.swapdeck.jetbrains.ui.showMenuPopup
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import java.awt.datatransfer.StringSelection
import java.nio.file.Path
import java.util.concurrent.RejectedExecutionException

/**
 * Azioni «SwapDeck» (menu Strumenti e Trova azione), porting dei comandi di VS Code. Funzionano anche
 * senza JCEF: tutto passa da `swapdeck-cli` sui thread del plugin, i dialoghi sull'EDT.
 */
abstract class SwapDeckAction : AnAction(), DumbAware {
    override fun getActionUpdateThread(): ActionUpdateThread = ActionUpdateThread.BGT

    final override fun actionPerformed(e: AnActionEvent) = perform(e.project)

    protected abstract fun perform(project: Project?)
}

/** Aiuti comuni: lavori sui thread del plugin, EDT, errori come notifiche. */
internal object ActionSupport {
    private val LOG = logger<SwapDeckAction>()

    val cli: SwapDeckCli get() = SwapDeckCli.getInstance()

    fun t(key: String, vararg params: Any): String = SwapDeckBundle.message(key, *params)

    /** [block] su un thread del plugin; un errore diventa una notifica ([failKey] col messaggio). */
    fun background(project: Project?, failKey: String?, block: () -> Unit) {
        try {
            SwapDeckThreads.io.execute {
                try {
                    block()
                } catch (e: Throwable) {
                    fail(project, failKey, e)
                }
            }
        } catch (_: RejectedExecutionException) {
            // Plugin in chiusura.
        }
    }

    /**
     * Notifica d'errore: il messaggio della CLI; per un errore inatteso un testo generico (il
     * dettaglio, che può avere percorsi locali, va nel log dell'IDE).
     */
    fun fail(project: Project?, failKey: String?, error: Throwable) {
        val cause = error.unwrap()
        val message = if (cause is CliException) {
            cause.error.message
        } else {
            LOG.warn("azione non riuscita", cause)
            t("action.internalError")
        }
        SwapDeckNotifications.error(project, if (failKey != null) t(failKey, message) else "SwapDeck: $message")
    }

    fun later(block: () -> Unit) = ApplicationManager.getApplication().invokeLater(block, ModalityState.nonModal())

    /** [block] in background con l'indicatore d'avanzamento dell'IDE. */
    fun withProgress(project: Project?, title: String, failKey: String?, block: () -> Unit) = later {
        ProgressManager.getInstance().run(
            object : Task.Backgroundable(project, title, false) {
                override fun run(indicator: ProgressIndicator) {
                    indicator.isIndeterminate = true
                    block()
                }

                override fun onThrowable(error: Throwable) = fail(project, failKey, error)
            },
        )
    }

    fun state(): StateService = StateService.getInstance().also { it.start() }
}

/** «Aggiorna»: nuova lettura dello stato (barra, pannelli, notifiche). */
class RefreshAction : SwapDeckAction() {
    override fun perform(project: Project?) = ActionSupport.background(project, null) {
        ActionSupport.state().refresh()
    }
}

/**
 * «Cambia account»: popup con «Account migliore» e «Prossimo disponibile» (solo con almeno due
 * account), poi gli account con 5h e 7d; attivo e disattivati non si scelgono; senza account un
 * messaggio. Conferma dovuta decisa dal Rust (`confirm -`) con il dialogo del ponte, poi `action -`
 * con l'indicatore d'avanzamento e `StateService.afterAction`.
 */
class SwitchAccountAction : SwapDeckAction() {
    override fun perform(project: Project?) = ActionSupport.background(project, "switch.failed") {
        val service = ActionSupport.state()
        val state = service.state?.takeIf { it.list != null } ?: service.refresh().get()
        val t = SwapDeckBundle.translate()
        when (val menu = SwitchMenu.of(state.list, t)) {
            SwitchMenu.NoAccounts -> SwapDeckNotifications.info(project, t.message("switch.noAccounts"))
            is SwitchMenu.Items -> ActionSupport.later {
                showMenuPopup(project, t.message("switch.title"), menu.items) { target ->
                    ActionSupport.background(project, "switch.failed") { switchTo(project, target) }
                }
            }
        }
    }

    private fun switchTo(project: Project?, target: SwitchTarget) {
        val action = target.action()
        val flow = ActionFlow(ActionSupport.cli)
        val ui = IdeBridgeUi(project)
        val needed = flow.required(action, StateService.getInstance().state?.list)
        if (needed != null) {
            val label = when (target) {
                is SwitchTarget.Account -> ActionSupport.t("switch.confirm.account", target.number)
                is SwitchTarget.Strategy -> ActionSupport.t("switch.confirm.strategy", target.name)
            }
            if (!ui.confirm(label, needed)) return
        }
        ActionSupport.withProgress(project, ActionSupport.t("switch.progress"), "switch.failed") {
            val outcome = flow.run(action) as? JsonObject ?: return@withProgress
            ActionFlow.showOutcome(outcome, ui)
            // Il cambio è fatto: un errore nell'aggiornare lo stato non lo rende fallito.
            runCatching { StateService.getInstance().afterAction(outcome).get() }
        }
    }
}

/** «Cambia mappa»: popup delle mappe (personali, poi i preset), con quella attiva spuntata; poi `select`. */
class SwitchMapAction : SwapDeckAction() {
    override fun perform(project: Project?) = ActionSupport.background(project, "map.failed") {
        val profiles = ActionSupport.cli.runBlocking(listOf("engine", "profiles")) as? JsonObject
            ?: throw CliException("failed", "engine profiles: risposta non valida")
        val t = SwapDeckBundle.translate()
        val activeId = profiles.textOrNull("activeId")
        val items = MapMenu.of(profiles, t)
        ActionSupport.later {
            showMenuPopup(project, t.message("map.title"), items) { id ->
                if (id != activeId) ActionSupport.background(project, "map.failed") { select(project, profiles, id) }
            }
        }
    }

    private fun select(project: Project?, profiles: JsonObject, id: String) {
        val command = buildJsonObject {
            put("op", JsonPrimitive("select"))
            put("id", JsonPrimitive(id))
        }
        ActionSupport.cli.runBlocking(listOf("engine", "profiles", "-"), CliOptions(stdin = command.toString()))
        SwapDeckNotifications.info(project, ActionSupport.t("map.done", MapMenu.nameOf(profiles, id)))
        ActionSupport.state().refresh()
    }
}

/**
 * «Avvia motore» e «Ferma motore» ([EngineToggle]); avviando accende anche il processo del motore
 * (`engineEnabled`) se era spento.
 */
abstract class EngineAction(private val enable: Boolean) : SwapDeckAction() {
    override fun perform(project: Project?) = ActionSupport.background(project, "engine.failed") {
        val toggle = EngineToggle(ActionSupport.cli)
        val result = if (enable) toggle.enable { IdeBridgeUi.onEdt { askFirstEnable(project) } } else toggle.disable()
        if (result !is EngineToggleResult.Changed) return@background
        val settings = SwapDeckSettings.getInstance()
        if (enable && !settings.snapshot.engineEnabled) settings.update { it.copy(engineEnabled = true) }
        EngineService.getInstance().sync()
        val key = when {
            !result.enabled -> "engine.off"
            result.dryRun -> "engine.on.observe"
            else -> "engine.on"
        }
        SwapDeckNotifications.info(project, ActionSupport.t(key))
        ActionSupport.state().refresh()
    }

    /** Dialogo modale della prima accensione: Accendi / Solo osservazione / Annulla (predefinito). */
    private fun askFirstEnable(project: Project?): FirstEnableAnswer? {
        val choice = Messages.showDialog(
            project?.takeUnless { it.isDisposed },
            IdeText.dialog(ActionSupport.t("engine.firstEnable")),
            "SwapDeck",
            null,
            arrayOf(ActionSupport.t("engine.firstEnable.on"), ActionSupport.t("engine.firstEnable.observe"), Messages.getCancelButton()),
            2,
            2,
            Messages.getWarningIcon(),
        )
        return EngineToggle.answerOf(choice)
    }
}

class StartEngineAction : EngineAction(true)

class StopEngineAction : EngineAction(false)

/** «Copia diagnostica»: `diagnostics --json --client …` negli appunti. */
class CopyDiagnosticsAction : SwapDeckAction() {
    override fun perform(project: Project?) = ActionSupport.background(project, null) {
        val result = ActionSupport.cli.runBlocking(listOf("diagnostics", "--json", "--client", IdeBridgeHost.CLIENT_INFO)) as? JsonObject
        val text = result?.textOrNull("text") ?: throw CliException("failed", "diagnostics: risposta non valida")
        IdeBridgeUi.onEdt { CopyPasteManager.getInstance().setContents(StringSelection(text)) }
        SwapDeckNotifications.info(project, ActionSupport.t("diagnostics.copied"))
    }
}

/** «Apri cartella dei log»: `logs dir` nel file manager del sistema. */
class OpenLogsFolderAction : SwapDeckAction() {
    override fun perform(project: Project?) = ActionSupport.background(project, null) {
        val dir = (ActionSupport.cli.runBlocking(listOf("logs", "dir")) as? JsonObject)?.textOrNull("dir")
            ?: throw CliException("failed", "logs dir: risposta non valida")
        RevealFileAction.openDirectory(Path.of(dir))
    }
}
