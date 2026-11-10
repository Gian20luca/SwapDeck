/**
 * Messaggi tra il pannello (`MessageBackend`) e l'host (extension host di VS Code, plugin JetBrains).
 * Il pannello non esegue nulla: chiede, e l'host valida il metodo e lancia `swapdeck-cli`. Elenchi
 * e limiti stanno in `protocol.json`, letto anche dagli host che non sono in TypeScript.
 */
import type {
  AppPreferences,
  CswapAction,
  CswapError,
  EngineRules,
  ListResponse,
  ProfileCommand,
  ReloginEvent,
  SetupProgress,
  SuspendReason,
  UsageQuery,
} from '@swapdeck/types';
import protocol from '../protocol.json' with { type: 'json' };

/** Metodi del `Backend` inoltrati all'host (`requestMethods` di `protocol.json`). */
export const REQUEST_METHODS: readonly RequestMethod[] = protocol.requestMethods as RequestMethod[];

/** Evento di avanzamento di una richiesta lunga: setup di cswap o «Rifai login». */
export type ProgressEvent = SetupProgress | ReloginEvent;

/** Nomi dei metodi, dai parametri qui sotto; `protocol.test.ts` controlla che coincidano col JSON. */
export type RequestMethod = RequestParams['method'];

export interface HistoryParams {
  account?: number;
  sinceSeconds?: number;
}

/** Opzioni dei selettori nativi (`PickOptions` della UI). */
export interface PickParams {
  title?: string;
  defaultPath?: string;
  filters?: Array<{ name: string; extensions: string[] }>;
}

export type RequestParams =
  | { method: 'listAccounts'; params: { refresh?: boolean } }
  | { method: 'getStatus'; params: Record<string, never> }
  | { method: 'getConfig'; params: Record<string, never> }
  | { method: 'getConfigValue'; params: { key: string } }
  | { method: 'getHistory'; params: HistoryParams }
  | { method: 'confirmationFor'; params: { action: CswapAction } }
  | { method: 'runAction'; params: { action: CswapAction } }
  | {
      method:
        | 'listMappings'
        | 'getEngineState'
        | 'getEngineProfiles'
        | 'getPreferences'
        | 'getNotificationSettings';
      params: Record<string, never>;
    }
  | { method: 'setEngineRules'; params: { rules: EngineRules } }
  | { method: 'applyEngineProfile'; params: { command: ProfileCommand } }
  | { method: 'setNotificationSettings'; params: { patch: Record<string, unknown> } }
  | { method: 'setPreferences'; params: { prefs: Partial<AppPreferences> } }
  | { method: 'getHistoryEvents'; params: HistoryParams }
  | { method: 'getUsageBreakdown'; params: { query: UsageQuery } }
  | { method: 'pickOpenFile' | 'pickSaveFile' | 'pickFolder'; params: PickParams }
  | { method: 'diagnoseSetup' | 'installCswap' | 'upgradeCswap'; params: Record<string, never> }
  | { method: 'checkCswapUpdate'; params: { force?: boolean } }
  | { method: 'dismissCswapUpdate'; params: { version: string } }
  | { method: 'getDiagnostics' | 'openLogsFolder'; params: Record<string, never> }
  | { method: 'copyText'; params: { text: string } }
  | { method: 'reloginAccounts'; params: { list: ListResponse } }
  | { method: 'startRelogin'; params: { account: number } }
  | { method: 'submitReloginCode'; params: { code: string } }
  | { method: 'cancelRelogin'; params: Record<string, never> };

/** Webview → host. */
export type WebviewMessage = ({ type: 'request'; id: number } & RequestParams) | { type: 'ready' };

/** Impostazioni di presentazione decise dall'host (lingua di SwapDeck o dell'IDE, tema dell'IDE). */
export interface ViewSettings {
  locale: 'it' | 'en';
  theme: 'light' | 'dark';
}

/** `language` delle preferenze di SwapDeck, se valida. */
export function preferredLocaleOf(language: unknown): ViewSettings['locale'] | undefined {
  return language === 'it' || language === 'en' ? language : undefined;
}

/** Lingua delle viste: quella scelta in SwapDeck, altrimenti quella dell'IDE (`it…` o inglese). */
export function viewLocale(
  preferred: ViewSettings['locale'] | undefined,
  hostLanguage: string,
): ViewSettings['locale'] {
  return preferred ?? (hostLanguage.toLowerCase().startsWith('it') ? 'it' : 'en');
}

/** Motore in esecuzione sospeso (`swapdeck-cli state`): riga sopra le schede. */
export interface EngineSuspension {
  reason: SuspendReason;
  detail: string[];
}

/** Campi di `EngineStatus` (`swapdeck-cli state`) che servono alla riga «motore sospeso». */
export interface EngineSuspensionSource {
  suspended?: SuspendReason;
  suspendDetail?: string[];
}

/** Sospensione da mostrare, decisa dal Rust (`engine.suspended` c'è solo se chiede attenzione). */
export function suspensionOf(engine: EngineSuspensionSource | undefined): EngineSuspension | null {
  if (!engine?.suspended) return null;
  return { reason: engine.suspended, detail: engine.suspendDetail ?? [] };
}

/** Host → webview. */
export type HostMessage =
  | { type: 'response'; id: number; ok: true; result: unknown }
  | { type: 'response'; id: number; ok: false; error: CswapError }
  | { type: 'state'; list: ListResponse }
  /** Avanzamento di installazione o aggiornamento di cswap, o di «Rifai login», per la richiesta id. */
  | { type: 'progress'; id: number; event: ProgressEvent }
  | { type: 'view'; settings: ViewSettings }
  /** Stato del motore per la riga «motore sospeso» (`null` = niente da segnalare). */
  | { type: 'engine'; suspended: EngineSuspension | null }
  /** Config di SwapDeck o di cswap cambiata (altra finestra, app, CLI, altra vista): le schede rileggono. */
  | { type: 'configChanged' };

export function isRequestMethod(value: unknown): value is RequestMethod {
  return typeof value === 'string' && (REQUEST_METHODS as readonly string[]).includes(value);
}
