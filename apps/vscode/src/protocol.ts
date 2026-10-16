/**
 * Messaggi tra il webview (VsCodeBackend) e l'extension host. Il webview non esegue nulla: chiede,
 * e l'host valida il metodo e lancia `swapdeck-cli`.
 */
import type { CswapAction, CswapError, ListResponse } from '@swapdeck/types';

/** Metodi del `Backend` inoltrati all'host. */
export const REQUEST_METHODS = [
  'listAccounts',
  'getStatus',
  'getConfig',
  'getConfigValue',
  'getHistory',
  'confirmationFor',
  'runAction',
  'pickOpenFile',
  'pickSaveFile',
  'pickFolder',
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
  | { method: 'pickOpenFile' | 'pickSaveFile' | 'pickFolder'; params: PickParams };

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
  | { type: 'view'; settings: ViewSettings };

export function isRequestMethod(value: unknown): value is RequestMethod {
  return typeof value === 'string' && (REQUEST_METHODS as readonly string[]).includes(value);
}
