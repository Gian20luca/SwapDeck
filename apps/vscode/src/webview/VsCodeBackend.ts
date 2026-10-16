import type {
  ActionOutcome as RustOutcome,
  ConfigGetResponse,
  ConfigListResponse,
  Confirmation,
  CswapAction,
  CswapError,
  ListResponse,
  StatusResponse,
} from '@swapdeck/types';
import {
  BackendError,
  type ActionOutcome,
  type Backend,
  type HistoryPoint,
  type HistoryQuery,
  type Unsubscribe,
} from '@swapdeck/ui';
import type { HostMessage, RequestParams, ViewSettings, WebviewMessage } from '../protocol.js';

export type PostFn = (message: WebviewMessage) => void;
export type SubscribeFn = (handler: (message: HostMessage) => void) => Unsubscribe;

interface Pending {
  resolve: (value: unknown) => void;
  reject: (error: BackendError) => void;
}

/** `Backend` del webview di VS Code: ogni chiamata è un messaggio all'extension host. */
export class VsCodeBackend implements Backend {
  /** Le conferme le impone l'extension host (`bridge.ts`) con i dialoghi modali di VS Code. */
  readonly hostConfirms = true;
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private readonly stateListeners = new Set<(state: ListResponse) => void>();
  private readonly viewListeners = new Set<(settings: ViewSettings) => void>();
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
      case 'state':
        for (const l of this.stateListeners) l(message.list);
        return;
      case 'view':
        for (const l of this.viewListeners) l(message.settings);
        return;
    }
  }

  private request<T>(req: RequestParams): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
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
    const params: { account?: number; sinceSeconds?: number } = {};
    if (query?.account !== undefined) params.account = query.account;
    if (query?.sinceSeconds !== undefined) params.sinceSeconds = query.sinceSeconds;
    return this.request({ method: 'getHistory', params });
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

  onStateChanged(listener: (state: ListResponse) => void): Unsubscribe {
    this.stateListeners.add(listener);
    return () => this.stateListeners.delete(listener);
  }

  /** Lingua e tema di VS Code, inviati dall'host all'avvio e quando cambiano. */
  onViewSettings(listener: (settings: ViewSettings) => void): Unsubscribe {
    this.viewListeners.add(listener);
    return () => this.viewListeners.delete(listener);
  }
}
