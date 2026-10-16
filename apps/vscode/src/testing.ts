/** Supporto ai test: fixture di cswap e traduzione che sostituisce solo i segnaposto. */
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
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

/** Processi figli nei test: lenti con la copertura attiva o la macchina carica. */
export const PROCESS_TEST_TIMEOUT_MS = 30_000;

/**
 * Ambiente per `node <argomenti>` con uno script precaricato che fa da processo finto
 * (`swapdeck-cli` o motore). Il figlio non raccoglie copertura.
 */
export function preloadEnv(dir: string, code: string): NodeJS.ProcessEnv {
  const file = join(dir, `pre-${Math.random().toString(36).slice(2)}.cjs`);
  writeFileSync(file, code);
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    NODE_OPTIONS: `--require "${file.replaceAll('\\', '/')}"`,
  };
  delete env['NODE_V8_COVERAGE'];
  return env;
}
