import type { SetupProgress } from '@swapdeck/types';
import { describe, expect, it } from 'vitest';
import { EMPTY_RUN, MAX_LOG_LINES, reduceProgress, setupErrorKey, stepKey } from './setup.js';

describe('reduceProgress', () => {
  it('segue i passi e conserva l’errore', () => {
    const events: SetupProgress[] = [
      { event: 'stepSkipped', step: 'downloadUv' },
      { event: 'stepStarted', step: 'installCswap' },
      { event: 'line', step: 'installCswap', text: 'Resolved 17 packages' },
      { event: 'stepFailed', step: 'installCswap', error: { kind: 'busy', message: 'x' } },
    ];
    const state = events.reduce(reduceProgress, EMPTY_RUN);
    expect(state.steps).toEqual({ downloadUv: 'skipped', installCswap: 'failed' });
    expect(state.lines).toEqual(['Resolved 17 packages']);
    expect(state.error?.kind).toBe('busy');
    const done = reduceProgress(state, { event: 'stepDone', step: 'verifyCswap' });
    expect(done.steps.verifyCswap).toBe('done');
  });

  it('tiene solo le ultime righe', () => {
    let state = EMPTY_RUN;
    for (let i = 0; i < MAX_LOG_LINES + 5; i += 1) {
      state = reduceProgress(state, { event: 'line', step: 'installUv', text: `r${i}` });
    }
    expect(state.lines).toHaveLength(MAX_LOG_LINES);
    expect(state.lines[0]).toBe('r5');
  });
});

describe('chiavi', () => {
  it('errori e passi', () => {
    expect(setupErrorKey({ kind: 'busy', message: '' })).toBe('setup.error.busy');
    expect(setupErrorKey({ kind: 'unsupportedSchema', message: '' })).toBe('setup.error.schema');
    expect(setupErrorKey({ kind: 'timeout', message: '' })).toBe('setup.error.timeout');
    expect(setupErrorKey({ kind: 'notInstalled', message: '' })).toBe('setup.error.notInstalled');
    expect(setupErrorKey({ kind: 'failed', message: '' })).toBe('setup.error.generic');
    expect(stepKey('verifyCswap')).toBe('setup.step.verifyCswap');
  });
});
