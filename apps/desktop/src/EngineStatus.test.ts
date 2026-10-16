import { describe, expect, it } from 'vitest';
import { engineLines } from './EngineStatus.js';
import type { EngineView, Snapshot } from './TauriBackend.js';

const engine = (view: Partial<EngineView>): EngineView => ({
  holder: true,
  enabled: true,
  dryRun: false,
  suspended: null,
  detail: [],
  ...view,
});

const snap = (s: Partial<Snapshot>): Snapshot => ({
  list: null,
  error: null,
  updatedAtMs: 1,
  engineSuspended: false,
  autoRunning: false,
  engine: null,
  ...s,
});

describe('engineLines', () => {
  it('is empty when there is nothing to report', () => {
    expect(engineLines(null, 'it')).toEqual([]);
    expect(engineLines(snap({}), 'it')).toEqual([]);
    expect(
      engineLines(snap({ engine: engine({ enabled: false, suspended: 'disabled' }) }), 'it'),
    ).toEqual([]);
  });

  it('shows a suspension with its detail', () => {
    const s = snap({
      engineSuspended: true,
      engine: engine({ suspended: 'conflict', detail: ['cswap-guard'] }),
    });
    expect(engineLines(s, 'it')).toEqual(['Motore sospeso: altro motore attivo (cswap-guard)']);
    expect(engineLines(s, 'en')).toEqual([
      'Engine suspended: another engine is active (cswap-guard)',
    ]);
  });

  it('shows observer mode and cswap auto', () => {
    const s = snap({ autoRunning: true, engine: engine({ holder: false }) });
    expect(engineLines(s, 'en')).toEqual([
      'Engine running in another instance: observing only',
      'cswap auto running',
    ]);
    expect(engineLines(snap({ engine: engine({ dryRun: true }) }), 'it')).toEqual([
      'Motore in sola osservazione',
    ]);
  });
});
