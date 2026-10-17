/**
 * Estensione SwapDeck: vista nella barra delle attività con l'app completa (anche in una scheda),
 * barra di stato, cambio rapido, notifiche, motore e sincronia fra le finestre di VS Code.
 */
import type {
  ActionOutcome,
  Confirmation,
  CswapAction,
  EngineProfiles,
  EngineState,
  ListResponse,
  Notification,
  NotifyPoll,
  ProfileCommand,
  SwitchStrategy,
} from '@swapdeck/types';
import { displayUsage } from '@swapdeck/ui/account';
import { displayPct, shortEmail } from '@swapdeck/ui/format';
import { mkdir } from 'node:fs/promises';
import { dirname } from 'node:path';
import * as vscode from 'vscode';
import { createRequestHandler, describeAction } from './bridge.js';
import {
  ACTION_TIMEOUT_MS,
  CliError,
  bundledCliPath,
  createCliRunner,
  ensureExecutable,
  resolveCliPath,
  toCswapError,
} from './cli.js';
import { DEACTIVATE_GRACE_MS, EngineProcess, shouldRetryEngine } from './engine.js';
import type { CliState } from './generated/index.js';
import { fromPoll, langOf, messageOf, parseEngineEvent, shouldPoll } from './notifications.js';
import { DashboardPanel, SIDEBAR_VIEW_ID, SidebarView, WebviewHub } from './panel.js';
import type { PickParams } from './protocol.js';
import { Refresher } from './refresher.js';
import { SyncWatcher } from './sync.js';
import {
  accountLabel,
  colorThresholds,
  engineView,
  statusModel,
  type Translate,
} from './statusBar.js';

const SECTION = 'swapdeck';
const MIN_INTERVAL_SECONDS = 15;
/** Al ritorno del focus sulla finestra si rilegge solo se l'ultima lettura è più vecchia. */
const FOCUS_REFRESH_AFTER_MS = 10_000;

/** API restituita da `activate`, usata solo dai test dell'estensione. */
export interface SwapDeckApi {
  refresh(): Promise<CliState>;
  state(): CliState | undefined;
  statusText(): string;
  switchTo(target: number | SwitchStrategy): Promise<ActionOutcome | undefined>;
  engineRunning(): boolean;
  panelOpen(): boolean;
  /** Un webview (vista o scheda) ha caricato lo script (CSP e nonce corretti) e chiesto lo stato. */
  panelReady(): boolean;
  /** La vista della barra laterale è stata creata da VS Code. */
  viewResolved(): boolean;
  /** Comando del clic sulla barra di stato. */
  statusCommand(): string | undefined;
  /** La finestra osserva la cartella della config (sincronia con le altre finestre). */
  syncWatching(): boolean;
  /** Avvisa le altre finestre come dopo un'azione (test della sincronia). */
  signalSync(): Promise<void>;
  cliPath(): string;
}

const t: Translate = (message, ...args) => vscode.l10n.t(message, ...args);

function settings() {
  const c = vscode.workspace.getConfiguration(SECTION);
  const interval = Math.max(MIN_INTERVAL_SECONDS, c.get<number>('refreshIntervalSeconds', 60));
  return {
    intervalMs: interval * 1000,
    cliPath: c.get<string>('cliPath', ''),
    engineEnabled: c.get<boolean>('engine.enabled', true),
    thresholds: c.get<number[]>('notifications.thresholds', [80, 95]),
  };
}

export async function activate(context: vscode.ExtensionContext): Promise<SwapDeckApi> {
  const output = vscode.window.createOutputChannel('SwapDeck', { log: true });
  const platform = process.platform;
  const cliPath = () => resolveCliPath(settings().cliPath, context.extensionPath, platform);
  const bundled = bundledCliPath(context.extensionPath, platform);
  await ensureExecutable(bundled, platform).catch(() => undefined);
  const cli = createCliRunner(cliPath);

  let state: CliState | undefined;
  let timer: NodeJS.Timeout | undefined;
  let engineWarned = false;
  /** Il lock è di un'altra finestra o dell'app desktop: si ritenta quando si libera. */
  let waitingLock = false;
  /** Osserva la cartella della config; nasce dopo `config get` (percorso deciso dalla CLI). */
  let sync: SyncWatcher | undefined;
  let syncStarting = false;
  const signalSync = () => sync?.signal() ?? Promise.resolve();

  const status = vscode.window.createStatusBarItem(
    'swapdeck.status',
    vscode.StatusBarAlignment.Left,
    100,
  );
  status.name = 'SwapDeck';
  // Un clic apre la vista (mai un cambio di account: lo switch è globale per l'utente).
  status.command = 'swapdeck.showView';

  const engine = new EngineProcess({
    onLine: (line) => {
      const note = parseEngineEvent(line);
      if (note) {
        // Solo il titolare del lock manda eventi: una notifica per evento, non una per finestra.
        show(note);
        void signalSync();
      } else output.info(`[motore] ${line}`);
    },
    onUnexpectedExit: (code) => {
      output.warn(`[motore] uscito con codice ${code ?? 'segnale'}`);
      if (!engineWarned) {
        engineWarned = true;
        void vscode.window.showWarningMessage(
          t(
            'SwapDeck engine stopped unexpectedly (exit code {0}). See the SwapDeck output.',
            code ?? '-',
          ),
        );
      }
      render();
    },
    onLockBusy: () => {
      waitingLock = true;
      output.info('[motore] attivo altrove (lock occupato): riprovo quando si libera');
      render();
    },
  });

  const render = () => {
    const s = settings();
    const model = statusModel(
      state,
      colorThresholds(s.thresholds),
      engineView(state, engine.pid),
      t,
      (ms) => new Date(ms).toLocaleTimeString(),
    );
    status.text = model.text;
    status.tooltip = model.tooltip;
    status.backgroundColor =
      model.level === 'danger' || model.level === 'error'
        ? new vscode.ThemeColor('statusBarItem.errorBackground')
        : model.level === 'warn'
          ? new vscode.ThemeColor('statusBarItem.warningBackground')
          : undefined;
    status.show();
  };

  function show(note: Notification): void {
    const { level, text } = messageOf(note, langOf(vscode.env.language));
    void (level === 'error'
      ? vscode.window.showErrorMessage(text)
      : level === 'warning'
        ? vscode.window.showWarningMessage(text)
        : vscode.window.showInformationMessage(text));
  }

  /** Mittente unico: senza motore vivo (né il nostro né altrove) le notifiche le chiede la CLI. */
  async function pollNotifications(list: ListResponse): Promise<void> {
    const elsewhere = state?.engine.problem ? undefined : state?.engine.running;
    if (!shouldPoll(engine.running, elsewhere)) return;
    try {
      const poll = await cli<NotifyPoll>(['notifications', 'poll', '-'], {
        stdin: JSON.stringify(list),
      });
      for (const note of fromPoll(poll)) show(note);
    } catch (error) {
      output.warn(`notifications: ${toCswapError(error).message}`);
    }
  }

  /** Selettori nativi di VS Code per i percorsi di export, import e cartelle mappate. */
  async function pickPath(
    kind: 'openFile' | 'saveFile' | 'folder',
    options: PickParams,
  ): Promise<string | null> {
    const common = {
      ...(options.title ? { title: options.title } : {}),
      ...(options.defaultPath ? { defaultUri: vscode.Uri.file(options.defaultPath) } : {}),
      ...(options.filters
        ? { filters: Object.fromEntries(options.filters.map((f) => [f.name, f.extensions])) }
        : {}),
    };
    if (kind === 'saveFile') return (await vscode.window.showSaveDialog(common))?.fsPath ?? null;
    const picked = await vscode.window.showOpenDialog({
      ...common,
      canSelectFiles: kind === 'openFile',
      canSelectFolders: kind === 'folder',
      canSelectMany: false,
    });
    return picked?.[0]?.fsPath ?? null;
  }

  const hub = new WebviewHub(
    context.extensionUri,
    createRequestHandler({
      cli,
      lastState: () => state,
      refresh: () => refresh(),
      onOutcome: (outcome) => afterAction(outcome),
      confirm: (action, needed) =>
        askConfirmation(needed, t('SwapDeck: confirm "{0}"?', describeAction(action))),
      pick: pickPath,
      confirmSetup: async (kind) => {
        const ok = t('Confirm');
        const label =
          kind === 'install'
            ? t('SwapDeck: install uv (if missing) and claude-swap for the current user?')
            : t('SwapDeck: upgrade claude-swap with "uv tool upgrade claude-swap"?');
        return (await vscode.window.showWarningMessage(label, { modal: true }, ok)) === ok;
      },
      pauseEngine: async () => {
        if (!engine.running) return () => undefined;
        output.info('[motore] arresto per l’aggiornamento di cswap');
        await engine.stop();
        return () => syncEngine();
      },
      onChanged: () => {
        void signalSync();
        configChanged();
      },
    }),
    (post) => {
      if (state?.list) post({ type: 'state', list: state.list });
    },
  );
  /** Le schede aperte nei webview di questa finestra rileggono i loro dati. */
  const configChanged = () => hub.post({ type: 'configChanged' });
  const panel = new DashboardPanel(hub);
  const sidebar = new SidebarView(hub);

  /** Cartella della config dalla CLI (anche con `SWAPDECK_HOME`), poi osservazione. */
  async function startSync(): Promise<void> {
    if (sync?.watching || syncStarting) return;
    syncStarting = true;
    try {
      if (!sync) {
        const view = await cli<{ path: string }>(['config', 'get']);
        const dir = dirname(view.path);
        // Su Linux la cartella della config nasce al primo salvataggio: si crea privata (0700),
        // così anche il segnale dei cambi di account funziona da subito.
        await mkdir(dir, { recursive: true, mode: 0o700 });
        sync = new SyncWatcher({
          dir,
          onChange: () => {
            void refresh();
            configChanged();
          },
          log: (message) => output.info(message),
        });
      }
      if (sync.start()) output.info('sync: osservo la cartella della config di SwapDeck');
    } catch (error) {
      output.warn(`sync: ${toCswapError(error).message}`);
    } finally {
      syncStarting = false;
    }
  }

  /** Lettura di `state`: non rifiuta, un errore della CLI diventa uno stato con `error`. */
  async function readState(): Promise<CliState> {
    try {
      return await cli<CliState>(['state']);
    } catch (error) {
      const info = toCswapError(error);
      output.error(`state: ${info.message}`);
      return {
        list: null,
        error: info,
        updatedAtMs: Date.now(),
        engine: state?.engine ?? {
          enabled: false,
          dryRun: false,
          pollIntervalSeconds: 60,
          running: false,
          lock: null,
        },
      };
    }
  }

  const refresher = new Refresher(readState, (next, current) => {
    if (!current) return state ?? next; // partita prima di un'azione: lo stato nuovo è già lì
    state = next;
    if (next.error) output.warn(`state: ${next.error.kind}: ${next.error.message}`);
    if (next.list) {
      void pollNotifications(next.list);
      hub.post({ type: 'state', list: next.list });
    }
    // La cartella della config può nascere dopo (primo salvataggio): si ritenta a ogni lettura.
    if (!sync?.watching) void startSync();
    if (
      shouldRetryEngine({
        enabled: settings().engineEnabled,
        localRunning: engine.running,
        waitingLock,
        lockHeldElsewhere: next.engine.problem ? undefined : next.engine.running,
      })
    ) {
      output.info('[motore] lock libero: nuovo avvio');
      syncEngine();
    }
    render();
    return next;
  });
  const refresh = () => refresher.refresh();

  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      void refresh().finally(schedule);
    }, settings().intervalMs);
  };

  async function afterAction(outcome: ActionOutcome): Promise<void> {
    refresher.invalidate();
    // Cambio di account o altra azione di cswap: le altre finestre rileggono subito, e così le
    // schede aperte nelle altre viste di questa finestra.
    void signalSync();
    configChanged();
    if (outcome.command) {
      const term = vscode.window.createTerminal({
        name: 'cswap run',
        shellPath: outcome.command.program,
        shellArgs: outcome.command.args,
      });
      term.show();
    }
    if (outcome.warning) void vscode.window.showWarningMessage(`SwapDeck: ${outcome.warning}`);
    if (outcome.switch && !outcome.switch.switched) {
      void vscode.window.showWarningMessage(`SwapDeck: ${outcome.switch.message}`);
    }
    if (outcome.list) {
      state = {
        ...(state ?? (await refresh())),
        list: outcome.list,
        error: null,
        updatedAtMs: Date.now(),
      };
      void pollNotifications(outcome.list);
      hub.post({ type: 'state', list: outcome.list });
      render();
    } else {
      await refresh();
    }
  }

  /** Chiede la conferma dovuta (regola del Rust) con i dialoghi di VS Code. */
  async function confirm(action: CswapAction, label: string): Promise<boolean> {
    const active = state?.list?.activeAccountNumber;
    const args = ['confirm', '-'];
    if (state?.list) args.push('--active', String(active ?? 'none'));
    const needed = await cli<Confirmation>(args, { stdin: JSON.stringify(action) });
    return needed.kind === 'none' || askConfirmation(needed, label);
  }

  /** Dialogo modale; per la conferma con parola (`purge`) poi anche la parola da digitare. */
  async function askConfirmation(
    needed: Exclude<Confirmation, { kind: 'none' }>,
    label: string,
  ): Promise<boolean> {
    const ok = t('Confirm');
    const choice = await vscode.window.showWarningMessage(label, { modal: true }, ok);
    if (choice !== ok) return false;
    if (needed.kind === 'simple') return true;
    const typed = await vscode.window.showInputBox({
      prompt: t('{0} — type "{1}" to confirm', label, needed.word),
      ignoreFocusOut: true,
      validateInput: (v) => (v === needed.word ? undefined : t('Type "{0}"', needed.word)),
    });
    return typed === needed.word;
  }

  async function runSwitch(target: number | SwitchStrategy): Promise<ActionOutcome | undefined> {
    const action: CswapAction =
      typeof target === 'number'
        ? { kind: 'switch', target }
        : { kind: 'switch', strategy: target };
    const label =
      typeof target === 'number'
        ? t('Switch to account #{0}?', target)
        : t('Switch account ({0})?', target);
    try {
      if (!(await confirm(action, label))) return undefined;
      const outcome = await vscode.window.withProgress(
        {
          location: vscode.ProgressLocation.Notification,
          title: t('SwapDeck: switching account…'),
        },
        () =>
          cli<ActionOutcome>(['action', '-'], {
            stdin: JSON.stringify(action),
            timeoutMs: ACTION_TIMEOUT_MS,
          }),
      );
      await afterAction(outcome);
      return outcome;
    } catch (error) {
      const info = toCswapError(error);
      output.error(`switch: ${info.kind}: ${info.message}`);
      void vscode.window.showErrorMessage(t('SwapDeck: switch failed: {0}', info.message));
      if (!(error instanceof CliError)) throw error;
      return undefined;
    }
  }

  async function pickAccount(): Promise<void> {
    const current = state?.list ? state : await refresh();
    type Item = vscode.QuickPickItem & { target?: number | SwitchStrategy };
    const items: Item[] = [
      { label: `$(sparkle) ${t('Best account')}`, description: 'best', target: 'best' },
      {
        label: `$(arrow-right) ${t('Next available')}`,
        description: 'next-available',
        target: 'next-available',
      },
    ];
    const accounts = current.list?.accounts ?? [];
    if (accounts.length > 0)
      items.push({ label: t('Accounts'), kind: vscode.QuickPickItemKind.Separator });
    for (const a of accounts) {
      const usage = displayUsage(a)?.usage;
      const pct = (v: number | undefined) => (v === undefined ? '—' : `${displayPct(v)}%`);
      const flags = [a.active ? t('active') : '', a.disabled ? t('disabled') : '']
        .filter(Boolean)
        .join(', ');
      const item: Item = {
        label: `${a.active ? '$(check) ' : ''}#${a.number} ${accountLabel(a)}`,
        description: `5h ${pct(usage?.fiveHour?.pct)} · 7d ${pct(usage?.sevenDay?.pct)}${flags ? ` · ${flags}` : ''}`,
        detail: shortEmail(a.email, 40),
      };
      if (!a.active && !a.disabled) item.target = a.number;
      items.push(item);
    }
    const picked = await vscode.window.showQuickPick(items, {
      placeHolder: t('Switch SwapDeck account'),
    });
    if (picked?.target !== undefined) await runSwitch(picked.target);
  }

  /** «SwapDeck: Cambia mappa»: quick pick delle mappe con quella attiva spuntata, poi `select`. */
  async function pickMap(): Promise<void> {
    let profiles: EngineProfiles;
    try {
      profiles = await cli<EngineProfiles>(['engine', 'profiles']);
    } catch (error) {
      const info = toCswapError(error);
      void vscode.window.showErrorMessage(t('SwapDeck: map change failed: {0}', info.message));
      if (!(error instanceof CliError)) throw error;
      return;
    }
    type Item = vscode.QuickPickItem & { id: string };
    const items: Item[] = profiles.profiles.map((p) => ({
      id: p.id,
      label: `${p.id === profiles.activeId ? '$(check) ' : ''}${p.name}`,
      ...(p.id === profiles.activeId ? { description: t('active') } : {}),
    }));
    const picked = await vscode.window.showQuickPick(items, {
      placeHolder: t('Choose the SwapDeck map'),
    });
    if (!picked || picked.id === profiles.activeId) return;
    const command: ProfileCommand = { op: 'select', id: picked.id };
    try {
      await cli(['engine', 'profiles', '-'], { stdin: JSON.stringify(command) });
      void vscode.window.showInformationMessage(
        t(
          'SwapDeck: active map is now {0}',
          profiles.profiles.find((p) => p.id === picked.id)?.name ?? picked.id,
        ),
      );
      await refresh();
    } catch (error) {
      const info = toCswapError(error);
      output.error(`mappa: ${info.kind}: ${info.message}`);
      void vscode.window.showErrorMessage(t('SwapDeck: map change failed: {0}', info.message));
      if (!(error instanceof CliError)) throw error;
    }
  }

  const syncEngine = () => {
    const s = settings();
    if (s.engineEnabled && !engine.running) {
      engineWarned = false;
      waitingLock = false;
      output.info(`[motore] avvio: ${cliPath()} headless`);
      engine.start(cliPath());
    } else if (!s.engineEnabled && engine.running) {
      output.info('[motore] arresto');
      void engine.stop().then(syncEngine);
    } else if (!s.engineEnabled) {
      waitingLock = false;
    }
    render();
  };

  /**
   * «Avvia/Ferma motore» come «Accendi» e «Motore acceso» della scheda Regole: `engine.enabled`
   * delle regole (condivise con le altre finestre e con l'app), riletto subito prima di scriverlo.
   * Il processo del motore resta acceso (storico e notifiche); avviando, lo si accende se era spento.
   */
  const setEngine = async (enabled: boolean) => {
    if (enabled && !settings().engineEnabled) {
      await vscode.workspace
        .getConfiguration(SECTION)
        .update('engine.enabled', true, vscode.ConfigurationTarget.Global);
    }
    try {
      const current = await cli<EngineState>(['engine', 'state']);
      if (current.rules.enabled !== enabled) {
        await cli(['engine', 'rules', 'set', '-'], {
          stdin: JSON.stringify({ ...current.rules, enabled }),
        });
      }
    } catch (error) {
      const info = toCswapError(error);
      output.error(`motore: ${info.kind}: ${info.message}`);
      void vscode.window.showErrorMessage(t('SwapDeck: engine change failed: {0}', info.message));
      if (!(error instanceof CliError)) throw error;
      return;
    }
    syncEngine();
    void vscode.window.showInformationMessage(
      enabled
        ? t('SwapDeck: engine on, it switches accounts by the rules')
        : t('SwapDeck: engine off, no automatic switches'),
    );
    await refresh();
  };

  /** Apre (o porta in primo piano) la vista SwapDeck della barra delle attività. */
  const showView = () => vscode.commands.executeCommand(`${SIDEBAR_VIEW_ID}.focus`);

  context.subscriptions.push(
    output,
    status,
    hub,
    panel,
    sidebar,
    vscode.window.registerWebviewViewProvider(SIDEBAR_VIEW_ID, sidebar),
    vscode.commands.registerCommand('swapdeck.showView', showView),
    vscode.commands.registerCommand('swapdeck.openPanel', () => panel.show()),
    vscode.commands.registerCommand('swapdeck.refresh', () => refresh()),
    vscode.commands.registerCommand('swapdeck.switchAccount', () => pickAccount()),
    vscode.commands.registerCommand('swapdeck.switchMap', () => pickMap()),
    vscode.commands.registerCommand('swapdeck.startEngine', () => setEngine(true)),
    vscode.commands.registerCommand('swapdeck.stopEngine', () => setEngine(false)),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (!e.affectsConfiguration(SECTION)) return;
      if (
        e.affectsConfiguration(`${SECTION}.engine.enabled`) ||
        e.affectsConfiguration(`${SECTION}.cliPath`)
      ) {
        if (e.affectsConfiguration(`${SECTION}.cliPath`) && engine.running) {
          void engine.stop().then(syncEngine);
        } else {
          syncEngine();
        }
      }
      schedule();
      render();
    }),
    // Tornando su questa finestra si rilegge (rete di sicurezza oltre al watcher e al polling).
    vscode.window.onDidChangeWindowState((w) => {
      if (w.focused && Date.now() - (state?.updatedAtMs ?? 0) > FOCUS_REFRESH_AFTER_MS) {
        void refresh();
      }
    }),
    { dispose: () => clearTimeout(timer) },
    { dispose: () => sync?.dispose() },
  );
  deactivateEngine = () => engine.stop(DEACTIVATE_GRACE_MS);

  render();
  syncEngine();
  void refresh().finally(schedule);

  return {
    refresh,
    state: () => state,
    statusText: () => status.text,
    switchTo: runSwitch,
    engineRunning: () => engine.running,
    panelOpen: () => panel.isOpen,
    panelReady: () => hub.readyWebviews > 0,
    viewResolved: () => sidebar.isResolved,
    statusCommand: () =>
      typeof status.command === 'string' ? status.command : status.command?.command,
    syncWatching: () => sync?.watching ?? false,
    signalSync,
    cliPath,
  };
}

let deactivateEngine: (() => Promise<void>) | undefined;

/** Ferma il motore avviato da questa finestra (VS Code concede pochi secondi). */
export async function deactivate(): Promise<void> {
  await deactivateEngine?.();
  deactivateEngine = undefined;
}
