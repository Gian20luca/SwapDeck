import type {
  ConfigGetResponse,
  ConfigListResponse,
  Confirmation,
  CswapAction,
  AppPreferences,
  CswapError,
  DirectoryMapping,
  EngineProfiles,
  EngineRules,
  EngineState,
  EngineView,
  HistoryEvents,
  ListResponse,
  NotificationConfig,
  ProfileCommand,
  SetupDiagnosis,
  SetupOutcome,
  SetupProgress,
  StatusResponse,
  SwitchResponse,
  UpdateInfo,
  UsageBreakdown,
  UsageQuery,
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
 * Formati del Rust (`swapdeck-core`, generati in `@swapdeck/types`): cartelle mappate, stato del
 * motore, preferenze e eventi dello storico.
 */
export type {
  AppPreferences,
  DirectoryMapping,
  EngineProfiles,
  EngineState,
  HistoryEvents,
  ProfileCommand,
};
/** Stato del motore all'ultimo giro (`EngineView` del Rust). */
export type EngineViewInfo = EngineView;

/** Patch delle notifiche: per ogni avviso solo i campi che cambiano. */
export type NotificationPatch = {
  [K in Exclude<keyof NotificationConfig, 'thresholds'>]?: Partial<NotificationConfig[K]>;
};
export type { NotificationConfig };

/** Impostazioni dell'host desktop (avvio al login, versione). */
export interface HostSettings {
  autostart: boolean;
  /** `false` nelle build di sviluppo: il sistema non viene modificato. */
  autostartManaged: boolean;
  version: string;
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
  /** Mappe (profili delle regole) e quella attiva. */
  getEngineProfiles?(): Promise<EngineProfiles>;
  /** Crea, rinomina, duplica, elimina, ripristina o seleziona una mappa; stato del motore aggiornato. */
  applyEngineProfile?(command: ProfileCommand): Promise<EngineState>;
  /** Regole delle notifiche (F18). */
  getNotificationSettings?(): Promise<NotificationConfig>;
  /** Merge patch delle regole (validata dal Rust); restituisce le regole salvate. */
  setNotificationSettings?(patch: NotificationPatch): Promise<NotificationConfig>;
  /** Intervallo, soglie delle notifiche, lingua e tema salvati nella config. */
  getPreferences?(): Promise<AppPreferences>;
  /** Salva solo i campi presenti. */
  setPreferences?(prefs: Partial<AppPreferences>): Promise<AppPreferences>;
  /** Impostazioni dell'host: avvio al login (solo app desktop). */
  getAppSettings?(): Promise<HostSettings>;
  setAutostart?(enabled: boolean): Promise<HostSettings>;
  /** Cambi di account e decisioni del motore. */
  getHistoryEvents?(query?: HistoryQuery): Promise<HistoryEvents>;
  /**
   * Chi consuma (F20): consumi locali di Claude Code per progetto, sessione, agente o modello, con
   * la stima degli altri dispositivi. La prima lettura può durare decine di secondi.
   */
  getUsageBreakdown?(query: UsageQuery): Promise<UsageBreakdown>;
  /** Selettori nativi: percorso scelto, `null` se annullato. */
  /** Testo di diagnostica già redatto (versioni, sistema, motore, mappa, ultime righe di log). */
  getDiagnostics?(): Promise<string>;
  /** Apre la cartella dei log nel file manager. */
  openLogsFolder?(): Promise<void>;
  /** Copia negli appunti dall'host (VS Code); senza, la pagina usa `navigator.clipboard`. */
  copyText?(text: string): Promise<void>;
  pickOpenFile?(options?: PickOptions): Promise<string | null>;
  pickSaveFile?(options?: PickOptions): Promise<string | null>;
  pickFolder?(options?: PickOptions): Promise<string | null>;
  /**
   * Controllo della finestra del mini-widget (solo app desktop): senza, `MiniWidget` non mostra i
   * pulsanti «Nascondi» e «Comprimi» (per esempio nel webview di VS Code).
   */
  readonly miniWindow?: MiniWindowControls;
  /**
   * Tema della cornice nativa della finestra (barra del titolo): `'light'`, `'dark'` o `null` per
   * seguire il sistema. Solo app desktop; `WindowTheme` lo chiama a ogni cambio di tema.
   */
  setWindowTheme?(theme: 'light' | 'dark' | null): Promise<void>;
}

/** Finestra del mini-widget gestita dall'host: nascondere, ridurre a striscia, ricordare lo stato. */
export interface MiniWindowControls {
  /** Nasconde la finestra; l'app resta attiva e il mini-widget si riapre dal menu dell'icona. */
  hide(): Promise<void>;
  /** Riduce a una striscia sottile o riespande; l'host ricorda la scelta per il prossimo avvio. */
  setCompact(compact: boolean): Promise<void>;
  /** Stato salvato: `true` se il mini-widget è ridotto a striscia. */
  isCompact(): Promise<boolean>;
  /** Altezza (px logici) che serve al contenuto: l'host adatta la finestra estesa, senza farla uscire dallo schermo. */
  fitHeight?(height: number): Promise<void>;
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
