/** Richieste del webview tradotte in comandi di `swapdeck-cli` (lato extension host). */
import type {
  ActionOutcome,
  Confirmation,
  CswapAction,
  HistoryRecord,
  ListResponse,
} from '@swapdeck/types';
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
  /** Conferma chiesta dall'host (dialoghi modali di VS Code); `false` = annullata. */
  confirm: (
    action: CswapAction,
    needed: Exclude<Confirmation, { kind: 'none' }>,
  ) => Promise<boolean>;
  now?: () => number;
}

/** Tutti i `kind` di `CswapAction`: un `kind` fuori elenco è `invalidUsage` già nell'host. */
export const ACTION_KINDS = [
  'switch',
  'add',
  'addToken',
  'remove',
  'disable',
  'enable',
  'setAlias',
  'swap',
  'move',
  'map',
  'unmap',
  'configSet',
  'configUnset',
  'export',
  'import',
  'unclaimedPurge',
  'purge',
  'upgrade',
  'run',
  'autoStart',
  'autoStop',
] as const satisfies ReadonlyArray<CswapAction['kind']>;

// Errore di compilazione se il Rust aggiunge un `kind` che manca qui.
type MissingKind = Exclude<CswapAction['kind'], (typeof ACTION_KINDS)[number]>;
const allKindsListed: [MissingKind] extends [never] ? true : MissingKind = true;
void allKindsListed;

/**
 * Argomenti di `claude` ammessi per `run` dal webview: solo flag senza valore e senza effetti sui
 * permessi (il comando si apre in un terminale dell'utente).
 */
export const SAFE_CLAUDE_ARGS: ReadonlySet<string> = new Set(['--continue', '--verbose']);

function invalid(message: string): CliError {
  return new CliError({ kind: 'invalidUsage', message });
}

/** Azione con `kind` noto; per `run` solo argomenti di `claude` dell'elenco chiuso. */
function checkedAction(value: unknown): CswapAction {
  if (typeof value !== 'object' || value === null) throw invalid('azione non valida');
  const kind = (value as { kind?: unknown }).kind;
  if (typeof kind !== 'string' || !(ACTION_KINDS as readonly string[]).includes(kind)) {
    throw invalid('azione sconosciuta');
  }
  if (kind === 'run') {
    const options = (value as { options?: unknown }).options;
    const args =
      typeof options === 'object' && options !== null
        ? (options as { claudeArgs?: unknown }).claudeArgs
        : undefined;
    if (
      args !== undefined &&
      (!Array.isArray(args) || !args.every((a) => typeof a === 'string' && SAFE_CLAUDE_ARGS.has(a)))
    ) {
      throw invalid('argomenti di claude non ammessi');
    }
  }
  return value as CswapAction;
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
      case 'confirmationFor':
        return confirmationFor(checkedAction(params['action']));
      case 'runAction': {
        const action = checkedAction(params['action']);
        // La conferma la impone l'host: il webview non è fidato.
        const needed = await confirmationFor(action);
        if (needed.kind !== 'none' && !(await deps.confirm(action, needed))) {
          throw new CliError({ kind: 'cancelled', message: 'azione annullata' });
        }
        const outcome = await deps.cli<ActionOutcome>(['action', '-'], {
          stdin: JSON.stringify(action),
          timeoutMs: ACTION_TIMEOUT_MS,
        });
        await deps.onOutcome(outcome);
        return outcome;
      }
    }
  }

  function confirmationFor(action: CswapAction): Promise<Confirmation> {
    const list = deps.lastState()?.list;
    const args = ['confirm', '-'];
    if (list) args.push('--active', String(list.activeAccountNumber ?? 'none'));
    return deps.cli<Confirmation>(args, { stdin: JSON.stringify(action) });
  }
}

/** Descrizione breve dell'azione per il dialogo di conferma (mai il token di `addToken`). */
export function describeAction(action: CswapAction): string {
  switch (action.kind) {
    case 'remove':
    case 'disable':
    case 'enable':
    case 'setAlias':
      return `${action.kind} #${action.account}`;
    case 'swap':
      return `swap #${action.a} ↔ #${action.b}`;
    case 'move':
      return `move #${action.account} → ${action.slot}`;
    case 'map':
      return `map #${action.account} ${action.path}`;
    case 'unmap':
    case 'import':
      return `${action.kind} ${action.path}`;
    case 'export':
      return `export${action.account === undefined ? '' : ` #${action.account}`} → ${action.path}`;
    case 'unclaimedPurge':
      return `unclaimed --purge ${action.id}`;
    case 'configSet':
    case 'configUnset':
      return `${action.kind} ${action.key}`;
    default:
      return action.kind;
  }
}
