import type { EngineRules } from '@swapdeck/types';
import { describe, expect, it } from 'vitest';
import { DEFAULT_RULES } from '../dev/FakeBackend.js';
import { configValueText, validateConfigValue } from './cswapConfig.js';
import type { Account } from '@swapdeck/types';
import {
  flowRows,
  fromDraft,
  materialize,
  missingAccounts,
  moveRow,
  setIncluded,
  toDraft,
  validateRules,
} from './engineRules.js';
import { mergeEvents } from './historyEvents.js';

const rules = (patch: Partial<EngineRules> = {}): EngineRules => ({ ...DEFAULT_RULES, ...patch });
const acct = (number: number): Account =>
  ({ number, email: `u${number}@x.it` }) as unknown as Account;
const accounts = [1, 2, 3, 6].map(acct);

describe('engineRules', () => {
  it('bozza e regole si convertono senza perdere i campi che la UI non modifica', () => {
    const base = rules({ maxUsageAgeSecs: 1234, sevenDayThreshold: null });
    const draft = toDraft(base);
    expect(draft.sevenDayThreshold).toBe('');
    expect(fromDraft(draft, base)).toEqual(base);
    expect(fromDraft({ ...draft, sevenDayThreshold: '80' }, base).sevenDayThreshold).toBe(80);
    expect(fromDraft({ ...draft, fiveHourThreshold: 'abc' }, base).fiveHourThreshold).toBeNaN();
  });

  it('la lista va in order; principale e riserve ne sono derivati', () => {
    const base = rules({ order: [6, 1, 3], primary: 6, reserves: [1, 3] });
    const moved = moveRow(toDraft(base), accounts, 2, 0);
    const out = fromDraft(moved, base);
    expect(out.order).toEqual([3, 6, 1, 2]);
    expect(out.primary).toBe(3);
    expect(out.reserves).toEqual([6, 1, 2]);
    // 2 non era nella lista: entra in fondo ma esclusa, così il motore fa come prima.
    expect(out.excluded).toEqual([2]);
  });

  it('pesi e modello si scrivono solo se cambiano', () => {
    const base = rules();
    const draft = toDraft(base);
    expect(fromDraft(draft, base).scoreWeights).toBeUndefined();
    const w = fromDraft({ ...draft, weights: { ...draft.weights, pace: '0' } }, base);
    expect(w.scoreWeights).toEqual({ fiveHour: 0.4, sevenDay: 0.3, reset: 0.15, pace: 0 });
    expect(fromDraft({ ...draft, headroomModel: ' Fable ' }, base).headroomModel).toBe('Fable');
  });

  it('righe del flusso come effective_order del Rust', () => {
    // Una sola utenza: le altre di cswap entrano tutte, in ordine di numero.
    const one = flowRows(toDraft(rules({ order: [6] })), accounts);
    expect(one.map((r) => [r.number, r.included])).toEqual([
      [6, true],
      [1, true],
      [2, true],
      [3, true],
    ]);
    // Più utenze: quelle non citate restano fuori.
    const two = flowRows(toDraft(rules({ order: [6, 9], excluded: [] })), accounts);
    expect(two.map((r) => [r.number, r.included, r.account !== undefined])).toEqual([
      [6, true, true],
      [9, true, false],
      [1, false, true],
      [2, false, true],
      [3, false, true],
    ]);
    expect(missingAccounts([6, 9], accounts)).toEqual([9]);
    // Lista vuota (prima configurazione): tutte incluse.
    const fresh = materialize(toDraft(rules()), accounts);
    expect(fresh.order).toEqual([1, 2, 3, 6]);
    expect(fresh.excluded).toEqual([]);
  });

  it('includi ed escludi', () => {
    const draft = toDraft(rules({ order: [6, 1] }));
    const off = setIncluded(draft, accounts, 1, false);
    expect(off.excluded.sort()).toEqual([1, 2, 3]);
    expect(setIncluded(off, accounts, 3, true).excluded.sort()).toEqual([1, 2]);
    expect(moveRow(draft, accounts, 0, 9).order).toEqual([6, 1, 2, 3]);
  });

  it('validazione come EngineRules::validate_for_save', () => {
    expect(validateRules(rules())).toEqual({});
    expect(validateRules(rules({ enabled: true }))).toEqual({
      order: 'rules.err.primaryMissing',
    });
    expect(validateRules(rules({ order: [6, 1], excluded: [6] }))).toEqual({
      order: 'rules.err.firstExcluded',
    });
    expect(validateRules(rules({ order: [6, 1], excluded: [1] }))).toEqual({});
    expect(validateRules(rules({ fiveHourThreshold: 0 })).fiveHour).toBe('rules.err.threshold');
    expect(validateRules(rules({ fiveHourThreshold: 100.5 })).fiveHour).toBe('rules.err.threshold');
    expect(validateRules(rules({ fiveHourThreshold: Number.NaN })).fiveHour).toBe(
      'rules.err.threshold',
    );
    expect(validateRules(rules({ sevenDayThreshold: 101 })).sevenDay).toBe('rules.err.threshold');
    expect(validateRules(rules({ sevenDayThreshold: null })).sevenDay).toBeUndefined();
    expect(validateRules(rules({ returnHysteresis: 51 })).hysteresis).toBe('rules.err.hysteresis');
    expect(validateRules(rules({ returnHysteresis: -1 })).hysteresis).toBe('rules.err.hysteresis');
    expect(validateRules(rules({ minSwitchGapSecs: -5 })).gap).toBe('rules.err.gap');
    expect(validateRules(rules({ minSwitchGapSecs: 1.5 })).gap).toBe('rules.err.gap');
    expect(
      validateRules(rules({ scoreWeights: { fiveHour: 0, sevenDay: 0, reset: 0, pace: 0 } }))
        .weights,
    ).toBe('rules.err.weights');
    expect(validateRules(rules({ reserveCriterion: 'model-headroom' })).model).toBe(
      'rules.err.model',
    );
  });
});

describe('cswapConfig', () => {
  it('valida le chiavi del contratto', () => {
    expect(validateConfigValue('autoswitch.threshold', '85')).toBeNull();
    expect(validateConfigValue('autoswitch.threshold', '49')).toBe('config.err.range');
    expect(validateConfigValue('autoswitch.threshold', '')).toBe('config.err.number');
    expect(validateConfigValue('autoswitch.threshold', 'x')).toBe('config.err.number');
    expect(validateConfigValue('autoswitch.unhealthyTicks', '2.5')).toBe('config.err.integer');
    expect(validateConfigValue('autoswitch.unhealthyTicks', '3')).toBeNull();
    expect(validateConfigValue('autoswitch.strategy', 'best')).toBeNull();
    expect(validateConfigValue('autoswitch.strategy', 'altro')).toBe('config.err.choice');
    expect(validateConfigValue('autoswitch.includeApiKeyAccounts', 'true')).toBeNull();
    expect(validateConfigValue('autoswitch.includeApiKeyAccounts', 'yes')).toBe(
      'config.err.choice',
    );
    expect(validateConfigValue('autoswitch.model', 'opus,sonnet')).toBeNull();
    expect(validateConfigValue('autoswitch.model', '  ')).toBe('config.err.empty');
    expect(validateConfigValue('ui.theme', 'auto')).toBeNull();
    expect(validateConfigValue('chiave.sconosciuta', 'x')).toBeNull();
    expect(validateConfigValue('chiave.sconosciuta', '')).toBe('config.err.empty');
  });

  it('testo del valore JSON', () => {
    expect(configValueText(null)).toBe('');
    expect(configValueText(undefined)).toBe('');
    expect(configValueText(90)).toBe('90');
    expect(configValueText(false)).toBe('false');
    expect(configValueText('best')).toBe('best');
  });
});

describe('historyEvents', () => {
  it('unisce cambi e decisioni dal più recente', () => {
    const rows = mergeEvents({
      switches: [{ ts: 10, from: 1, to: 2, cause: 'primaryAtLimit', switched: true, message: '' }],
      decisions: [
        { ts: 30, decision: { kind: 'noAction', reason: 'cooldown' }, dryRun: false },
        { ts: 20, decision: { kind: 'suspended', reason: 'conflict', detail: [] }, dryRun: false },
      ],
    });
    expect(rows.map((r) => [r.kind, r.ts])).toEqual([
      ['decision', 30],
      ['decision', 20],
      ['switch', 10],
    ]);
  });
});
