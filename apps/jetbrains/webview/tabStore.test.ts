import { describe, expect, it } from 'vitest';
import { STATE_KEY, loadState, saveState } from './tabStore.js';

function memory() {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
  };
}

describe('scheda ricordata in localStorage', () => {
  it('salva e rilegge la scheda', () => {
    const s = memory();
    saveState(s, { tab: 'history' });
    expect(s.data.get(STATE_KEY)).toBe('{"tab":"history"}');
    expect(loadState(s)).toEqual({ tab: 'history' });
  });

  it('valori estranei, JSON rotto o storage assente valgono come vuoto', () => {
    const s = memory();
    expect(loadState(s)).toEqual({});
    s.data.set(STATE_KEY, '{"tab":"nope"}');
    expect(loadState(s)).toEqual({});
    s.data.set(STATE_KEY, '{rotto');
    expect(loadState(s)).toEqual({});
    expect(loadState(undefined)).toEqual({});
  });

  it('gli errori dello storage non escono', () => {
    const failing = {
      getItem: () => {
        throw new Error('SecurityError');
      },
      setItem: () => {
        throw new Error('QuotaExceededError');
      },
    };
    expect(loadState(failing)).toEqual({});
    expect(() => saveState(failing, { tab: 'rules' })).not.toThrow();
    expect(() => saveState(undefined, { tab: 'rules' })).not.toThrow();
  });
});
