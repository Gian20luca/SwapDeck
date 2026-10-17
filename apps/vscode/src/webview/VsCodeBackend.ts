import type {
  ActionOutcome as RustOutcome,
  AppPreferences,
  ConfigGetResponse,
  ConfigListResponse,
  Confirmation,
  CswapAction,
  CswapError,
  DirectoryMapping,
  EngineProfiles,
  EngineRules,
  EngineState,
  NotificationConfig,
  ProfileCommand,
  HistoryEvents,
  ListResponse,
  SetupDiagnosis,
  SetupOutcome,
  SetupProgress,
  StatusResponse,
  UpdateInfo,
  UsageBreakdown,
  UsageQuery,
} from '@swapdeck/types';
import {
  BackendError,
  type ActionOutcome,
  type Backend,
  type HistoryPoint,
  type HistoryQuery,
  type NotificationPatch,
  type PickOptions,
  type SetupBackend,
  type Unsubscribe,
} from '@swapdeck/ui';
import type {
  HistoryParams,
  HostMessage,
  PickParams,
  RequestParams,
  ViewSettings,
  WebviewMessage,
} from '../protocol.js';

export type PostFn = (message: WebviewMessage) => void;
export type SubscribeFn = (handler: (message: HostMessage) => void) => Unsubscribe;

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: BackendError) => void;
  /** Avanzamento di installazione o aggiornamento di cswap. */
  progress?: (event: SetupProgress) => void;
}

/** `Backend` del webview di VS Code: ogni chiamata è un messaggio all'extension host. */
export class VsCodeBackend implements Backend, SetupBackend {
  /** Le conferme le impone l'extension host (`bridge.ts`) con i dialoghi modali di VS Code. */
  readonly hostConfirms = true;
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private readonly stateListeners = new Set<(state: ListResponse) => void>();
  private readonly viewListeners = new Set<(settings: ViewSettings) => void>();
  private readonly configListeners = new Set<() => void>();
  private readonly unsubscribe: Unsubscribe;

  constructor(
    private readonly post: PostFn,
    subscribe: SubscribeFn,
  ) {
    this.unsubscribe = subscribe((message) => this.receive(message));
  }

  /** Avvisa l'host che il webview è pronto a ricevere stato e impostazioni. */
  ready(): void {
    this.post({ type: 'ready' });
  }

  dispose(): void {
    this.unsubscribe();
    for (const p of this.pending.values()) {
      p.reject(new BackendError({ kind: 'cancelled', message: 'webview chiuso' }));
    }
    this.pending.clear();
    this.stateListeners.clear();
    this.viewListeners.clear();
    this.configListeners.clear();
  }

  private receive(message: HostMessage): void {
    switch (message.type) {
      case 'response': {
        const p = this.pending.get(message.id);
        if (!p) return;
        this.pending.delete(message.id);
        if (message.ok) p.resolve(message.result);
        else p.reject(new BackendError(message.error));
        return;
      }
      case 'progress':
        this.pending.get(message.id)?.progress?.(message.event);
        return;
      case 'state':
        for (const l of this.stateListeners) l(message.list);
        return;
      case 'view':
        for (const l of this.viewListeners) l(message.settings);
        return;
      case 'configChanged':
        for (const l of this.configListeners) l();
        return;
    }
  }

  private request<T>(req: RequestParams, progress?: (event: SetupProgress) => void): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: resolve as (value: unknown) => void,
        reject,
        ...(progress ? { progress } : {}),
      });
      try {
        this.post({ type: 'request', id, ...req });
      } catch (error) {
        this.pending.delete(id);
        const message = error instanceof Error ? error.message : String(error);
        reject(new BackendError({ kind: 'failed', message } satisfies CswapError));
      }
    });
  }

  listAccounts(options?: { refresh?: boolean }): Promise<ListResponse> {
    return this.request({ method: 'listAccounts', params: { refresh: options?.refresh ?? false } });
  }

  getStatus(): Promise<StatusResponse> {
    return this.request({ method: 'getStatus', params: {} });
  }

  getConfig(): Promise<ConfigListResponse> {
    return this.request({ method: 'getConfig', params: {} });
  }

  getConfigValue(key: string): Promise<ConfigGetResponse> {
    return this.request({ method: 'getConfigValue', params: { key } });
  }

  getHistory(query?: HistoryQuery): Promise<HistoryPoint[]> {
    return this.request({ method: 'getHistory', params: historyParams(query) });
  }

  listMappings(): Promise<DirectoryMapping[]> {
    return this.request({ method: 'listMappings', params: {} });
  }

  getEngineState(): Promise<EngineState> {
    return this.request({ method: 'getEngineState', params: {} });
  }

  /** Salvate nella config: il motore avviato dall'estensione le rilegge al giro dopo. */
  setEngineRules(rules: EngineRules): Promise<EngineState> {
    return this.request({ method: 'setEngineRules', params: { rules } });
  }

  getEngineProfiles(): Promise<EngineProfiles> {
    return this.request({ method: 'getEngineProfiles', params: {} });
  }

  applyEngineProfile(command: ProfileCommand): Promise<EngineState> {
    return this.request({ method: 'applyEngineProfile', params: { command } });
  }

  getNotificationSettings(): Promise<NotificationConfig> {
    return this.request({ method: 'getNotificationSettings', params: {} });
  }

  setNotificationSettings(patch: NotificationPatch): Promise<NotificationConfig> {
    return this.request({ method: 'setNotificationSettings', params: { patch } });
  }

  getPreferences(): Promise<AppPreferences> {
    return this.request({ method: 'getPreferences', params: {} });
  }

  setPreferences(prefs: Partial<AppPreferences>): Promise<AppPreferences> {
    return this.request({ method: 'setPreferences', params: { prefs } });
  }

  getHistoryEvents(query?: HistoryQuery): Promise<HistoryEvents> {
    return this.request({ method: 'getHistoryEvents', params: historyParams(query) });
  }

  getUsageBreakdown(query: UsageQuery): Promise<UsageBreakdown> {
    return this.request({ method: 'getUsageBreakdown', params: { query } });
  }

  confirmationFor(action: CswapAction): Promise<Confirmation> {
    return this.request({ method: 'confirmationFor', params: { action } });
  }

  async runAction(action: CswapAction): Promise<ActionOutcome> {
    const outcome = await this.request<RustOutcome>({ method: 'runAction', params: { action } });
    const message = [outcome.message, outcome.warning].filter(Boolean).join(' — ');
    const result: ActionOutcome = { message };
    if (outcome.switch) result.switch = outcome.switch;
    return result;
  }

  /** Selettori nativi di VS Code (`showOpenDialog`/`showSaveDialog` nell'host); `null` se annullati. */
  pickOpenFile(options: PickOptions = {}): Promise<string | null> {
    return this.request({ method: 'pickOpenFile', params: pickParams(options) });
  }

  pickSaveFile(options: PickOptions = {}): Promise<string | null> {
    return this.request({ method: 'pickSaveFile', params: pickParams(options) });
  }

  pickFolder(options: PickOptions = {}): Promise<string | null> {
    return this.request({ method: 'pickFolder', params: pickParams(options) });
  }

  diagnoseSetup(): Promise<SetupDiagnosis> {
    return this.request({ method: 'diagnoseSetup', params: {} });
  }

  /** L'host chiede conferma con un dialogo modale; se annullata la promessa rifiuta con cancelled. */
  installCswap(onProgress: (event: SetupProgress) => void): Promise<SetupOutcome> {
    return this.request({ method: 'installCswap', params: {} }, onProgress);
  }

  /** L'host chiede conferma e ferma il proprio motore prima di aggiornare. */
  upgradeCswap(onProgress: (event: SetupProgress) => void): Promise<SetupOutcome> {
    return this.request({ method: 'upgradeCswap', params: {} }, onProgress);
  }

  checkCswapUpdate(options?: { force?: boolean }): Promise<UpdateInfo> {
    return this.request({ method: 'checkCswapUpdate', params: { force: options?.force ?? false } });
  }

  async dismissCswapUpdate(version: string): Promise<void> {
    await this.request({ method: 'dismissCswapUpdate', params: { version } });
  }

  onStateChanged(listener: (state: ListResponse) => void): Unsubscribe {
    this.stateListeners.add(listener);
    return () => this.stateListeners.delete(listener);
  }

  onConfigChanged(listener: () => void): Unsubscribe {
    this.configListeners.add(listener);
    return () => this.configListeners.delete(listener);
  }

  /** Lingua e tema di VS Code, inviati dall'host all'avvio e quando cambiano. */
  onViewSettings(listener: (settings: ViewSettings) => void): Unsubscribe {
    this.viewListeners.add(listener);
    return () => this.viewListeners.delete(listener);
  }
}

function historyParams(query?: HistoryQuery): HistoryParams {
  const params: HistoryParams = {};
  if (query?.account !== undefined) params.account = query.account;
  if (query?.sinceSeconds !== undefined) params.sinceSeconds = query.sinceSeconds;
  return params;
}

function pickParams(options: PickOptions): PickParams {
  return {
    ...(options.title ? { title: options.title } : {}),
    ...(options.defaultPath ? { defaultPath: options.defaultPath } : {}),
    ...(options.filters ? { filters: options.filters } : {}),
  };
}
