import type { Account, EngineState, ReserveCriterion, SuspendReason } from '@swapdeck/types';

/** Come il motore usa la soglia: `active` cambia account davvero; gli altri tre non cambiano nulla. */
export type MarkMode = 'active' | 'off' | 'observe' | 'suspended';

/** Soglia di uscita del motore su una finestra, per il trattino sulla barra. */
export interface EngineMark {
  pct: number;
  mode: MarkMode;
  criterion: ReserveCriterion;
  /** Solo con `mode: 'suspended'`. */
  reason: SuspendReason | null;
  /** Utenza a cui andrebbe adesso il motore (`EngineView.candidate`). */
  target?: number | null;
  /** Nome di `target` (alias o numero), per il tooltip del trattino. */
  targetName?: string;
}

/** Trattini dell'account attivo. Il motore decide solo sulle 5 ore e sui 7 giorni: i limiti per modello non hanno soglia. */
export interface EngineMarks {
  fiveHour: EngineMark;
  /** `null` se la soglia 7d è spenta (solo il blocco al 100%) e l'utenza non ha un massimo 7d. */
  sevenDay: EngineMark | null;
}

/**
 * Soglie e modo dalle regole e dallo stato del motore (letti dal Backend, mai fissi nel codice).
 * Con `active` le soglie sono quelle effettive dell'utenza attiva calcolate dal motore
 * (`EngineState.thresholds`: anche il suo massimo, F22); senza, quelle della mappa.
 */
export function engineMarks(state: EngineState, active?: number | null): EngineMarks {
  const { rules, view } = state;
  const own = state.thresholds?.find((t) => t.number === active);
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
    target: view?.candidate?.number ?? null,
  });
  const fiveHour = own ? own.fiveHour : rules.fiveHourThreshold;
  const sevenDay = own ? own.sevenDay : rules.sevenDayThreshold;
  return {
    fiveHour: make(fiveHour),
    sevenDay: sevenDay === null ? null : make(sevenDay),
  };
}

/** Aggiunge ai trattini il nome dell'utenza di destinazione (alias, altrimenti `#numero`). */
export function withTargetNames(marks: EngineMarks, accounts: readonly Account[]): EngineMarks {
  const name = (mark: EngineMark): EngineMark => {
    if (mark.target === null || mark.target === undefined) return mark;
    const found = accounts.find((a) => a.number === mark.target);
    return { ...mark, targetName: found?.alias ? found.alias : `#${mark.target}` };
  };
  return { fiveHour: name(marks.fiveHour), sevenDay: marks.sevenDay ? name(marks.sevenDay) : null };
}
