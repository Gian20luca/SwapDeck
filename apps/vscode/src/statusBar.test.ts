import type { IdeStatus, IdeWindow } from '@swapdeck/types';
import { describe, expect, it } from 'vitest';
import { accountLabel, errorStatus, statusArgs, statusModel } from './statusBar.js';
import { fmt } from './testing.js';

// Livelli, ultimo dato noto, titolare del motore e soglie: test nel Rust (`ide.rs`).
const time = () => '12:00';

const window = (pct: number | null, extra: Partial<IdeWindow> = {}): IdeWindow => ({
  pct,
  level: 'ok',
  countdown: null,
  resetsAt: null,
  resetsInSecs: null,
  expectedPct: null,
  ...extra,
});

function summary(over: Partial<IdeStatus> = {}): IdeStatus {
  return {
    level: 'ok',
    content: {
      kind: 'account',
      account: { number: 6, alias: 'alias6', shortEmail: 'user6@example.com' },
      fiveHour: window(66, { countdown: '1h 44m' }),
      sevenDay: window(77, { expectedPct: 26 }),
      lastKnown: false,
    },
    engine: { holder: 'off', pid: null, switching: false, dryRun: false, activeMap: null },
    updatedAtMs: 0,
    thresholds: { warn: 80, danger: 95 },
    ...over,
  };
}

describe('statusModel', () => {
  it("alias e percentuali dell'account attivo con livello e tooltip", () => {
    const m = statusModel(summary(), fmt, time);
    expect(m.text).toBe('$(account) alias6 5h 66% · 7d 77%');
    expect(m.level).toBe('ok');
    expect(m.tooltip).toContain('SwapDeck — alias6 (user6@example.com)');
    expect(m.tooltip).toContain('5h: 66% (resets in 1h 44m)');
    expect(m.tooltip).toContain('7d: 77% (expected 26%)');
    expect(m.tooltip).toContain('Engine: stopped · automatic switching off');
    expect(m.tooltip).toContain('Updated at 12:00');
    expect(m.tooltip).not.toContain('Map:');
  });

  it('livello dal Rust, mappa attiva e motore in questa finestra', () => {
    const m = statusModel(
      summary({
        level: 'danger',
        engine: { holder: 'here', pid: 7, switching: true, dryRun: false, activeMap: 'Eco' },
      }),
      fmt,
      time,
    );
    expect(m.level).toBe('danger');
    expect(m.tooltip).toContain('Map: Eco');
    expect(m.tooltip).toContain('Engine: running in this window · automatic switching on');
  });

  it('errore, caricamento e nessun account attivo', () => {
    const err = statusModel(
      summary({
        level: 'error',
        content: { kind: 'error', error: { kind: 'notInstalled', message: 'cswap non trovato' } },
        engine: { holder: 'other', pid: 42, switching: false, dryRun: false, activeMap: null },
      }),
      fmt,
      time,
    );
    expect(err.level).toBe('error');
    expect(err.text).toBe('$(warning) SwapDeck');
    expect(err.tooltip).toContain('cswap non trovato');
    expect(err.tooltip).toContain('another window or instance (PID 42)');
    const unknown = statusModel(
      summary({ level: 'error', content: { kind: 'error', error: null } }),
      fmt,
      time,
    );
    expect(unknown.tooltip).toContain('unknown error');
    expect(statusModel(undefined, fmt, time).text).toContain('sync~spin');
    const none = statusModel(
      summary({ level: 'unknown', content: { kind: 'noAccount' } }),
      fmt,
      time,
    );
    expect(none.text).toBe('$(account) SwapDeck: —');
    expect(none.tooltip).toContain('No active account');
  });

  it('dato vecchio marcato con ~, senza alias il numero, percentuali assenti', () => {
    const m = statusModel(
      summary({
        content: {
          kind: 'account',
          account: { number: 6, alias: null, shortEmail: 'u…@example.com' },
          fiveHour: window(66),
          sevenDay: window(null),
          lastKnown: true,
        },
        engine: { holder: 'other', pid: null, switching: false, dryRun: false, activeMap: null },
      }),
      fmt,
      time,
    );
    expect(m.text).toBe('$(account) #6 5h ~66% · 7d ~—');
    expect(m.tooltip).toContain('5h: 66%\n');
    expect(m.tooltip).toContain('7d: —');
    expect(m.tooltip).toContain('Last known data');
    expect(m.tooltip).toContain('Engine: running in another window or instance ·');
  });
});

describe('argomenti e ripieghi', () => {
  it('statusArgs: soglie, PID del motore di questa finestra e stato da stdin', () => {
    expect(statusArgs([80, 95], 4242)).toEqual([
      'ide',
      'status',
      '--thresholds',
      '80,95',
      '--engine-pid',
      '4242',
      '-',
    ]);
    expect(statusArgs(undefined, undefined)).toEqual(['ide', 'status', '--thresholds', '', '-']);
  });

  it('errorStatus: la CLI non risponde', () => {
    const m = statusModel(errorStatus({ kind: 'failed', message: 'uscita 1' }), fmt, time);
    expect(m.level).toBe('error');
    expect(m.tooltip).toContain('uscita 1');
    expect(m.tooltip).toContain('Engine: stopped');
  });

  it('accountLabel', () => {
    expect(accountLabel({ number: 3, alias: 'x' })).toBe('x');
    expect(accountLabel({ number: 3, alias: '' })).toBe('#3');
    expect(accountLabel({ number: 3 })).toBe('#3');
  });
});
