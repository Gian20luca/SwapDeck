import type {
  ConfigGetResponse,
  ConfigListResponse,
  Confirmation,
  CswapAction,
  CswapError,
  ListResponse,
  StatusResponse,
  SwitchResponse,
} from '@swapdeck/types';
import {
  BackendError,
  type ActionOutcome,
  type Backend,
  type HistoryPoint,
  type HistoryQuery,
  type Unsubscribe,
} from '@swapdeck/ui';

/** Evento del ciclo di lettura in Rust (`STATE_EVENT` in `src-tauri/src/state.rs`). */
export const STATE_EVENT = 'state-changed';

/** Stato inviato dal Rust a ogni lettura (`Snapshot`). */
export interface Snapshot {
  list: ListResponse | null;
  error: CswapError | null;
  updatedAtMs: number;
  engineSuspended: boolean;
}

/** Esito di `run_action` (`Outcome` del Rust). */
interface RustOutcome {
  message: string;
  list?: ListResponse;
  switch?: SwitchResponse;
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

/** `Backend` dell'app desktop: comandi `invoke` di Tauri ed evento `state-changed`. */
export class TauriBackend implements Backend {
  constructor(
    private readonly invoke: InvokeFn,
    private readonly listen: ListenFn,
  ) {}

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
    const result: ActionOutcome = { message: outcome.message };
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

  getAppSettings(): Promise<AppSettings> {
    return this.call('get_app_settings');
  }

  setAutostart(enabled: boolean): Promise<AppSettings> {
    return this.call('set_autostart', { enabled });
  }

  showMain(): Promise<void> {
    return this.call('show_main');
  }
}
