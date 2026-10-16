import type {
  Account,
  ConfigGetResponse,
  ConfigListResponse,
  Confirmation,
  CswapAction,
  ListResponse,
  SetupDiagnosis,
  SetupOutcome,
  SetupProgress,
  StatusResponse,
  SwitchReason,
  SwitchResponse,
  UpdateInfo,
} from '@swapdeck/types';
import {
  BackendError,
  type ActionOutcome,
  type Backend,
  type HistoryPoint,
  type HistoryQuery,
  type Unsubscribe,
} from '../backend/types.js';
import configList from '../../../../fixtures/cswap/config-list.json';
import { loadScenario, type ScenarioName } from './fixtures.js';
import { FakeSetup, type FakeSetupOptions } from './fakeSetup.js';
import { generateHistory } from './history.js';

export interface FakeBackendOptions {
  scenario?: ScenarioName;
  /** Ritardo simulato di ogni chiamata, in ms (default 0: i test restano veloci). */
  latencyMs?: number;
  /** Orologio iniettabile per i test. */
  now?: () => number;
  /** Stato simulato di cswap e uv per la procedura di primo avvio e l'aggiornamento. */
  setup?: FakeSetupOptions;
}

/** Soglia oltre cui un account conta come «al limite» per `best` e `next-available`. */
const LIMIT_PCT = 95;
/** Parola della doppia conferma di `purge` (specchio di `PURGE_CONFIRM_WORD` del Rust). */
const PURGE_WORD = 'purge';

function fail(message: string, errorType: string): never {
  throw new BackendError({ kind: 'failed', message, exitCode: 1, errorType });
}

function isDisabled(account: Account): boolean {
  return account.disabled === true;
}

function atLimit(account: Account): boolean {
  const usage = account.usage;
  if (!usage) return true;
  return (usage.fiveHour?.pct ?? 0) >= LIMIT_PCT || (usage.sevenDay?.pct ?? 0) >= LIMIT_PCT;
}

function usable(account: Account): boolean {
  return !isDisabled(account) && account.usageStatus === 'ok' && !atLimit(account);
}

/**
 * Backend finto per sviluppo e test: legge le fixture di `fixtures/cswap/`, simula i cambi di account
 * e genera uno storico sintetico di 7 giorni. Non parla con cswap né con il disco.
 */
export class FakeBackend implements Backend {
  private list: ListResponse;
  private readonly listeners = new Set<(state: ListResponse) => void>();
  private readonly latencyMs: number;
  private readonly now: () => number;
  private config: ConfigListResponse;
  /** Contatore delle chiamate a `listAccounts`, utile ai test. */
  listCalls = 0;
  /** Azioni ricevute da `runAction`, utili ai test. */
  readonly actionLog: CswapAction[] = [];
  /** Simulazione di diagnosi, installazione e aggiornamento (`calls` per i test). */
  readonly setup: FakeSetup;

  constructor(options: FakeBackendOptions = {}) {
    this.latencyMs = options.latencyMs ?? 0;
    this.now = options.now ?? Date.now;
    this.list = loadScenario(options.scenario ?? 'default', this.now());
    this.config = structuredClone(configList) as unknown as ConfigListResponse;
    this.setup = new FakeSetup(
      options.setup ?? {},
      () => this.list.accounts.length,
      () => this.delay(),
    );
  }

  private async delay(): Promise<void> {
    if (this.latencyMs > 0) await new Promise((resolve) => setTimeout(resolve, this.latencyMs));
  }

  private snapshot(): ListResponse {
    return structuredClone(this.list);
  }

  private emit(): void {
    const state = this.snapshot();
    for (const listener of this.listeners) listener(state);
  }

  private find(number: number): Account | undefined {
    return this.list.accounts.find((a) => a.number === number);
  }

  async listAccounts(_options?: { refresh?: boolean }): Promise<ListResponse> {
    this.listCalls += 1;
    await this.delay();
    return this.snapshot();
  }

  async getStatus(): Promise<StatusResponse> {
    await this.delay();
    const active = this.list.accounts.find((a) => a.active);
    if (!active) return { schemaVersion: 1, active: null };
    const { active: _active, disabled: _disabled, ...rest } = structuredClone(active);
    return {
      schemaVersion: 1,
      active: { ...rest, managed: true },
      totalManagedAccounts: this.list.accounts.length,
    };
  }

  async getConfig(): Promise<ConfigListResponse> {
    await this.delay();
    return structuredClone(this.config);
  }

  async getConfigValue(key: string): Promise<ConfigGetResponse> {
    await this.delay();
    const found = this.config.settings.find((s) => s.key === key);
    if (!found) fail(`unknown setting '${key}'`, 'ConfigError');
    return { schemaVersion: 1, key: found.key, value: found.value, isSet: found.isSet };
  }

  async getHistory(query: HistoryQuery = {}): Promise<HistoryPoint[]> {
    await this.delay();
    const points = generateHistory(this.list, this.now(), query.sinceSeconds);
    return query.account === undefined ? points : points.filter((p) => p.account === query.account);
  }

  /** Specchio semplificato di `CswapAction::confirmation` del Rust (nel backend reale decide il Rust). */
  async confirmationFor(action: CswapAction): Promise<Confirmation> {
    switch (action.kind) {
      case 'purge':
        return { kind: 'typed', word: PURGE_WORD };
      case 'remove':
      case 'import':
      case 'export':
      case 'swap':
      case 'move':
      case 'unmap':
      case 'unclaimedPurge':
      case 'upgrade':
        return { kind: 'simple' };
      case 'disable':
        return this.find(action.account)?.active ? { kind: 'simple' } : { kind: 'none' };
      default:
        return { kind: 'none' };
    }
  }

  async runAction(action: CswapAction): Promise<ActionOutcome> {
    this.actionLog.push(action);
    await this.delay();
    switch (action.kind) {
      case 'switch':
        return this.doSwitch(action);
      case 'disable':
      case 'enable': {
        const account =
          this.find(action.account) ??
          fail(`Account-${action.account} does not exist`, 'AccountNotFoundError');
        if (action.kind === 'disable') account.disabled = true;
        else delete account.disabled;
        this.emit();
        return {};
      }
      case 'setAlias': {
        const account =
          this.find(action.account) ??
          fail(`Account-${action.account} does not exist`, 'AccountNotFoundError');
        if (action.alias === null) delete account.alias;
        else account.alias = action.alias;
        this.emit();
        return {};
      }
      case 'remove': {
        const before = this.list.accounts.length;
        this.list.accounts = this.list.accounts.filter((a) => a.number !== action.account);
        if (this.list.accounts.length === before)
          fail(`Account-${action.account} does not exist`, 'AccountNotFoundError');
        if (this.list.activeAccountNumber === action.account) this.list.activeAccountNumber = null;
        this.emit();
        return {};
      }
      case 'swap': {
        const a =
          this.find(action.a) ?? fail(`Account-${action.a} does not exist`, 'AccountNotFoundError');
        const b =
          this.find(action.b) ?? fail(`Account-${action.b} does not exist`, 'AccountNotFoundError');
        [a.number, b.number] = [b.number, a.number];
        if (this.list.activeAccountNumber === action.a) this.list.activeAccountNumber = action.b;
        else if (this.list.activeAccountNumber === action.b)
          this.list.activeAccountNumber = action.a;
        this.list.accounts.sort((x, y) => x.number - y.number);
        this.emit();
        return {};
      }
      case 'move': {
        const account =
          this.find(action.account) ??
          fail(`Account-${action.account} does not exist`, 'AccountNotFoundError');
        if (this.find(action.slot)) fail(`Slot ${action.slot} is already used`, 'ConfigError');
        if (this.list.activeAccountNumber === account.number)
          this.list.activeAccountNumber = action.slot;
        account.number = action.slot;
        this.list.accounts.sort((x, y) => x.number - y.number);
        this.emit();
        return {};
      }
      case 'configSet': {
        const setting = this.config.settings.find((s) => s.key === action.key);
        if (!setting) fail(`unknown setting '${action.key}'`, 'ConfigError');
        setting.value = Number.isNaN(Number(action.value)) ? action.value : Number(action.value);
        setting.isSet = true;
        return {};
      }
      default:
        // Le altre azioni (add, export, run, ...) non hanno effetti simulati.
        return { message: 'Simulato dal FakeBackend' };
    }
  }

  diagnoseSetup(): Promise<SetupDiagnosis> {
    return this.setup.diagnose();
  }

  installCswap(onProgress: (event: SetupProgress) => void): Promise<SetupOutcome> {
    return this.setup.install(onProgress);
  }

  checkCswapUpdate(_options?: { force?: boolean }): Promise<UpdateInfo> {
    return this.setup.checkUpdate();
  }

  dismissCswapUpdate(version: string): Promise<void> {
    return this.setup.dismiss(version);
  }

  upgradeCswap(onProgress: (event: SetupProgress) => void): Promise<SetupOutcome> {
    return this.setup.upgrade(onProgress);
  }

  onStateChanged(listener: (state: ListResponse) => void): Unsubscribe {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /** Simula un aggiornamento del motore: l'account attivo consuma un po' e il dato diventa fresco. */
  tick(): void {
    const active = this.list.accounts.find((a) => a.active);
    if (active?.usage) {
      const bump = (w: { pct: number } | undefined, by: number) => {
        if (w) w.pct = Math.min(100, Math.round((w.pct + by) * 10) / 10);
      };
      bump(active.usage.fiveHour, 1.5);
      bump(active.usage.sevenDay, 0.4);
    }
    for (const account of this.list.accounts) {
      if (account.usage) {
        account.usageFetchedAt = new Date(this.now()).toISOString();
        account.usageAgeSeconds = 0;
      }
    }
    this.emit();
  }

  private doSwitch(action: Extract<CswapAction, { kind: 'switch' }>): ActionOutcome {
    const accounts = this.list.accounts;
    const current = accounts.find((a) => a.active) ?? null;
    const from = current ? { number: current.number, email: current.email } : null;
    const response = (
      to: Account | null,
      switched: boolean,
      reason: SwitchReason,
      strategy: SwitchResponse['strategy'],
      message: string,
      warnings: string[] = [],
    ): ActionOutcome => {
      if (switched && to) {
        for (const a of accounts) a.active = a.number === to.number;
        this.list.activeAccountNumber = to.number;
        this.emit();
      }
      return {
        switch: {
          schemaVersion: 1,
          switched,
          from,
          to: to ? { number: to.number, email: to.email } : null,
          strategy,
          reason,
          message,
          warnings,
        },
      };
    };

    if (action.target !== undefined) {
      const target =
        accounts.find((a) => a.number === action.target) ??
        fail(`No account found with identifier: ${action.target}`, 'AccountNotFoundError');
      if (target.active) {
        return response(
          target,
          false,
          'already-active',
          'direct',
          `Account-${target.number} is already active`,
        );
      }
      return response(
        target,
        true,
        'switched',
        'direct',
        `Switched to Account-${target.number} (${target.email})`,
      );
    }

    const candidates = accounts.filter((a) => !a.active);
    const warnings = accounts
      .filter((a) => !a.active && isDisabled(a))
      .map((a) => `Skipped Account-${a.number} (disabled)`);

    if (action.strategy === 'best') {
      const pool = candidates.filter(usable);
      if (pool.length === 0) {
        return response(
          null,
          false,
          'candidates-exhausted',
          'best',
          'All other accounts are at their limit',
          warnings,
        );
      }
      const rank = (a: Account) => a.usage?.sevenDay?.pct ?? 0;
      const best = pool.reduce((m, a) => (rank(a) < rank(m) ? a : m));
      if (current && usable(current) && rank(current) <= rank(best)) {
        return response(
          current,
          false,
          'already-best',
          'best',
          `Account-${current.number} is already the best`,
          warnings,
        );
      }
      return response(
        best,
        true,
        'switched',
        'best',
        `Switched to Account-${best.number} (${best.email})`,
        warnings,
      );
    }

    // `next-available` e rotazione semplice: primo account utile dopo quello attivo, in cerchio.
    const ordered = [...accounts].sort((a, b) => a.number - b.number);
    const start = Math.max(
      0,
      ordered.findIndex((a) => a.active),
    );
    const rotated = [...ordered.slice(start + 1), ...ordered.slice(0, start)];
    const strategy = action.strategy === 'next-available' ? 'next-available' : 'rotation';
    const next = rotated.find((a) => (strategy === 'rotation' ? !isDisabled(a) : usable(a)));
    if (!next) {
      return response(
        null,
        false,
        'candidates-exhausted',
        strategy,
        'No valid target account',
        warnings,
      );
    }
    return response(
      next,
      true,
      'switched',
      strategy,
      `Switched to Account-${next.number} (${next.email})`,
      warnings,
    );
  }
}
