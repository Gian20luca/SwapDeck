import { describe, expect, it } from 'vitest';
import { colorThresholds, engineView, statusModel } from './statusBar.js';
import { cliState, fixtureList, fmt, withActiveUsage } from './testing.js';

const th = { warn: 80, danger: 95 };
const time = () => '12:00';

describe('statusModel', () => {
  it("alias e percentuali dell'account attivo con livello e tooltip", () => {
    const m = statusModel(cliState(fixtureList()), th, 'off', fmt, time);
    expect(m.text).toBe('$(account) alias6 5h 66% · 7d 77%');
    expect(m.level).toBe('ok');
    expect(m.tooltip).toContain('5h: 66% (resets in 1h 44m)');
    expect(m.tooltip).toContain('7d: 77% (expected 26%)');
    expect(m.tooltip).toContain('Engine: stopped');
    expect(m.tooltip).toContain('Updated at 12:00');
  });

  it('nome della mappa attiva nel tooltip', () => {
    const state = cliState(fixtureList());
    expect(statusModel(state, th, 'off', fmt, time).tooltip).not.toContain('Map:');
    state.engine.activeMap = 'Eco';
    expect(statusModel(state, th, 'off', fmt, time).tooltip).toContain('Map: Eco');
  });

  it('il livello è il peggiore tra 5h e 7d', () => {
    const warn = statusModel(
      cliState(withActiveUsage(fixtureList(), 10, 85)),
      th,
      'off',
      fmt,
      time,
    );
    expect(warn.level).toBe('warn');
    const danger = statusModel(
      cliState(withActiveUsage(fixtureList(), 96, 10)),
      th,
      'off',
      fmt,
      time,
    );
    expect(danger.level).toBe('danger');
  });

  it('errore, caricamento e nessun account attivo', () => {
    const err = statusModel(cliState(null), th, 'other', fmt, time);
    expect(err.level).toBe('error');
    expect(err.tooltip).toContain('cswap non trovato');
    expect(err.tooltip).toContain('another instance');
    expect(statusModel(undefined, th, 'off', fmt, time).text).toContain('sync~spin');
    const none = fixtureList();
    for (const a of none.accounts) a.active = false;
    expect(statusModel(cliState(none), th, 'off', fmt, time).text).toBe('$(account) SwapDeck: —');
  });

  it('dato vecchio marcato con ~ e senza alias il numero', () => {
    const list = fixtureList();
    const active = list.accounts.find((a) => a.active)!;
    active.lastGoodUsage = active.usage!;
    active.usage = null;
    delete active.alias;
    const m = statusModel(cliState(list), th, 'child', fmt, time);
    expect(m.text).toBe('$(account) #6 5h ~66% · 7d ~77%');
    expect(m.tooltip).toContain('Last known data');
    expect(m.tooltip).toContain('this window');
  });
});

describe('soglie e motore', () => {
  it('colorThresholds usa la più bassa e la più alta valida', () => {
    expect(colorThresholds([95, 70, 85])).toEqual({ warn: 70, danger: 95 });
    expect(colorThresholds([90])).toEqual({ warn: 90, danger: 90 });
    expect(colorThresholds([])).toEqual({ warn: 80, danger: 95 });
    expect(colorThresholds([0, 150, Number.NaN])).toEqual({ warn: 80, danger: 95 });
  });

  it('engineView preferisce il processo figlio', () => {
    expect(engineView(cliState(fixtureList(), true), true)).toBe('child');
    expect(engineView(cliState(fixtureList(), true), false)).toBe('other');
    expect(engineView(undefined, false)).toBe('off');
  });
});
