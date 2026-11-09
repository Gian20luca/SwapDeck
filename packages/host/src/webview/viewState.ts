/**
 * Stato del pannello che l'host conserva tra un caricamento e l'altro (`getState`/`setState` in VS
 * Code, `localStorage` in JetBrains): oggi solo la scheda aperta.
 */
import type { TabId } from '@swapdeck/ui';

export interface WebviewState {
  tab?: TabId;
}

const TABS: readonly TabId[] = ['accounts', 'folders', 'rules', 'history', 'tools', 'settings'];

/** Stato riletto, controllato: un valore estraneo vale come assente. */
export function readState(raw: unknown): WebviewState {
  if (typeof raw !== 'object' || raw === null) return {};
  const tab = (raw as { tab?: unknown }).tab;
  return typeof tab === 'string' && (TABS as readonly string[]).includes(tab)
    ? { tab: tab as TabId }
    : {};
}
