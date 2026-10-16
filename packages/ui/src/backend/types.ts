import type {
  ConfigGetResponse,
  ConfigListResponse,
  Confirmation,
  CswapAction,
  CswapError,
  ListResponse,
  SetupDiagnosis,
  SetupOutcome,
  SetupProgress,
  StatusResponse,
  SwitchResponse,
  UpdateInfo,
} from '@swapdeck/types';

export type Unsubscribe = () => void;

/**
 * Un punto dello storico dei consumi per un account.
 *
 * NOTA: lo storico (JSONL, step 2) non ha ancora un tipo generato dal Rust. Questo è il formato che
 * la UI si aspetta dal `Backend`; quando `@swapdeck/types` esporterà il tipo dello storico va
 * sostituito con quello (e il FakeBackend adattato).
 */
export interface HistoryPoint {
  /** Istante in secondi Unix. */
  ts: number;
  account: number;
  fiveHourPct: number | null;
  sevenDayPct: number | null;
  /** Consumo atteso al ritmo lineare della finestra, quando è noto. */
  fiveHourExpectedPct: number | null;
  sevenDayExpectedPct: number | null;
}

export interface HistoryQuery {
  /** Solo questo account; senza, tutti. */
  account?: number;
  /** Inizio dell'intervallo in secondi Unix; default: 7 giorni fa. */
  sinceSeconds?: number;
}

/** Esito di un'azione: `switch` porta la risposta di cswap, le altre al massimo un messaggio. */
export interface ActionOutcome {
  switch?: SwitchResponse;
  message?: string;
}

/** Errore di un comando, già ripulito da token e email (`CswapError` del Rust). */
export class BackendError extends Error {
  readonly info: CswapError;
  constructor(info: CswapError) {
    super(info.message);
    this.name = 'BackendError';
    this.info = info;
  }
}

/**
 * Interfaccia tra la UI e chi la ospita. Due implementazioni reali: `invoke` di Tauri (step 4) e
 * `postMessage` del webview di VS Code (step 7); `FakeBackend` (`@swapdeck/ui/fake`) per sviluppo e test.
 * Tutti i metodi sono asincroni e in caso di errore rifiutano con `BackendError`.
 * Un'azione che cambia lo stato (switch, disable, ...) deve far scattare `onStateChanged` con la lista aggiornata: la UI non la rilegge da sola.
 */
export interface Backend {
  /**
   * L'host chiede da sé le conferme dovute dentro `runAction` (dialoghi nativi, es. VS Code):
   * la UI allora non mostra il suo `ConfirmDialog`, per non chiedere due volte.
   */
  readonly hostConfirms?: boolean;
  /** `cswap list --json`. Con `refresh` forza una nuova lettura invece di usare l'ultima. */
  listAccounts(options?: { refresh?: boolean }): Promise<ListResponse>;
  /** `cswap status --json`. */
  getStatus(): Promise<StatusResponse>;
  /** `cswap config --json`. */
  getConfig(): Promise<ConfigListResponse>;
  /** `cswap config get KEY --json`. */
  getConfigValue(key: string): Promise<ConfigGetResponse>;
  /** Storico dei consumi, dal più vecchio al più recente. */
  getHistory(query?: HistoryQuery): Promise<HistoryPoint[]>;
  /** Conferma da chiedere prima dell'azione (la regola sta nel Rust, `CswapAction::confirmation`). */
  confirmationFor(action: CswapAction): Promise<Confirmation>;
  /** Esegue un'azione di F0: switch, add, add-token, remove, disable/enable, alias, swap, move, map, config, export/import, unclaimed, purge, upgrade, run, auto. */
  runAction(action: CswapAction): Promise<ActionOutcome>;
  /** Evento di aggiornamento dello stato: il motore o un'azione hanno prodotto una nuova lista. */
  onStateChanged(listener: (state: ListResponse) => void): Unsubscribe;
}

/**
 * Primo avvio e manutenzione di cswap (F5). Facoltativa: un host che non la implementa non mostra
 * `SetupWizard` né `CswapUpdateNotice` (vedi `hasSetup`). Errori come `Backend`.
 */
export interface SetupBackend {
  /** Diagnosi di cswap e uv (presenza, percorso, versione, `schemaVersion`, numero di account). Solo letture. */
  diagnoseSetup(): Promise<SetupDiagnosis>;
  /**
   * Installa per utente, senza admin, uv (se manca) e claude-swap, poi verifica `--version` e `list --json`.
   * L'avanzamento arriva a passi su `onProgress`, con l'output dei processi riga per riga già ripulito.
   */
  installCswap(onProgress: (event: SetupProgress) => void): Promise<SetupOutcome>;
  /** Versione installata contro l'ultima su PyPI (cache di 6 ore; `force` la ignora). */
  checkCswapUpdate(options?: { force?: boolean }): Promise<UpdateInfo>;
  /** Ricorda che l'avviso per `version` è stato chiuso (`UpdateInfo.dismissed`). */
  dismissCswapUpdate(version: string): Promise<void>;
  /**
   * `uv tool upgrade claude-swap` e verifica di `schemaVersion`. L'host ferma prima i propri processi cswap
   * (ciclo di lettura, motore, `cswap auto`) e li riavvia dopo. Errori tipici: `busy` (file in uso, o motore
   * attivo in un'altra SwapDeck: fermarlo e riprovare), `unsupportedSchema` (serve una SwapDeck più recente;
   * cswap è già stato riportato da solo alla versione precedente, passo `rollbackCswap`).
   */
  upgradeCswap(onProgress: (event: SetupProgress) => void): Promise<SetupOutcome>;
}

/** L'host offre anche l'installazione e l'aggiornamento di cswap. */
export function hasSetup(backend: Backend): backend is Backend & SetupBackend {
  const b = backend as Partial<SetupBackend>;
  return (
    typeof b.diagnoseSetup === 'function' &&
    typeof b.installCswap === 'function' &&
    typeof b.checkCswapUpdate === 'function' &&
    typeof b.dismissCswapUpdate === 'function' &&
    typeof b.upgradeCswap === 'function'
  );
}
