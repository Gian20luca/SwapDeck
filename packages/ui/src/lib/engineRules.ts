import type { EngineRules } from '@swapdeck/types';
import type { MessageKey } from '../i18n/index.js';

/** Preset «come cswap-guard»: principale 6, riserve [1, 3], settimanale più bassa, soglia 95%, ritorno dopo il reset. */
export const GUARD_PRESET = {
  primary: 6,
  reserves: [1, 3],
  reserveCriterion: 'lowest-weekly',
  fiveHourThreshold: 95,
  returnToPrimary: true,
} as const satisfies Partial<EngineRules>;

export function applyGuardPreset(rules: EngineRules): EngineRules {
  return { ...rules, ...GUARD_PRESET, reserves: [...GUARD_PRESET.reserves] };
}

/** Regole come si modificano nei campi: i numeri restano testo finché l'utente scrive. */
export interface RulesDraft {
  enabled: boolean;
  dryRun: boolean;
  primary: number | null;
  reserves: number[];
  excluded: number[];
  fiveHourThreshold: string;
  /** Vuoto = nessuna soglia sui 7 giorni (solo il blocco al 100%). */
  sevenDayThreshold: string;
  reserveCriterion: EngineRules['reserveCriterion'];
  returnToPrimary: boolean;
  returnHysteresis: string;
  minSwitchGapSecs: string;
  ignoredConflicts: string[];
}

export function toDraft(rules: EngineRules): RulesDraft {
  return {
    enabled: rules.enabled,
    dryRun: rules.dryRun,
    primary: rules.primary,
    reserves: [...rules.reserves],
    excluded: [...rules.excluded],
    fiveHourThreshold: String(rules.fiveHourThreshold),
    sevenDayThreshold: rules.sevenDayThreshold === null ? '' : String(rules.sevenDayThreshold),
    reserveCriterion: rules.reserveCriterion,
    returnToPrimary: rules.returnToPrimary,
    returnHysteresis: String(rules.returnHysteresis),
    minSwitchGapSecs: String(rules.minSwitchGapSecs),
    ignoredConflicts: [...rules.ignoredConflicts],
  };
}

/** Le regole salvate con i campi della bozza sopra; i campi che la UI non modifica restano di `base`. */
export function fromDraft(draft: RulesDraft, base: EngineRules): EngineRules {
  return {
    ...base,
    enabled: draft.enabled,
    dryRun: draft.dryRun,
    primary: draft.primary,
    reserves: [...draft.reserves],
    excluded: [...draft.excluded],
    fiveHourThreshold: Number(draft.fiveHourThreshold),
    sevenDayThreshold:
      draft.sevenDayThreshold.trim() === '' ? null : Number(draft.sevenDayThreshold),
    reserveCriterion: draft.reserveCriterion,
    returnToPrimary: draft.returnToPrimary,
    returnHysteresis: Number(draft.returnHysteresis),
    minSwitchGapSecs: Number(draft.minSwitchGapSecs),
    ignoredConflicts: [...draft.ignoredConflicts],
  };
}

export type RulesErrors = Partial<
  Record<'primary' | 'fiveHour' | 'sevenDay' | 'hysteresis' | 'gap' | 'reserves', MessageKey>
>;

const pctOk = (v: number) => Number.isFinite(v) && v > 0 && v <= 100;

/** Stesse regole di `EngineRules::validate` del Rust, più il formato dei campi; il principale serve solo a motore acceso. */
export function validateRules(rules: EngineRules): RulesErrors {
  const errors: RulesErrors = {};
  if (rules.primary === null) {
    if (rules.enabled) errors.primary = 'rules.err.primaryMissing';
  } else if (rules.excluded.includes(rules.primary)) {
    errors.primary = 'rules.err.primaryExcluded';
  } else if (rules.reserves.includes(rules.primary)) {
    errors.primary = 'rules.err.primaryReserve';
  }
  if (!pctOk(rules.fiveHourThreshold)) errors.fiveHour = 'rules.err.threshold';
  if (rules.sevenDayThreshold !== null && !pctOk(rules.sevenDayThreshold))
    errors.sevenDay = 'rules.err.threshold';
  if (!(
    Number.isFinite(rules.returnHysteresis) &&
    rules.returnHysteresis >= 0 &&
    rules.returnHysteresis <= 50
  ))
    errors.hysteresis = 'rules.err.hysteresis';
  if (!(Number.isInteger(rules.minSwitchGapSecs) && rules.minSwitchGapSecs >= 0))
    errors.gap = 'rules.err.gap';
  if (rules.reserves.some((n) => rules.excluded.includes(n)))
    errors.reserves = 'rules.err.reserveExcluded';
  return errors;
}
