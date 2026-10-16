/** Richieste del webview tradotte in comandi di `swapdeck-cli` (lato extension host). */
import type {
  ActionOutcome,
  Confirmation,
  CswapAction,
  HistoryRecord,
  ListResponse,
  SetupProgress,
} from '@swapdeck/types';
import type { HistoryPoint } from '@swapdeck/ui';
import { ACTION_TIMEOUT_MS, CliError, SETUP_TIMEOUT_MS, type CliRunner } from './cli.js';
import type { CliState } from './generated/index.js';
import { isRequestMethod, type PickParams, type RequestMethod } from './protocol.js';

const DAY_SECONDS = 24 * 3600;
const DEFAULT_HISTORY_SECONDS = 7 * DAY_SECONDS;
const CONFIG_KEY = /^[A-Za-z0-9_.-]{1,100}$/;
/** Versione di claude-swap (PEP 440, solo caratteri sicuri). */
const VERSION = /^[0-9][0-9A-Za-z.+-]{0,63}$/;

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
  /** Selettori nativi di file e cartelle; assente = metodo non disponibile. */
  pick?: (kind: 'openFile' | 'saveFile' | 'folder', options: PickParams) => Promise<string | null>;
  /** Conferma dell'host per installare o aggiornare cswap; assente = metodi non disponibili. */
  confirmSetup?: (kind: 'install' | 'upgrade') => Promise<boolean>;
  /**
   * Prima dell'aggiornamento: ferma il motore dell'estensione (il suo `cswap auto` bloccherebbe i
   * file) e restituisce la funzione che lo riavvia alla fine, riuscita o no.
   */
  pauseEngine?: () => Promise<() => void>;
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

/** Opzioni del selettore: solo testi, estensioni semplici; il webview non è fidato. */
export function checkedPick(params: Record<string, unknown>): PickParams {
  const out: PickParams = {};
  for (const key of ['title', 'defaultPath'] as const) {
    const value = params[key];
    if (value === undefined) continue;
    if (typeof value !== 'string' || value.length > 1000) throw invalid(`${key} non valido`);
    out[key] = value;
  }
  const filters = params['filters'];
  if (filters !== undefined) {
    if (!Array.isArray(filters) || filters.length > 10) throw invalid('filtri non validi');
    out.filters = filters.map((f: unknown) => {
      const name = (f as { name?: unknown } | null)?.name;
      const extensions = (f as { extensions?: unknown } | null)?.extensions;
      if (
        typeof name !== 'string' ||
        name.length > 100 ||
        !Array.isArray(extensions) ||
        !extensions.every((e) => typeof e === 'string' && /^[A-Za-z0-9]{1,10}$/.test(e))
      ) {
        throw invalid('filtri non validi');
      }
      return { name, extensions: extensions as string[] };
    });
  }
  return out;
}

/** Oggetto JSON semplice (la validazione completa la fa il Rust). */
function plainObject(value: unknown, name: string): Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw invalid(`${name} non valide`);
  }
  return value as Record<string, unknown>;
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
  return async (
    method: unknown,
    rawParams: unknown,
    progress?: (event: SetupProgress) => void,
  ): Promise<unknown> => {
    if (!isRequestMethod(method)) throw invalid('metodo sconosciuto');
    const params = (typeof rawParams === 'object' && rawParams !== null ? rawParams : {}) as Record<
      string,
      unknown
    >;
    return handle(method, params, progress ?? (() => undefined));
  };

  /** Installazione o aggiornamento: conferma dell'host, eventi in streaming, poi nuova lettura. */
  async function setupRun(
    kind: 'install' | 'upgrade',
    progress: (event: SetupProgress) => void,
  ): Promise<unknown> {
    if (!deps.confirmSetup) throw invalid('installazione non disponibile');
    if (!(await deps.confirmSetup(kind))) {
      throw new CliError({ kind: 'cancelled', message: 'operazione annullata' });
    }
    const resume = kind === 'upgrade' && deps.pauseEngine ? await deps.pauseEngine() : undefined;
    try {
      return await deps.cli(['setup', kind], {
        timeoutMs: SETUP_TIMEOUT_MS,
        onEvent: (event) => progress(event as SetupProgress),
      });
    } finally {
      resume?.();
      void deps.refresh().catch(() => undefined);
    }
  }

  async function handle(
    method: RequestMethod,
    params: Record<string, unknown>,
    progress: (event: SetupProgress) => void,
  ): Promise<unknown> {
    switch (method) {
      case 'diagnoseSetup':
        return deps.cli(['setup', 'diagnose']);
      case 'installCswap':
        return setupRun('install', progress);
      case 'upgradeCswap':
        return setupRun('upgrade', progress);
      case 'checkCswapUpdate':
        return deps.cli(
          params['force'] === true
            ? ['setup', 'check-update', '--force']
            : ['setup', 'check-update'],
        );
      case 'dismissCswapUpdate': {
        const version = params['version'];
        if (typeof version !== 'string' || !VERSION.test(version)) {
          throw invalid('versione non valida');
        }
        await deps.cli(['setup', 'dismiss-update', version]);
        return undefined;
      }
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
      case 'listMappings':
        return deps.cli(['mappings']);
      case 'getEngineState':
        return deps.cli(['engine', 'state']);
      case 'setEngineRules': {
        const rules = plainObject(params['rules'], 'regole');
        return deps.cli(['engine', 'rules', 'set', '-'], { stdin: JSON.stringify(rules) });
      }
      case 'getEngineProfiles':
        return deps.cli(['engine', 'profiles']);
      case 'applyEngineProfile': {
        const command = plainObject(params['command'], 'mappa');
        return deps.cli(['engine', 'profiles', '-'], { stdin: JSON.stringify(command) });
      }
      case 'getPreferences':
        return deps.cli(['preferences', 'get']);
      case 'setPreferences': {
        const prefs = plainObject(params['prefs'], 'preferenze');
        return deps.cli(['preferences', 'set', '-'], { stdin: JSON.stringify(prefs) });
      }
      case 'getHistoryEvents': {
        const args = ['history', 'events'];
        const since = optionalInt(params['sinceSeconds'], 'sinceSeconds');
        const account = optionalInt(params['account'], 'account');
        if (since !== undefined) args.push('--since', String(since));
        if (account !== undefined) args.push('--account', String(account));
        return deps.cli(args);
      }
      case 'pickOpenFile':
      case 'pickSaveFile':
      case 'pickFolder': {
        if (!deps.pick) throw invalid('selettore non disponibile');
        const kind =
          method === 'pickOpenFile'
            ? 'openFile'
            : method === 'pickSaveFile'
              ? 'saveFile'
              : 'folder';
        return deps.pick(kind, checkedPick(params));
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
