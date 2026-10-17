import type { UsageRow } from '@swapdeck/types';
import { describe, expect, it } from 'vitest';
import { formatPct, formatTokens, prettyProject, rowLabel, shortId } from './usageView.js';

const totals = {
  tokens: { input: 0, output: 0, cacheCreation: 0, cacheRead: 0 },
  equivalent: 0,
  messages: 0,
};
const row = (extra: Partial<UsageRow>): UsageRow => ({
  key: 'k',
  totals,
  sharePct: 0,
  lastUsed: 0,
  ...extra,
});

describe('usageView', () => {
  it('prettyProject toglie l’unità o il trattino iniziale e non lascia nomi vuoti', () => {
    expect(prettyProject('c--Progetti-SwapDeck')).toBe('Progetti-SwapDeck');
    expect(prettyProject('-home-utente-app')).toBe('home-utente-app');
    expect(prettyProject('app')).toBe('app');
    expect(prettyProject('c--')).toBe('c--');
  });

  it('shortId accorcia solo gli identificativi lunghi', () => {
    expect(shortId('sess-a1')).toBe('sess-a1');
    expect(shortId('0123456789abcdef')).toBe('01234567…');
  });

  it('rowLabel per ogni raggruppamento', () => {
    const base = { project: 'c--P-app', session: 'sess-1' };
    expect(rowLabel(row(base), 'project', 'Princ.')).toEqual({ title: 'P-app' });
    expect(rowLabel(row(base), 'session', 'Princ.')).toEqual({ title: 'sess-1', sub: 'P-app' });
    expect(rowLabel(row({ model: 'm' }), 'model', 'Princ.')).toEqual({ title: 'm' });
    expect(rowLabel(row(base), 'agent', 'Princ.')).toEqual({
      title: 'Princ.',
      sub: 'P-app / sess-1',
    });
    expect(
      rowLabel(
        row({ ...base, agent: 'a1', agentType: 'Explore', description: 'Cerca' }),
        'agent',
        'Princ.',
      ),
    ).toEqual({ title: 'Explore', sub: 'Cerca · P-app / sess-1' });
    expect(rowLabel(row({ agent: 'a1' }), 'agent', 'Princ.')).toEqual({ title: 'a1' });
  });

  it('formati numerici nella lingua scelta', () => {
    expect(formatTokens(850, 'it')).toBe('850');
    expect(formatTokens(1_234_567, 'en')).toBe('1.2M');
    expect(formatPct(11.46, 'it')).toBe('11,5');
    expect(formatPct(11.46, 'en')).toBe('11.5');
  });
});
