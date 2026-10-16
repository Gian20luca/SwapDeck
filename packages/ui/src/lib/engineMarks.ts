import type { EngineState, ReserveCriterion, SuspendReason } from '@swapdeck/types';

/** Come il motore usa la soglia: `active` cambia account davvero; gli altri tre non cambiano nulla. */
export type MarkMode = 'active' | 'off' | 'observe' | 'suspended';

/** Soglia di uscita del motore su una finestra, per il trattino sulla barra. */
export interface EngineMark {
  pct: number;
  mode: MarkMode;
  criterion: ReserveCriterion;
  /** Solo con `mode: 'suspended'`. */
  reason: SuspendReason | null;
}

/** Trattini dell'account attivo. Il motore decide solo sulle 5 ore e sui 7 giorni: i limiti per modello non hanno soglia. */
export interface EngineMarks {
  fiveHour: EngineMark;
  /** `null` se `sevenDayThreshold` è spenta (solo il blocco al 100%). */
  sevenDay: EngineMark | null;
}

/** Soglie e modo dalle regole e dallo stato del motore (letti dal Backend, mai fissi nel codice). */
export function engineMarks(state: EngineState): EngineMarks {
  const { rules, view } = state;
  const suspended = view?.suspended ?? null;
  let mode: MarkMode = 'active';
  let reason: SuspendReason | null = null;
  if (!rules.enabled || suspended === 'disabled') mode = 'off';
  else if (suspended) {
    mode = 'suspended';
    reason = suspended;
  } else if (rules.dryRun || view?.dryRun) mode = 'observe';
  const make = (pct: number): EngineMark => ({
    pct,
    mode,
    criterion: rules.reserveCriterion,
    reason,
  });
  return {
    fiveHour: make(rules.fiveHourThreshold),
    sevenDay: rules.sevenDayThreshold === null ? null : make(rules.sevenDayThreshold),
  };
}
