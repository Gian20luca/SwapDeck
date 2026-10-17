import type {
  ConfigGetResponse,
  ConfigListResponse,
  Confirmation,
  CswapAction,
  CswapError,
  EngineProfiles,
  EngineRules,
  ListResponse,
  NotificationConfig,
  ProfileCommand,
  SetupDiagnosis,
  SetupOutcome,
  SetupProgress,
  StatusResponse,
  SuspendReason,
  SwitchResponse,
  UpdateInfo,
  UsageBreakdown,
  UsageQuery,
} from '@swapdeck/types';
import {
  BackendError,
  type ActionOutcome,
  type AppPreferences,
  type Backend,
  type DirectoryMapping,
  type EngineState,
  type HistoryEvents,
  type HistoryPoint,
  type HistoryQuery,
  type NotificationPatch,
  type MiniWindowControls,
  type PickOptions,
  type SetupBackend,
  type Unsubscribe,
} from '@swapdeck/ui';

/** Evento del ciclo di lettura in Rust (`STATE_EVENT` in `src-tauri/src/state.rs`). */
export const STATE_EVENT = 'state-changed';

/** Avanzamento di installazione e aggiornamento di cswap (`PROGRESS_EVENT` in `src-tauri/src/setup_cmd.rs`). */
export const SETUP_PROGRESS_EVENT = 'setup-progress';

/** Stato inviato dal Rust a ogni lettura (`Snapshot`). */
export interface Snapshot {
  list: ListResponse | null;
  error: CswapError | null;
  updatedAtMs: number;
  engineSuspended: boolean;
  /** `cswap auto` avviato dall'app è in esecuzione. */
  autoRunning: boolean;
  /** Stato del motore all'ultimo giro che ha deciso (`EngineView` in `engine_loop.rs`). */
  engine: EngineView | null;
}

export interface EngineView {
  /** Il lock del motore è di questa app. */
  holder: boolean;
  enabled: boolean;
  dryRun: boolean;
  suspended: SuspendReason | null;
  detail: string[];
}

/** Esito di `run_action` (`Outcome` del Rust). */
interface RustOutcome {
  message: string;
  list?: ListResponse;
  switch?: SwitchResponse;
  /** Azione riuscita ma non verificata. */
  warning?: string;
}

/** Impostazioni dell'app (`AppSettingsView` del Rust). */
export interface AppSettings {
  autostart: boolean;
  autostartManaged: boolean;
  version: string;
}

export type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
export type ListenFn = <T>(
  event: string,
  handler: (event: { payload: T }) => void,
) => Promise<() => void>;

/** Parte di `@tauri-apps/plugin-dialog` che usa l'app (iniettabile per i test). */
export interface DialogApi {
  open(options: {
    directory?: boolean;
    multiple?: false;
    title?: string;
    defaultPath?: string;
    filters?: { name: string; extensions: string[] }[];
  }): Promise<string | null>;
  save(options: {
    title?: string;
    defaultPath?: string;
    filters?: { name: string; extensions: string[] }[];
  }): Promise<string | null>;
}

function isCswapError(value: unknown): value is CswapError {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { kind?: unknown }).kind === 'string' &&
    typeof (value as { message?: unknown }).message === 'string'
  );
}

/** Converte il rifiuto di `invoke` (un `CswapError` serializzato o un testo) in `BackendError`. */
export function toBackendError(error: unknown): BackendError {
  if (error instanceof BackendError) return error;
  if (isCswapError(error)) return new BackendError(error);
  const message = error instanceof Error ? error.message : String(error);
  return new BackendError({ kind: 'failed', message });
}

/** Parte di `Window` di `@tauri-apps/api/window` che usa l'app (iniettabile per i test). */
export interface WindowApi {
  setTheme(theme: 'light' | 'dark' | null): Promise<void>;
  /** Etichetta della finestra: `main` o `mini`. */
  label: string;
}

/** `Backend` dell'app desktop: comandi `invoke` di Tauri ed evento `state-changed`. */
export class TauriBackend implements Backend, SetupBackend {
  constructor(
    private readonly invoke: InvokeFn,
    private readonly listen: ListenFn,
    private readonly dialog?: DialogApi,
    windowApi?: WindowApi,
  ) {
    if (windowApi) {
      this.setWindowTheme = (theme) => windowApi.setTheme(theme);
      if (windowApi.label === 'mini') this.miniWindow = this.miniControls();
    }
  }

  /** Tema della cornice nativa (barra del titolo di Windows), se la finestra lo permette. */
  readonly setWindowTheme?: (theme: 'light' | 'dark' | null) => Promise<void>;
  /** Finestra del mini-widget: solo dentro la finestra `mini`. */
  readonly miniWindow?: MiniWindowControls;

  private miniControls(): MiniWindowControls {
    return {
      hide: () => this.call('mini_hide'),
      setCompact: (compact) => this.call('mini_set_compact', { compact }),
      isCompact: () => this.call('mini_is_compact'),
      fitHeight: (height) => this.call('mini_fit_height', { height }),
    };
  }

  private async call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    try {
      return await this.invoke<T>(cmd, args);
    } catch (error) {
      throw toBackendError(error);
    }
  }

  listAccounts(options?: { refresh?: boolean }): Promise<ListResponse> {
    return this.call('list_accounts', { refresh: options?.refresh ?? false });
  }

  getStatus(): Promise<StatusResponse> {
    return this.call('get_status');
  }

  getConfig(): Promise<ConfigListResponse> {
    return this.call('get_config');
  }

  getConfigValue(key: string): Promise<ConfigGetResponse> {
    return this.call('get_config_value', { key });
  }

  getHistory(query?: HistoryQuery): Promise<HistoryPoint[]> {
    return this.call('get_history', { query: query ?? {} });
  }

  confirmationFor(action: CswapAction): Promise<Confirmation> {
    return this.call('confirmation_for', { action });
  }

  async runAction(action: CswapAction): Promise<ActionOutcome> {
    const outcome = await this.call<RustOutcome>('run_action', { action });
    const message = [outcome.message, outcome.warning].filter(Boolean).join(' — ');
    const result: ActionOutcome = { message };
    if (outcome.switch) result.switch = outcome.switch;
    return result;
  }

  /** Ogni lettura riuscita del ciclo in Rust; gli errori restano nell'ultimo `Snapshot`. */
  onStateChanged(listener: (state: ListResponse) => void): Unsubscribe {
    return this.onSnapshot((snap) => {
      if (snap.list) listener(snap.list);
    });
  }

  /** Lo `Snapshot` completo (lista, errore, motore sospeso). */
  onSnapshot(listener: (snap: Snapshot) => void): Unsubscribe {
    let off: (() => void) | undefined;
    let closed = false;
    this.listen<Snapshot>(STATE_EVENT, (event) => {
      if (!closed) listener(event.payload);
    })
      .then((unlisten) => {
        if (closed) unlisten();
        else off = unlisten;
      })
      .catch(() => {
        // Ascolto non disponibile (permesso mancante): la UI resta con le letture esplicite.
      });
    return () => {
      closed = true;
      off?.();
    };
  }

  /** Ultimo `Snapshot` del Rust (all'apertura, prima del prossimo evento). */
  getSnapshot(): Promise<Snapshot> {
    return this.call('get_snapshot');
  }

  getAppSettings(): Promise<AppSettings> {
    return this.call('get_app_settings');
  }

  setAutostart(enabled: boolean): Promise<AppSettings> {
    return this.call('set_autostart', { enabled });
  }

  showMain(): Promise<void> {
    return this.call('show_main');
  }

  /*
   * Gestione completa (step 5). I comandi `list_mappings`, `get_engine_state`, `set_engine_rules`,
   * `get_preferences`, `set_preferences` e `get_history_events` sono da implementare in
   * `src-tauri/src/commands.rs` (vedi HANDOFF.md dello step 5); finché mancano la UI mostra l'errore.
   */

  listMappings(): Promise<DirectoryMapping[]> {
    return this.call('list_mappings');
  }

  getEngineState(): Promise<EngineState> {
    return this.call('get_engine_state');
  }

  setEngineRules(rules: EngineRules): Promise<EngineState> {
    return this.call('set_engine_rules', { rules });
  }

  getEngineProfiles(): Promise<EngineProfiles> {
    return this.call('get_engine_profiles');
  }

  applyEngineProfile(command: ProfileCommand): Promise<EngineState> {
    return this.call('apply_engine_profile', { command });
  }

  getNotificationSettings(): Promise<NotificationConfig> {
    return this.call('get_notification_settings');
  }

  setNotificationSettings(patch: NotificationPatch): Promise<NotificationConfig> {
    return this.call('set_notification_settings', { patch });
  }

  getPreferences(): Promise<AppPreferences> {
    return this.call('get_preferences');
  }

  setPreferences(prefs: Partial<AppPreferences>): Promise<AppPreferences> {
    return this.call('set_preferences', { prefs });
  }

  getHistoryEvents(query?: HistoryQuery): Promise<HistoryEvents> {
    return this.call('get_history_events', { query: query ?? {} });
  }

  getUsageBreakdown(query: UsageQuery): Promise<UsageBreakdown> {
    return this.call('get_usage_breakdown', { query });
  }

  diagnoseSetup(): Promise<SetupDiagnosis> {
    return this.call('diagnose_setup');
  }

  installCswap(onProgress: (event: SetupProgress) => void): Promise<SetupOutcome> {
    return this.withProgress('install_cswap', onProgress);
  }

  /** Il Rust ferma `cswap auto` e tiene fermi ciclo di lettura e motore durante l'aggiornamento. */
  upgradeCswap(onProgress: (event: SetupProgress) => void): Promise<SetupOutcome> {
    return this.withProgress('upgrade_cswap', onProgress);
  }

  checkCswapUpdate(options?: { force?: boolean }): Promise<UpdateInfo> {
    return this.call('check_cswap_update', { force: options?.force ?? false });
  }

  dismissCswapUpdate(version: string): Promise<void> {
    return this.call('dismiss_cswap_update', { version });
  }

  /** Ascolta l'avanzamento prima di avviare il comando e smette alla fine, anche se non riesce. */
  private async withProgress(
    cmd: string,
    onProgress: (event: SetupProgress) => void,
  ): Promise<SetupOutcome> {
    const unlisten = await this.listen<SetupProgress>(SETUP_PROGRESS_EVENT, (event) =>
      onProgress(event.payload),
    );
    try {
      return await this.call<SetupOutcome>(cmd);
    } finally {
      unlisten();
    }
  }

  private dialogApi(): DialogApi {
    if (!this.dialog) throw toBackendError('finestre di dialogo non disponibili');
    return this.dialog;
  }

  /** File da aprire con il selettore nativo (`tauri-plugin-dialog`); `null` se annullato. */
  async pickOpenFile(options: PickOptions = {}): Promise<string | null> {
    return this.dialogApi().open({ ...dialogOptions(options), multiple: false });
  }

  /** Percorso in cui salvare un file, con il selettore nativo; `null` se annullato. */
  async pickSaveFile(options: PickOptions = {}): Promise<string | null> {
    return this.dialogApi().save(dialogOptions(options));
  }

  /** Cartella scelta con il selettore nativo; `null` se annullato. */
  async pickFolder(options: PickOptions = {}): Promise<string | null> {
    return this.dialogApi().open({ ...dialogOptions(options), directory: true, multiple: false });
  }
}

function dialogOptions(options: PickOptions) {
  return {
    ...(options.title ? { title: options.title } : {}),
    ...(options.defaultPath ? { defaultPath: options.defaultPath } : {}),
    ...(options.filters ? { filters: options.filters } : {}),
  };
}
