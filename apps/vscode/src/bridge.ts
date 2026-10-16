/** Richieste del webview tradotte in comandi di `swapdeck-cli` (lato extension host). */
import type { ActionOutcome, CswapAction, HistoryRecord, ListResponse } from '@swapdeck/types';
import type { HistoryPoint } from '@swapdeck/ui';
import { ACTION_TIMEOUT_MS, CliError, type CliRunner } from './cli.js';
import type { CliState } from './generated/index.js';
import { isRequestMethod, type RequestMethod } from './protocol.js';

const DAY_SECONDS = 24 * 3600;
const DEFAULT_HISTORY_SECONDS = 7 * DAY_SECONDS;
const CONFIG_KEY = /^[A-Za-z0-9_.-]{1,100}$/;

export interface BridgeDeps {
  cli: CliRunner;
  /** Ultimo stato letto, se c'è. */
  lastState: () => CliState | undefined;
  /** Nuova lettura (condivisa con il ciclo di aggiornamento). */
  refresh: () => Promise<CliState>;
  /** Dopo un'azione riuscita: nuova lista, terminale per `run`, avvisi. */
  onOutcome: (outcome: ActionOutcome) => Promise<void> | void;
  now?: () => number;
}

function invalid(message: string): CliError {
  return new CliError({ kind: 'invalidUsage', message });
}

function isAction(value: unknown): value is CswapAction {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { kind?: unknown }).kind === 'string'
  );
}

function optionalInt(value: unknown, name: string): number | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) {
    throw invalid(`${name} non valido`);
  }
  return value;
}

/** Campioni dello storico nel formato della UI (`HistoryPoint`). */
export function toHistoryPoints(records: HistoryRecord[]): HistoryPoint[] {
  const points: HistoryPoint[] = [];
  for (const r of records) {
    if (r.type !== 'sample') continue;
    points.push({
      ts: r.ts,
      account: r.account,
      fiveHourPct: r.fiveHourPct,
      sevenDayPct: r.sevenDayPct,
      fiveHourExpectedPct: null,
      sevenDayExpectedPct: r.sevenDayExpectedPct,
    });
  }
  return points;
}

function stateList(state: CliState): ListResponse {
  if (state.list) return state.list;
  throw new CliError(state.error ?? { kind: 'failed', message: 'lista non disponibile' });
}

/** Gestore delle richieste: valida metodo e parametri, poi chiama la CLI. */
export function createRequestHandler(deps: BridgeDeps) {
  const now = deps.now ?? (() => Date.now());
  return async (method: unknown, rawParams: unknown): Promise<unknown> => {
    if (!isRequestMethod(method)) throw invalid('metodo sconosciuto');
    const params = (typeof rawParams === 'object' && rawParams !== null ? rawParams : {}) as Record<
      string,
      unknown
    >;
    return handle(method, params);
  };

  async function handle(method: RequestMethod, params: Record<string, unknown>): Promise<unknown> {
    switch (method) {
      case 'listAccounts': {
        const last = deps.lastState();
        if (params['refresh'] !== true && last?.list) return last.list;
        return stateList(await deps.refresh());
      }
      case 'getStatus':
        return deps.cli(['status']);
      case 'getConfig':
        return deps.cli(['cswap-config']);
      case 'getConfigValue': {
        const key = params['key'];
        if (typeof key !== 'string' || !CONFIG_KEY.test(key)) throw invalid('chiave non valida');
        return deps.cli(['cswap-config', key]);
      }
      case 'getHistory': {
        const account = optionalInt(params['account'], 'account');
        const nowSec = Math.floor(now() / 1000);
        const since = optionalInt(params['sinceSeconds'], 'sinceSeconds');
        const args = ['history', '--from', String(since ?? nowSec - DEFAULT_HISTORY_SECONDS)];
        args.push('--to', String(nowSec));
        if (account !== undefined) args.push('--account', String(account));
        return toHistoryPoints(await deps.cli<HistoryRecord[]>(args));
      }
      case 'confirmationFor': {
        const action = params['action'];
        if (!isAction(action)) throw invalid('azione non valida');
        const list = deps.lastState()?.list;
        const args = ['confirm', '-'];
        if (list) args.push('--active', String(list.activeAccountNumber ?? 'none'));
        return deps.cli(args, { stdin: JSON.stringify(action) });
      }
      case 'runAction': {
        const action = params['action'];
        if (!isAction(action)) throw invalid('azione non valida');
        const outcome = await deps.cli<ActionOutcome>(['action', '-'], {
          stdin: JSON.stringify(action),
          timeoutMs: ACTION_TIMEOUT_MS,
        });
        await deps.onOutcome(outcome);
        return outcome;
      }
    }
  }
}
