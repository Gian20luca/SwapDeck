import { describe, expect, it } from 'vitest';
import { readState } from './viewState.js';

describe('readState', () => {
  it('ricorda solo una scheda valida', () => {
    expect(readState({ tab: 'rules' })).toEqual({ tab: 'rules' });
    expect(readState({ tab: 'nope' })).toEqual({});
    expect(readState(undefined)).toEqual({});
    expect(readState('rules')).toEqual({});
    expect(readState({ tab: 3 })).toEqual({});
  });
});
