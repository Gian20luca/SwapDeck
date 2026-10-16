/** Supporto ai test: fixture di cswap e traduzione che sostituisce solo i segnaposto. */
import { readFileSync } from 'node:fs';
import type { ListResponse } from '@swapdeck/types';
import type { CliState } from './generated/index.js';

export function fixtureList(name = 'list.json'): ListResponse {
  const url = new URL(`../../../fixtures/cswap/${name}`, import.meta.url);
  return JSON.parse(readFileSync(url, 'utf8')) as ListResponse;
}

export function cliState(list: ListResponse | null, running = false): CliState {
  return {
    list,
    error: list ? null : { kind: 'notInstalled', message: 'cswap non trovato' },
    updatedAtMs: 0,
    engine: { enabled: false, dryRun: false, pollIntervalSeconds: 60, running, lock: null },
  };
}

export const fmt = (message: string, ...args: Array<string | number>) =>
  message.replace(/\{(\d+)\}/g, (_, i: string) => String(args[Number(i)]));

/** Copia profonda con l'uso dell'account attivo cambiato. */
export function withActiveUsage(list: ListResponse, five: number, seven: number): ListResponse {
  const copy = structuredClone(list);
  const active = copy.accounts.find((a) => a.active)!;
  active.usage = { ...active.usage, fiveHour: { pct: five }, sevenDay: { pct: seven } };
  return copy;
}

export function withActive(list: ListResponse, number: number): ListResponse {
  const copy = structuredClone(list);
  for (const a of copy.accounts) a.active = a.number === number;
  copy.activeAccountNumber = number;
  return copy;
}
