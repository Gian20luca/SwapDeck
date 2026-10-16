import type { EngineState } from '@swapdeck/types';
import { describe, expect, it } from 'vitest';
import { DEFAULT_RULES } from '../dev/FakeBackend.js';
import { engineMarks } from './engineMarks.js';

function state(rules: Partial<EngineState['rules']>, view: EngineState['view']): EngineState {
  return { rules: { ...DEFAULT_RULES, ...rules }, view, conflicts: [] };
}
const run = { holder: true, enabled: true, dryRun: false, suspended: null, detail: [] };

describe('engineMarks', () => {
  it('motore acceso e non sospeso: soglie dalle regole, modo attivo', () => {
    const marks = engineMarks(state({ enabled: true, fiveHourThreshold: 90 }, run));
    expect(marks.fiveHour).toMatchObject({ pct: 90, mode: 'active', criterion: 'lowest-weekly' });
    expect(marks.sevenDay).toMatchObject({ pct: 98, mode: 'active' });
  });
  it('acceso ma senza giro fatto: attivo', () => {
    expect(engineMarks(state({ enabled: true }, null)).fiveHour.mode).toBe('active');
  });
  it('spento, anche se la vista dice «disabled»', () => {
    expect(engineMarks(state({ enabled: false }, null)).fiveHour.mode).toBe('off');
    const view = { ...run, enabled: false, suspended: 'disabled' as const };
    expect(engineMarks(state({ enabled: true }, view)).fiveHour.mode).toBe('off');
  });
  it('sola osservazione', () => {
    expect(
      engineMarks(state({ enabled: true, dryRun: true }, { ...run, dryRun: true })).sevenDay?.mode,
    ).toBe('observe');
  });
  it('sospeso: con il motivo', () => {
    const view = { ...run, suspended: 'conflict' as const };
    expect(engineMarks(state({ enabled: true }, view)).fiveHour).toMatchObject({
      mode: 'suspended',
      reason: 'conflict',
    });
  });
  it('senza soglia sui 7 giorni non c’è il trattino', () => {
    expect(engineMarks(state({ enabled: true, sevenDayThreshold: null }, run)).sevenDay).toBeNull();
  });
});
