/**
 * Messaggi tra il webview (VsCodeBackend) e l'extension host. Il webview non esegue nulla: chiede,
 * e l'host valida il metodo e lancia `swapdeck-cli`.
 */
import type {
  AppPreferences,
  CswapAction,
  CswapError,
  EngineRules,
  ListResponse,
  ProfileCommand,
  SetupProgress,
  UsageQuery,
} from '@swapdeck/types';

/** Metodi del `Backend` inoltrati all'host. */
export const REQUEST_METHODS = [
  'listAccounts',
  'getStatus',
  'getConfig',
  'getConfigValue',
  'getHistory',
  'confirmationFor',
  'runAction',
  'listMappings',
  'getEngineState',
  'setEngineRules',
  'getEngineProfiles',
  'applyEngineProfile',
  'getNotificationSettings',
  'setNotificationSettings',
  'getPreferences',
  'setPreferences',
  'getHistoryEvents',
  'getUsageBreakdown',
  'pickOpenFile',
  'pickSaveFile',
  'pickFolder',
  'diagnoseSetup',
  'installCswap',
  'upgradeCswap',
  'checkCswapUpdate',
  'dismissCswapUpdate',
] as const;

export type RequestMethod = (typeof REQUEST_METHODS)[number];

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
  | { method: 'dismissCswapUpdate'; params: { version: string } };

/** Webview → host. */
export type WebviewMessage = ({ type: 'request'; id: number } & RequestParams) | { type: 'ready' };

/** Impostazioni di presentazione decise dall'host (lingua e tema di VS Code). */
export interface ViewSettings {
  locale: 'it' | 'en';
  theme: 'light' | 'dark';
}

/** Host → webview. */
export type HostMessage =
  | { type: 'response'; id: number; ok: true; result: unknown }
  | { type: 'response'; id: number; ok: false; error: CswapError }
  | { type: 'state'; list: ListResponse }
  /** Avanzamento di installazione o aggiornamento di cswap per la richiesta id. */
  | { type: 'progress'; id: number; event: SetupProgress }
  | { type: 'view'; settings: ViewSettings }
  /** Config di SwapDeck o di cswap cambiata (altra finestra, app, CLI, altra vista): le schede rileggono. */
  | { type: 'configChanged' };

export function isRequestMethod(value: unknown): value is RequestMethod {
  return typeof value === 'string' && (REQUEST_METHODS as readonly string[]).includes(value);
}
