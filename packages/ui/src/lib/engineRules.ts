import type { Account, EngineRules, ReserveCriterion, ScoreWeights } from '@swapdeck/types';
import type { MessageKey } from '../i18n/index.js';

/** I 9 criteri di scelta della riserva, nell'ordine dell'interfaccia. */
export const CRITERIA: readonly ReserveCriterion[] = [
  'lowest-weekly',
  'best',
  'fixed-order',
  'reset-soonest',
  'pace-safe',
  'lowest5h',
  'balanced-score',
  'even-spread',
  'model-headroom',
];

/** Pesi di default del punteggio bilanciato (specchio di `ScoreWeights::default` del Rust). */
export const DEFAULT_WEIGHTS: ScoreWeights = {
  fiveHour: 0.4,
  sevenDay: 0.3,
  reset: 0.15,
  pace: 0.15,
};

/** Lista della mappa: `order` se c'è (fonte di verità), altrimenti la forma precedente. */
export function configuredOrder(rules: EngineRules): number[] {
  if (rules.order) return [...rules.order];
  return rules.primary !== null ? [rules.primary, ...rules.reserves] : [];
}

/** Regole come si modificano nei campi: i numeri restano testo finché l'utente scrive. */
export interface RulesDraft {
  enabled: boolean;
  dryRun: boolean;
  /** Utenze della mappa in ordine di priorità; la prima è la principale. */
  order: number[];
  excluded: number[];
  fiveHourThreshold: string;
  /** Vuoto = nessuna soglia sui 7 giorni (solo il blocco al 100%). */
  sevenDayThreshold: string;
  reserveCriterion: ReserveCriterion;
  weights: Record<keyof ScoreWeights, string>;
  headroomModel: string;
  returnToPrimary: boolean;
  returnHysteresis: string;
  minSwitchGapSecs: string;
  ignoredConflicts: string[];
}

export function toDraft(rules: EngineRules): RulesDraft {
  const w = rules.scoreWeights ?? DEFAULT_WEIGHTS;
  return {
    enabled: rules.enabled,
    dryRun: rules.dryRun,
    order: configuredOrder(rules),
    excluded: [...rules.excluded],
    fiveHourThreshold: String(rules.fiveHourThreshold),
    sevenDayThreshold: rules.sevenDayThreshold === null ? '' : String(rules.sevenDayThreshold),
    reserveCriterion: rules.reserveCriterion,
    weights: {
      fiveHour: String(w.fiveHour),
      sevenDay: String(w.sevenDay),
      reset: String(w.reset),
      pace: String(w.pace),
    },
    headroomModel: rules.headroomModel ?? '',
    returnToPrimary: rules.returnToPrimary,
    returnHysteresis: String(rules.returnHysteresis),
    minSwitchGapSecs: String(rules.minSwitchGapSecs),
    ignoredConflicts: [...rules.ignoredConflicts],
  };
}

/**
 * Le regole salvate con i campi della bozza sopra. I pesi
 * e il modello si scrivono solo se cambiano rispetto a `base` (assenti = default del Rust).
 */
export function fromDraft(draft: RulesDraft, base: EngineRules): EngineRules {
  const out: EngineRules = {
    ...base,
    enabled: draft.enabled,
    dryRun: draft.dryRun,
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
  // La lista va in `order` (fonte di verità del Rust); `primary` e `reserves` si scrivono
  // derivati, perché un `primary: null` esplicito accanto a `order` vuol dire «nessuna
  // principale» e farebbe scartare la lista.
  const [first, ...rest] = draft.order;
  if (first !== undefined) {
    out.order = [...draft.order];
    out.primary = first;
    out.reserves = rest;
  } else if (base.order || base.primary !== null) {
    delete out.order;
    out.primary = null;
    out.reserves = [];
  }
  const weights: ScoreWeights = {
    fiveHour: Number(draft.weights.fiveHour),
    sevenDay: Number(draft.weights.sevenDay),
    reset: Number(draft.weights.reset),
    pace: Number(draft.weights.pace),
  };
  const baseWeights = base.scoreWeights ?? DEFAULT_WEIGHTS;
  if (JSON.stringify(weights) !== JSON.stringify(baseWeights)) out.scoreWeights = weights;
  const model = draft.headroomModel.trim();
  if (model !== (base.headroomModel ?? '').trim()) {
    if (model === '') delete out.headroomModel;
    else out.headroomModel = model;
  }
  return out;
}

/** Una riga della lista: numero, account di cswap (assente = citato ma inesistente), inclusa o no. */
export interface FlowRow {
  number: number;
  account: Account | undefined;
  included: boolean;
}

/**
 * Righe della lista come le usa il motore (`effective_order` del Rust): prima quelle della mappa,
 * poi gli account di cswap che la mappa non cita, in ordine di numero. Questi ultimi contano solo
 * con una lista di una sola utenza (le altre sono allora tutte quelle di cswap).
 */
export function flowRows(draft: RulesDraft, accounts: readonly Account[]): FlowRow[] {
  const byNumber = new Map(accounts.map((a) => [a.number, a]));
  const rows: FlowRow[] = draft.order.map((n) => ({
    number: n,
    account: byNumber.get(n),
    included: !draft.excluded.includes(n),
  }));
  const extras = accounts
    .map((a) => a.number)
    .filter((n) => !draft.order.includes(n))
    .sort((a, b) => a - b);
  for (const n of extras)
    rows.push({
      number: n,
      account: byNumber.get(n),
      included: draft.order.length === 1 && !draft.excluded.includes(n),
    });
  return rows;
}

/**
 * Rende esplicita la lista: tutte le righe in `order`, e gli account che il motore non usava fra
 * gli esclusi. Si applica prima di ogni modifica della lista, così il resto non cambia senso. Con
 * una lista vuota (prima configurazione) tutte le utenze di cswap entrano incluse.
 */
export function materialize(draft: RulesDraft, accounts: readonly Account[]): RulesDraft {
  const fresh = draft.order.length === 0;
  const rows = flowRows(draft, accounts);
  const excluded = new Set(draft.excluded);
  for (const r of rows) if (!r.included && !fresh) excluded.add(r.number);
  return {
    ...draft,
    order: rows.map((r) => r.number),
    excluded: [...excluded].filter((n) => rows.some((r) => r.number === n)),
  };
}

/** Sposta la riga `from` in posizione `to` (indici delle righe di `flowRows`). */
export function moveRow(
  draft: RulesDraft,
  accounts: readonly Account[],
  from: number,
  to: number,
): RulesDraft {
  const next = materialize(draft, accounts);
  const order = [...next.order];
  if (from < 0 || from >= order.length || to < 0 || to >= order.length || from === to) return next;
  const [item] = order.splice(from, 1);
  order.splice(to, 0, item!);
  return { ...next, order };
}

/** Include o esclude un'utenza dalla lista. */
export function setIncluded(
  draft: RulesDraft,
  accounts: readonly Account[],
  number: number,
  included: boolean,
): RulesDraft {
  const next = materialize(draft, accounts);
  const excluded = next.excluded.filter((n) => n !== number);
  if (!included) excluded.push(number);
  return { ...next, excluded };
}

/** Utenze citate dalla lista che in cswap non esistono (`EngineRules::missing_accounts` del Rust). */
export function missingAccounts(order: readonly number[], accounts: readonly Account[]): number[] {
  return order.filter((n) => !accounts.some((a) => a.number === n));
}

export type RulesErrors = Partial<
  Record<'order' | 'fiveHour' | 'sevenDay' | 'hysteresis' | 'gap' | 'weights' | 'model', MessageKey>
>;

const pctOk = (v: number) => Number.isFinite(v) && v > 0 && v <= 100;
const weightOk = (v: number) => Number.isFinite(v) && v >= 0 && v <= 100;

/** Stesse regole di `EngineRules::validate_for_save` del Rust, più il formato dei campi. */
export function validateRules(rules: EngineRules): RulesErrors {
  const errors: RulesErrors = {};
  const order = configuredOrder(rules);
  const first = order[0];
  if (first === undefined) {
    if (rules.enabled || rules.dryRun) errors.order = 'rules.err.primaryMissing';
  } else if (rules.excluded.includes(first)) {
    errors.order = 'rules.err.firstExcluded';
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
  const w = rules.scoreWeights;
  if (w) {
    const values = [w.fiveHour, w.sevenDay, w.reset, w.pace];
    if (!values.every(weightOk) || !values.some((v) => v > 0)) errors.weights = 'rules.err.weights';
  }
  if (rules.reserveCriterion === 'model-headroom' && !(rules.headroomModel ?? '').trim())
    errors.model = 'rules.err.model';
  return errors;
}
