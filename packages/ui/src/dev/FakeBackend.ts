import type {
  Account,
  ConfigGetResponse,
  ConfigListResponse,
  Confirmation,
  Conflict,
  CswapAction,
  EngineProfiles,
  EngineRules,
  ListResponse,
  NotificationConfig,
  ProfileCommand,
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
  type AppPreferences,
  type Backend,
  type DirectoryMapping,
  type EngineState,
  type EngineViewInfo,
  type HistoryEvents,
  type HistoryPoint,
  type HistoryQuery,
  type HostSettings,
  type NotificationPatch,
  type MiniWindowControls,
  type PickOptions,
  type SetupBackend,
  type Unsubscribe,
} from '../backend/types.js';
import { CONFIG_KEYS } from '../lib/cswapConfig.js';
import { validateRules } from '../lib/engineRules.js';
import configList from '../../../../fixtures/cswap/config-list.json';
import { loadScenario, type ScenarioName } from './fixtures.js';
import { generateEvents, generateHistory } from './history.js';
import { FakeSetup, type FakeSetupOptions } from './fakeSetup.js';
import { applyProfile, checkEditable, fakeCandidate, seedProfiles } from './fakeProfiles.js';
import { defaultNotifications, patchNotifications } from './fakeNotifications.js';

export interface FakeBackendOptions {
  scenario?: ScenarioName;
  /** Ritardo simulato di ogni chiamata, in ms (default 0: i test restano veloci). */
  latencyMs?: number;
  /** Orologio iniettabile per i test. */
  now?: () => number;
  /** Credenziali parcheggiate di `unclaimed` (default: nessuna). */
  unclaimed?: string[];
  /** Cartelle già mappate (default: nessuna). */
  mappings?: DirectoryMapping[];
  /** Altri motori rilevati (default: nessuno). */
  conflicts?: Conflict[];
  /** Regole iniziali del motore (default: quelle del Rust, motore spento). */
  rules?: Partial<EngineRules>;
  /** Percorsi restituiti dai selettori nativi; `null` = annullato. */
  picks?: { openFile?: string | null; saveFile?: string | null; folder?: string | null };
  /** Simula un host che conferma da sé (come VS Code): la UI non deve mostrare il suo dialogo. */
  hostConfirms?: boolean;
  /** Simula l'host desktop con la finestra del mini-widget (default `true`; `false` = come VS Code). */
  miniWindow?: boolean;
  /** Stato simulato di cswap e uv per la procedura di primo avvio e l'aggiornamento. */
  setup?: FakeSetupOptions;
}

/** Regole di default del motore (specchio di `EngineRules::default` del Rust). */
export const DEFAULT_RULES: EngineRules = {
  enabled: false,
  dryRun: false,
  primary: null,
  reserves: [],
  excluded: [],
  fiveHourThreshold: 95,
  sevenDayThreshold: 98,
  reserveCriterion: 'lowest-weekly',
  returnToPrimary: true,
  returnHysteresis: 10,
  minSwitchGapSecs: 300,
  maxUsageAgeSecs: 900,
  ignoredConflicts: [],
};

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
export class FakeBackend implements Backend, SetupBackend {
  readonly hostConfirms?: boolean;
  /** Finestra del mini-widget simulata (assente con `miniWindow: false`). */
  readonly miniWindow?: MiniWindowControls;
  /** Chiamate a `miniWindow`, utili ai test: `hide`, `compact`, `expand`. */
  readonly miniLog: string[] = [];
  /** Altezze chieste con `fitHeight`. */
  readonly miniHeights: number[] = [];
  private miniCompact = false;
  private list: ListResponse;
  private mappings: DirectoryMapping[];
  private rules: EngineRules;
  private profiles: EngineProfiles;
  private notifications: NotificationConfig = defaultNotifications();
  private readonly conflicts: Conflict[];
  private prefs: AppPreferences = {
    pollIntervalSeconds: 60,
    notificationThresholds: [80, 95],
    language: 'it',
    theme: 'system',
  };
  private host: HostSettings = { autostart: true, autostartManaged: true, version: '0.0.0-fake' };
  private readonly picks: NonNullable<FakeBackendOptions['picks']>;
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
    if (options.unclaimed && options.unclaimed.length > 0)
      this.list.unclaimedCredentials = [...options.unclaimed];
    this.mappings = structuredClone(options.mappings ?? []);
    this.conflicts = structuredClone(options.conflicts ?? []);
    this.rules = { ...structuredClone(DEFAULT_RULES), ...structuredClone(options.rules ?? {}) };
    this.profiles = seedProfiles(this.rules);
    this.picks = options.picks ?? {};
    if (options.hostConfirms !== undefined) this.hostConfirms = options.hostConfirms;
    if (options.miniWindow !== false) {
      this.miniWindow = {
        hide: async () => {
          this.miniLog.push('hide');
        },
        setCompact: async (compact) => {
          this.miniCompact = compact;
          this.miniLog.push(compact ? 'compact' : 'expand');
        },
        isCompact: async () => this.miniCompact,
        fitHeight: async (height) => {
          this.miniHeights.push(height);
        },
      };
    }
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

  async getHistoryEvents(query: HistoryQuery = {}): Promise<HistoryEvents> {
    await this.delay();
    return generateEvents(this.list, this.now(), query.sinceSeconds);
  }

  async listMappings(): Promise<DirectoryMapping[]> {
    await this.delay();
    return structuredClone(this.mappings);
  }

  private engineState(): EngineState {
    const rules = structuredClone(this.rules);
    const ignored = rules.ignoredConflicts.map((c) => c.toLowerCase());
    const blocking = this.conflicts.filter(
      (c) => c.certain && !ignored.includes(c.id.toLowerCase()),
    );
    const problems = Object.keys(validateRules(rules));
    let view: EngineViewInfo;
    if (!rules.enabled) {
      view = {
        holder: true,
        enabled: false,
        dryRun: rules.dryRun,
        suspended: 'disabled',
        detail: [],
      };
    } else if (problems.length > 0) {
      view = {
        holder: true,
        enabled: true,
        dryRun: rules.dryRun,
        suspended: 'invalidRules',
        detail: problems,
      };
    } else if (blocking.length > 0) {
      view = {
        holder: true,
        enabled: true,
        dryRun: rules.dryRun,
        suspended: 'conflict',
        detail: blocking.map((c) => c.id),
      };
    } else {
      view = { holder: true, enabled: true, dryRun: rules.dryRun, suspended: null, detail: [] };
      const candidate = fakeCandidate(rules, this.list.accounts);
      if (candidate) view.candidate = candidate;
    }
    return { rules, view, conflicts: structuredClone(this.conflicts) };
  }

  async getEngineState(): Promise<EngineState> {
    await this.delay();
    return this.engineState();
  }

  async setEngineRules(rules: EngineRules): Promise<EngineState> {
    await this.delay();
    const problems = Object.keys(validateRules(rules));
    if (problems.length > 0) fail(`Regole non valide: ${problems.join(', ')}`, 'ConfigError');
    try {
      checkEditable(this.profiles, this.rules, rules);
    } catch (error) {
      fail(error instanceof Error ? error.message : String(error), 'ConfigError');
    }
    this.rules = structuredClone(rules);
    return this.engineState();
  }

  async getEngineProfiles(): Promise<EngineProfiles> {
    await this.delay();
    const out = structuredClone(this.profiles);
    const active = out.profiles.find((p) => p.id === out.activeId);
    if (active) active.rules = structuredClone(this.rules);
    return out;
  }

  async getNotificationSettings(): Promise<NotificationConfig> {
    await this.delay();
    return structuredClone(this.notifications);
  }

  async setNotificationSettings(patch: NotificationPatch): Promise<NotificationConfig> {
    await this.delay();
    try {
      this.notifications = patchNotifications(this.notifications, patch);
    } catch (error) {
      fail(error instanceof Error ? error.message : String(error), 'ConfigError');
    }
    return structuredClone(this.notifications);
  }

  async applyEngineProfile(command: ProfileCommand): Promise<EngineState> {
    await this.delay();
    let next: EngineProfiles;
    try {
      next = applyProfile(this.profiles, command, this.rules);
    } catch (error) {
      fail(error instanceof Error ? error.message : String(error), 'ConfigError');
    }
    const rules = next.profiles.find((p) => p.id === next.activeId)!.rules;
    // Le regole della nuova mappa attiva entrano solo se valide (come `apply_profile_command`).
    if (JSON.stringify(rules) !== JSON.stringify(this.rules)) {
      const problems = Object.keys(validateRules(rules));
      if (problems.length > 0) fail(`Regole non valide: ${problems.join(', ')}`, 'ConfigError');
    }
    this.profiles = next;
    this.rules = structuredClone(rules);
    this.emit();
    return this.engineState();
  }

  async getPreferences(): Promise<AppPreferences> {
    await this.delay();
    return structuredClone(this.prefs);
  }

  async setPreferences(prefs: Partial<AppPreferences>): Promise<AppPreferences> {
    await this.delay();
    this.prefs = { ...this.prefs, ...structuredClone(prefs) };
    return structuredClone(this.prefs);
  }

  async getAppSettings(): Promise<HostSettings> {
    await this.delay();
    return { ...this.host };
  }

  async setAutostart(enabled: boolean): Promise<HostSettings> {
    await this.delay();
    this.host = { ...this.host, autostart: enabled };
    return { ...this.host };
  }

  async pickOpenFile(_options?: PickOptions): Promise<string | null> {
    return this.picks.openFile === undefined
      ? String.raw`C:\Temp\swapdeck-import.json`
      : this.picks.openFile;
  }

  async pickSaveFile(_options?: PickOptions): Promise<string | null> {
    return this.picks.saveFile === undefined
      ? String.raw`C:\Temp\swapdeck-export.json`
      : this.picks.saveFile;
  }

  async pickFolder(_options?: PickOptions): Promise<string | null> {
    return this.picks.folder === undefined ? String.raw`C:\Progetti\demo` : this.picks.folder;
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
    // Il token di `addToken` non finisce mai nel registro delle azioni.
    this.actionLog.push(action.kind === 'addToken' ? { ...action, token: '***' } : action);
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
      case 'map': {
        const account =
          this.find(action.account) ??
          fail(`Account-${action.account} does not exist`, 'AccountNotFoundError');
        this.mappings = this.mappings.filter((m) => m.path !== action.path);
        this.mappings.push({ path: action.path, account: account.number, email: account.email });
        return { message: `Mapped ${action.path} → Account-${account.number} (${account.email})` };
      }
      case 'unmap': {
        const before = this.mappings.length;
        this.mappings = this.mappings.filter((m) => m.path !== action.path);
        return {
          message:
            before === this.mappings.length
              ? `No mapping for ${action.path}`
              : `Unmapped ${action.path}`,
        };
      }
      case 'configUnset': {
        const setting = this.config.settings.find((s) => s.key === action.key);
        if (!setting) fail(`unknown setting '${action.key}'`, 'ConfigError');
        const fallback = CONFIG_KEYS[action.key]?.default;
        setting.value =
          fallback === undefined || fallback === ''
            ? null
            : fallback === 'true' || fallback === 'false'
              ? fallback === 'true'
              : Number.isNaN(Number(fallback))
                ? fallback
                : Number(fallback);
        setting.isSet = false;
        return {};
      }
      case 'unclaimedPurge': {
        const ids = this.list.unclaimedCredentials ?? [];
        if (!ids.includes(action.id))
          fail(`No unclaimed credential entry '${action.id}'`, 'ConfigError');
        const rest = ids.filter((id) => id !== action.id);
        if (rest.length > 0) this.list.unclaimedCredentials = rest;
        else delete this.list.unclaimedCredentials;
        this.emit();
        return { message: `Purged ${action.id}` };
      }
      case 'addToken': {
        const taken = new Set(this.list.accounts.map((a) => a.number));
        let slot = action.slot ?? 1;
        if (action.slot === undefined) while (taken.has(slot)) slot += 1;
        else if (taken.has(slot) && !action.overwrite)
          fail(`Slot ${slot} already occupied`, 'ConfigError');
        this.list.accounts = this.list.accounts.filter((a) => a.number !== slot);
        this.list.accounts.push({
          number: slot,
          email: action.email ?? `setup-token-${slot}@token.local`,
          organizationName: '',
          organizationUuid: '',
          isOrganization: false,
          active: false,
          usageStatus: 'api_key',
          usage: null,
        });
        this.list.accounts.sort((x, y) => x.number - y.number);
        this.emit();
        return { message: `Added Account-${slot}` };
      }
      case 'purge':
        this.list.accounts = [];
        this.list.activeAccountNumber = null;
        delete this.list.unclaimedCredentials;
        this.mappings = [];
        this.emit();
        return { message: 'Purged' };
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
