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

describe('withTargetNames', () => {
  it('il trattino porta il nome dell’utenza di destinazione e il tooltip lo dice', async () => {
    const { withTargetNames } = await import('./engineMarks.js');
    const { engineTip } = await import('../components/UsageBar.js');
    const { translate } = await import('../i18n/index.js');
    const candidate = { number: 1, criterion: 'lowest-weekly' as const, fallback: false, why: 'x' };
    const marks = engineMarks(state({ enabled: true }, { ...run, candidate }));
    const accounts = [{ number: 1, alias: 'links-erica' }] as unknown as Parameters<
      typeof withTargetNames
    >[1];
    const named = withTargetNames(marks, accounts);
    expect(named.fiveHour.targetName).toBe('links-erica');
    const t = (k: Parameters<typeof translate>[1], p?: Record<string, string | number>) =>
      translate('it', k, p);
    expect(engineTip(named.fiveHour, t)).toMatch(/\(ora: links-erica\)$/);
    expect(withTargetNames(marks, []).fiveHour.targetName).toBe('#1');
    const none = engineMarks(state({ enabled: true }, run));
    expect(withTargetNames(none, accounts).fiveHour.targetName).toBeUndefined();
  });
});
