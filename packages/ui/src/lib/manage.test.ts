import type { EngineRules } from '@swapdeck/types';
import { describe, expect, it } from 'vitest';
import { DEFAULT_RULES } from '../dev/FakeBackend.js';
import { configValueText, validateConfigValue } from './cswapConfig.js';
import {
  applyGuardPreset,
  fromDraft,
  GUARD_PRESET,
  toDraft,
  validateRules,
} from './engineRules.js';
import { mergeEvents } from './historyEvents.js';

const rules = (patch: Partial<EngineRules> = {}): EngineRules => ({ ...DEFAULT_RULES, ...patch });

describe('engineRules', () => {
  it('il preset è quello di cswap-guard', () => {
    expect(GUARD_PRESET).toMatchObject({
      primary: 6,
      reserves: [1, 3],
      reserveCriterion: 'lowest-weekly',
      fiveHourThreshold: 95,
    });
    const applied = applyGuardPreset(rules({ enabled: true, dryRun: true, excluded: [2] }));
    expect(applied).toMatchObject({
      primary: 6,
      reserves: [1, 3],
      enabled: true,
      dryRun: true,
      excluded: [2],
    });
    // Copia, non il riferimento del preset.
    applied.reserves.push(9);
    expect(GUARD_PRESET.reserves).toEqual([1, 3]);
  });

  it('bozza e regole si convertono senza perdere i campi che la UI non modifica', () => {
    const base = rules({ maxUsageAgeSecs: 1234, sevenDayThreshold: null });
    const draft = toDraft(base);
    expect(draft.sevenDayThreshold).toBe('');
    expect(fromDraft(draft, base)).toEqual(base);
    expect(fromDraft({ ...draft, sevenDayThreshold: '80' }, base).sevenDayThreshold).toBe(80);
    expect(fromDraft({ ...draft, fiveHourThreshold: 'abc' }, base).fiveHourThreshold).toBeNaN();
  });

  it('validazione come EngineRules::validate', () => {
    expect(validateRules(rules())).toEqual({});
    expect(validateRules(rules({ enabled: true }))).toEqual({
      primary: 'rules.err.primaryMissing',
    });
    expect(validateRules(rules({ primary: 6, reserves: [6] }))).toEqual({
      primary: 'rules.err.primaryReserve',
    });
    expect(validateRules(rules({ primary: 6, excluded: [6] }))).toEqual({
      primary: 'rules.err.primaryExcluded',
    });
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
    expect(validateRules(rules({ reserves: [2], excluded: [2] })).reserves).toBe(
      'rules.err.reserveExcluded',
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
