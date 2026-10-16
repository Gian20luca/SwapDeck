/** Estensione SwapDeck: barra di stato, cambio rapido, pannello, notifiche e motore opzionale. */
import type { ActionOutcome, Confirmation, CswapAction, SwitchStrategy } from '@swapdeck/types';
import { displayUsage } from '@swapdeck/ui/account';
import { displayPct, shortEmail } from '@swapdeck/ui/format';
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
import { UsageNotifier, type Notice } from './notifier.js';
import { DashboardPanel } from './panel.js';
import type { PickParams } from './protocol.js';
import { Refresher } from './refresher.js';
import {
  accountLabel,
  colorThresholds,
  engineView,
  statusModel,
  type Translate,
} from './statusBar.js';

const SECTION = 'swapdeck';
const MIN_INTERVAL_SECONDS = 15;

/** API restituita da `activate`, usata solo dai test dell'estensione. */
export interface SwapDeckApi {
  refresh(): Promise<CliState>;
  state(): CliState | undefined;
  statusText(): string;
  switchTo(target: number | SwitchStrategy): Promise<ActionOutcome | undefined>;
  engineRunning(): boolean;
  panelOpen(): boolean;
  /** Il webview ha caricato lo script (CSP e nonce corretti) e ha chiesto lo stato. */
  panelReady(): boolean;
  cliPath(): string;
}

const t: Translate = (message, ...args) => vscode.l10n.t(message, ...args);

function settings() {
  const c = vscode.workspace.getConfiguration(SECTION);
  const interval = Math.max(MIN_INTERVAL_SECONDS, c.get<number>('refreshIntervalSeconds', 60));
  return {
    intervalMs: interval * 1000,
    cliPath: c.get<string>('cliPath', ''),
    engineEnabled: c.get<boolean>('engine.enabled', false),
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
  let panelReady = false;
  const notifier = new UsageNotifier();

  const status = vscode.window.createStatusBarItem(
    'swapdeck.status',
    vscode.StatusBarAlignment.Left,
    100,
  );
  status.name = 'SwapDeck';
  status.command = 'swapdeck.switchAccount';

  const engine = new EngineProcess({
    onLine: (line) => output.info(`[motore] ${line}`),
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
      engineView(state, engine.running),
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

  const notify = (notices: Notice[]) => {
    for (const n of notices) {
      if (n.kind === 'switch') {
        void vscode.window.showInformationMessage(t('SwapDeck: active account is now {0}', n.name));
      } else {
        const text = t(
          'SwapDeck: {0} has used {1}% of the {2} window (threshold {3}%)',
          n.name,
          displayPct(n.pct),
          n.window,
          n.threshold,
        );
        void (n.threshold >= colorThresholds(settings().thresholds).danger
          ? vscode.window.showWarningMessage(text)
          : vscode.window.showInformationMessage(text));
      }
    }
  };

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

  const panel = new DashboardPanel(
    context.extensionUri,
    createRequestHandler({
      cli,
      lastState: () => state,
      refresh: () => refresh(),
      onOutcome: (outcome) => afterAction(outcome),
      confirm: (action, needed) =>
        askConfirmation(needed, t('SwapDeck: confirm "{0}"?', describeAction(action))),
      pick: pickPath,
    }),
    () => {
      panelReady = true;
      if (state?.list) panel.post({ type: 'state', list: state.list });
    },
  );

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
      notify(notifier.update(next.list, settings().thresholds));
      panel.post({ type: 'state', list: next.list });
    }
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
      notify(notifier.update(outcome.list, settings().thresholds));
      panel.post({ type: 'state', list: outcome.list });
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

  const setEngine = async (enabled: boolean) => {
    await vscode.workspace
      .getConfiguration(SECTION)
      .update('engine.enabled', enabled, vscode.ConfigurationTarget.Global);
    syncEngine();
  };

  context.subscriptions.push(
    output,
    status,
    panel,
    vscode.commands.registerCommand('swapdeck.openPanel', () => panel.show()),
    vscode.commands.registerCommand('swapdeck.refresh', () => refresh()),
    vscode.commands.registerCommand('swapdeck.switchAccount', () => pickAccount()),
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
    { dispose: () => clearTimeout(timer) },
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
    panelReady: () => panelReady,
    cliPath,
  };
}

let deactivateEngine: (() => Promise<void>) | undefined;

/** Ferma il motore avviato da questa finestra (VS Code concede pochi secondi). */
export async function deactivate(): Promise<void> {
  await deactivateEngine?.();
  deactivateEngine = undefined;
}
