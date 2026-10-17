import { describe, expect, it } from 'vitest';
import { colorThresholds, engineView, statusModel } from './statusBar.js';
import { cliState, fixtureList, fmt, withActiveUsage } from './testing.js';

const th = { warn: 80, danger: 95 };
const time = () => '12:00';
const OFF = { kind: 'off' } as const;

describe('statusModel', () => {
  it("alias e percentuali dell'account attivo con livello e tooltip", () => {
    const m = statusModel(cliState(fixtureList()), th, OFF, fmt, time);
    expect(m.text).toBe('$(account) alias6 5h 66% · 7d 77%');
    expect(m.level).toBe('ok');
    expect(m.tooltip).toContain('5h: 66% (resets in 1h 44m)');
    expect(m.tooltip).toContain('7d: 77% (expected 26%)');
    expect(m.tooltip).toContain('Engine: stopped · automatic switching off');
    expect(m.tooltip).toContain('Updated at 12:00');
  });

  it('nome della mappa attiva nel tooltip', () => {
    const state = cliState(fixtureList());
    expect(statusModel(state, th, OFF, fmt, time).tooltip).not.toContain('Map:');
    state.engine.activeMap = 'Eco';
    expect(statusModel(state, th, OFF, fmt, time).tooltip).toContain('Map: Eco');
  });

  it('il livello è il peggiore tra 5h e 7d', () => {
    const warn = statusModel(cliState(withActiveUsage(fixtureList(), 10, 85)), th, OFF, fmt, time);
    expect(warn.level).toBe('warn');
    const danger = statusModel(
      cliState(withActiveUsage(fixtureList(), 96, 10)),
      th,
      OFF,
      fmt,
      time,
    );
    expect(danger.level).toBe('danger');
  });

  it('errore, caricamento e nessun account attivo', () => {
    const err = statusModel(cliState(null), th, { kind: 'other', pid: 42 }, fmt, time);
    expect(err.level).toBe('error');
    expect(err.tooltip).toContain('cswap non trovato');
    expect(err.tooltip).toContain('another window or instance (PID 42)');
    expect(statusModel(undefined, th, OFF, fmt, time).text).toContain('sync~spin');
    const none = fixtureList();
    for (const a of none.accounts) a.active = false;
    expect(statusModel(cliState(none), th, OFF, fmt, time).text).toBe('$(account) SwapDeck: —');
  });

  it('dato vecchio marcato con ~ e senza alias il numero', () => {
    const list = fixtureList();
    const active = list.accounts.find((a) => a.active)!;
    active.lastGoodUsage = active.usage!;
    active.usage = null;
    delete active.alias;
    const m = statusModel(cliState(list), th, { kind: 'here' }, fmt, time);
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

  it('engineView: titolare del lock per PID, uguale in ogni finestra tranne la riga del tooltip', () => {
    const state = cliState(fixtureList(), true);
    state.engine.lock = {
      pid: 4242,
      host: 'pc',
      instance: 'x',
      processStart: null,
      acquiredAt: 0,
      heartbeatAt: 0,
      intervalSecs: 60,
    };
    expect(engineView(state, 4242)).toEqual({ kind: 'here' });
    // Processo figlio vivo ma lock di un'altra finestra: questa osserva.
    expect(engineView(state, 99)).toEqual({ kind: 'other', pid: 4242 });
    expect(engineView(state, undefined)).toEqual({ kind: 'other', pid: 4242 });
    expect(engineView(cliState(fixtureList(), false), 4242)).toEqual({ kind: 'off' });
    expect(engineView(undefined, undefined)).toEqual({ kind: 'off' });
    state.engine.enabled = true;
    const here = statusModel(state, th, { kind: 'here' }, fmt, time);
    const other = statusModel(state, th, { kind: 'other', pid: 4242 }, fmt, time);
    expect(here.text).toBe(other.text);
    expect(here.level).toBe(other.level);
    expect(here.tooltip).toContain('automatic switching on');
  });
});
