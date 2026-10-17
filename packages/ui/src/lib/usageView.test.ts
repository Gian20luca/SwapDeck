import type { UsageRow } from '@swapdeck/types';
import { describe, expect, it } from 'vitest';
import { formatPct, formatTokens, rowLabel, shortId } from './usageView.js';

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
  it('shortId accorcia solo gli identificativi lunghi', () => {
    expect(shortId('sess-a1')).toBe('sess-a1');
    expect(shortId('0123456789abcdef')).toBe('01234567…');
  });

  it('rowLabel per ogni raggruppamento', () => {
    const base = { project: 'c--P-my-app', projectLabel: 'my-app', session: 'sess-1' };
    expect(rowLabel(row(base), 'project', 'Princ.')).toEqual({ title: 'my-app' });
    expect(rowLabel(row(base), 'session', 'Princ.')).toEqual({ title: 'sess-1', sub: 'my-app' });
    expect(rowLabel(row({ model: 'm' }), 'model', 'Princ.')).toEqual({ title: 'm' });
    expect(rowLabel(row(base), 'agent', 'Princ.')).toEqual({
      title: 'Princ.',
      sub: 'my-app / sess-1',
    });
    expect(
      rowLabel(
        row({ ...base, agent: 'a1', agentType: 'Explore', description: 'Cerca' }),
        'agent',
        'Princ.',
      ),
    ).toEqual({ title: 'Explore', sub: 'Cerca · my-app / sess-1' });
    expect(rowLabel(row({ agent: 'a1' }), 'agent', 'Princ.')).toEqual({ title: 'a1' });
    // Mai il percorso intero: senza projectLabel non si ricade su project né sulla chiave.
    expect(rowLabel(row({ key: 'c--P-x', project: 'c--P-x' }), 'project', 'Princ.')).toEqual({
      title: '?',
    });
    expect(rowLabel(row({ project: 'c--P-x', session: 's' }), 'session', 'Princ.')).toEqual({
      title: 's',
    });
  });

  it('formati numerici nella lingua scelta', () => {
    expect(formatTokens(850, 'it')).toBe('850');
    expect(formatTokens(1_234_567, 'en')).toBe('1.2M');
    expect(formatPct(11.46, 'it')).toBe('11,5');
    expect(formatPct(11.46, 'en')).toBe('11.5');
  });
});
