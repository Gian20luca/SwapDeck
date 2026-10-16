import type {
  ConfigGetResponse,
  ConfigListResponse,
  Confirmation,
  Conflict,
  CswapAction,
  CswapError,
  DecisionRecord,
  EngineRules,
  Language,
  ListResponse,
  StatusResponse,
  SuspendReason,
  SwitchRecord,
  SwitchResponse,
  Theme,
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

/** Cartella mappata a un account (`cswap map`). */
export interface DirectoryMapping {
  path: string;
  account: number;
  email?: string;
}

/** Stato del motore all'ultimo giro (specchio di `EngineView` del Rust). */
export interface EngineViewInfo {
  /** Il lock del motore è di questa istanza. */
  holder: boolean;
  enabled: boolean;
  dryRun: boolean;
  suspended: SuspendReason | null;
  /** Id dei conflitti o problemi delle regole che hanno sospeso il motore. */
  detail: string[];
}

/** Regole salvate, stato del motore e conflitti noti (`EngineRules` è nel Rust). */
export interface EngineState {
  rules: EngineRules;
  view: EngineViewInfo | null;
  conflicts: Conflict[];
}

/** Impostazioni di SwapDeck salvate nella config (`AppConfig`, senza le regole del motore). */
export interface AppPreferences {
  pollIntervalSeconds: number;
  /** Soglie delle notifiche in percentuale, crescenti. */
  notificationThresholds: number[];
  language: Language;
  theme: Theme;
}

/** Impostazioni dell'host desktop (avvio al login, versione). */
export interface HostSettings {
  autostart: boolean;
  /** `false` nelle build di sviluppo: il sistema non viene modificato. */
  autostartManaged: boolean;
  version: string;
}

/** Cambi di account e decisioni del motore, dal più vecchio al più recente. */
export interface HistoryEvents {
  switches: SwitchRecord[];
  decisions: DecisionRecord[];
}

export interface FileFilter {
  name: string;
  extensions: string[];
}

export interface PickOptions {
  title?: string;
  defaultPath?: string;
  filters?: FileFilter[];
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

  /*
   * Gestione completa (step 5). Tutti i metodi sono facoltativi: un host che non li ha
   * (per esempio l'estensione VS Code) ottiene dalla UI un avviso di funzione non disponibile,
   * oppure il solo campo di testo al posto del selettore di file.
   */

  /** Cartelle mappate (`cswap map`, che non ha `--json`: l'host ne interpreta il testo). */
  listMappings?(): Promise<DirectoryMapping[]>;
  /** Regole del motore, stato e conflitti. */
  getEngineState?(): Promise<EngineState>;
  /** Salva le regole (validate dal Rust) e restituisce lo stato aggiornato. */
  setEngineRules?(rules: EngineRules): Promise<EngineState>;
  /** Intervallo, soglie delle notifiche, lingua e tema salvati nella config. */
  getPreferences?(): Promise<AppPreferences>;
  /** Salva solo i campi presenti. */
  setPreferences?(prefs: Partial<AppPreferences>): Promise<AppPreferences>;
  /** Impostazioni dell'host: avvio al login (solo app desktop). */
  getAppSettings?(): Promise<HostSettings>;
  setAutostart?(enabled: boolean): Promise<HostSettings>;
  /** Cambi di account e decisioni del motore. */
  getHistoryEvents?(query?: HistoryQuery): Promise<HistoryEvents>;
  /** Selettori nativi: percorso scelto, `null` se annullato. */
  pickOpenFile?(options?: PickOptions): Promise<string | null>;
  pickSaveFile?(options?: PickOptions): Promise<string | null>;
  pickFolder?(options?: PickOptions): Promise<string | null>;
}
