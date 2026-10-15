import { describe, expect, it, vi } from 'vitest';
import { actions } from '../backend/actions.js';
import { BackendError } from '../backend/types.js';
import { FakeBackend } from './FakeBackend.js';
import { SCENARIO_NAMES, loadScenario } from './fixtures.js';
import { generateHistory } from './history.js';

const NOW = Date.parse('2026-10-20T12:00:00Z');

describe('fixtures', () => {
  it('sposta gli istanti in modo che «ora» coincida con la cattura', () => {
    const list = loadScenario('default', NOW);
    const delta = NOW - Date.parse('2026-10-15T19:32:50Z');
    expect(Date.parse(list.accounts[0]!.usage!.sevenDay!.resetsAt!)).toBeCloseTo(
      Date.parse('2026-10-19T16:59:59.948Z') + delta,
      -2,
    );
  });

  it('ogni scenario si carica', () => {
    for (const name of SCENARIO_NAMES) {
      expect(loadScenario(name, NOW).schemaVersion).toBe(1);
    }
  });
});

describe('FakeBackend: letture', () => {
  it('listAccounts restituisce una copia della fixture', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const a = await backend.listAccounts();
    a.accounts.length = 0;
    const b = await backend.listAccounts();
    expect(b.accounts).toHaveLength(5);
    expect(b.activeAccountNumber).toBe(6 - 0);
  });

  it('getStatus riflette l’account attivo', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const status = await backend.getStatus();
    expect(status.active).toMatchObject({ number: 6, managed: true });
    expect(status.totalManagedAccounts).toBe(5);
  });

  it('getStatus senza account attivo', async () => {
    const backend = new FakeBackend({ scenario: 'noAccounts', now: () => NOW });
    expect((await backend.getStatus()).active).toBeNull();
  });

  it('getConfig e getConfigValue', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const config = await backend.getConfig();
    expect(config.settings.length).toBeGreaterThan(0);
    const key = config.settings[0]!.key;
    expect((await backend.getConfigValue(key)).key).toBe(key);
    await expect(backend.getConfigValue('nope')).rejects.toBeInstanceOf(BackendError);
  });

  it('confirmationFor segue le regole del piano', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    expect(await backend.confirmationFor({ kind: 'purge' })).toEqual({ kind: 'typed', word: 'purge' });
    expect(await backend.confirmationFor({ kind: 'remove', account: 1 })).toEqual({ kind: 'simple' });
    expect(await backend.confirmationFor({ kind: 'disable', account: 6 })).toEqual({ kind: 'simple' });
    expect(await backend.confirmationFor({ kind: 'disable', account: 1 })).toEqual({ kind: 'none' });
    expect(await backend.confirmationFor(actions.switchTo(1))).toEqual({ kind: 'none' });
  });
});

describe('FakeBackend: switch', () => {
  it('diretto: cambia l’attivo ed emette lo stato', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const listener = vi.fn();
    backend.onStateChanged(listener);
    const out = await backend.runAction(actions.switchTo(1));
    expect(out.switch).toMatchObject({ switched: true, reason: 'switched', strategy: 'direct' });
    expect(out.switch?.from?.number).toBe(6);
    expect(out.switch?.to?.number).toBe(1);
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener.mock.calls[0]![0].activeAccountNumber).toBe(1);
    const list = await backend.listAccounts();
    expect(list.accounts.filter((a) => a.active).map((a) => a.number)).toEqual([1]);
  });

  it('diretto sull’account già attivo non cambia nulla', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const listener = vi.fn();
    backend.onStateChanged(listener);
    const out = await backend.runAction(actions.switchTo(6));
    expect(out.switch).toMatchObject({ switched: false, reason: 'already-active' });
    expect(listener).not.toHaveBeenCalled();
  });

  it('account inesistente: BackendError', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    await expect(backend.runAction(actions.switchTo(99))).rejects.toMatchObject({
      info: { errorType: 'AccountNotFoundError' },
    });
  });

  it('best sceglie l’account utilizzabile con meno consumo settimanale', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const out = await backend.runAction(actions.switchBy('best'));
    // utilizzabili: 1 (50%) e 3 (21%); 2 e 5 hanno relogin_required
    expect(out.switch).toMatchObject({ switched: true, strategy: 'best' });
    expect(out.switch?.to?.number).toBe(3);
  });

  it('next-available salta gli account non utilizzabili', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const out = await backend.runAction(actions.switchBy('next-available'));
    // dopo 6 si riparte da 1 (utilizzabile)
    expect(out.switch).toMatchObject({ switched: true, strategy: 'next-available' });
    expect(out.switch?.to?.number).toBe(1);
  });

  it('senza candidati: candidates-exhausted e nessun cambio', async () => {
    const backend = new FakeBackend({ scenario: 'tokenExpired', now: () => NOW });
    const before = (await backend.listAccounts()).activeAccountNumber;
    const out = await backend.runAction(actions.switchBy('best'));
    expect(out.switch?.switched).toBe(false);
    expect(['candidates-exhausted', 'already-best']).toContain(out.switch?.reason);
    expect((await backend.listAccounts()).activeAccountNumber).toBe(before);
  });
});

describe('FakeBackend: altre azioni', () => {
  it('disable, enable e setAlias aggiornano la lista', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    await backend.runAction({ kind: 'disable', account: 1 });
    expect((await backend.listAccounts()).accounts.find((a) => a.number === 1)?.disabled).toBe(true);
    await backend.runAction({ kind: 'enable', account: 1 });
    expect((await backend.listAccounts()).accounts.find((a) => a.number === 1)?.disabled).toBeUndefined();
    await backend.runAction({ kind: 'setAlias', account: 1, alias: 'nuovo' });
    expect((await backend.listAccounts()).accounts.find((a) => a.number === 1)?.alias).toBe('nuovo');
    await backend.runAction({ kind: 'setAlias', account: 1, alias: null });
    expect((await backend.listAccounts()).accounts.find((a) => a.number === 1)?.alias).toBeUndefined();
  });

  it('remove e swap', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    await backend.runAction({ kind: 'swap', a: 1, b: 6 });
    const list = await backend.listAccounts();
    expect(list.activeAccountNumber).toBe(1);
    expect(list.accounts.find((a) => a.number === 1)?.email).toBe('user6@example.com');
    await backend.runAction({ kind: 'remove', account: 3 });
    expect((await backend.listAccounts()).accounts.some((a) => a.number === 3)).toBe(false);
    await expect(backend.runAction({ kind: 'remove', account: 3 })).rejects.toBeInstanceOf(BackendError);
  });

  it('le azioni non simulate rispondono con un messaggio e vengono registrate', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const out = await backend.runAction({ kind: 'upgrade' });
    expect(out.message).toBeTruthy();
    expect(backend.actionLog.at(-1)).toEqual({ kind: 'upgrade' });
  });

  it('tick aggiorna il consumo dell’attivo ed emette lo stato', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const listener = vi.fn();
    const off = backend.onStateChanged(listener);
    const before = (await backend.listAccounts()).accounts.find((a) => a.active)!.usage!.fiveHour!.pct;
    backend.tick();
    const after = listener.mock.calls[0]![0].accounts.find((a: { active: boolean }) => a.active).usage.fiveHour.pct;
    expect(after).toBeGreaterThan(before);
    off();
    backend.tick();
    expect(listener).toHaveBeenCalledTimes(1);
  });
});

describe('storico sintetico', () => {
  it('copre 7 giorni a passi di 30 minuti per gli account con consumi', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const points = await backend.getHistory({ account: 6 });
    expect(points.length).toBeGreaterThanOrEqual(330);
    expect(points.length).toBeLessThanOrEqual(340);
    expect(points.every((p) => p.account === 6)).toBe(true);
    const first = points[0]!.ts;
    const last = points.at(-1)!.ts;
    expect(last - first).toBeGreaterThan(6.9 * 86400);
    expect(last * 1000).toBeLessThanOrEqual(NOW + 1000);
  });

  it('l’ultimo punto coincide col consumo attuale', async () => {
    const backend = new FakeBackend({ now: () => NOW });
    const list = await backend.listAccounts();
    const active = list.accounts.find((a) => a.active)!;
    const points = await backend.getHistory({ account: active.number });
    const last = points.at(-1)!;
    expect(last.fiveHourPct).toBeCloseTo(active.usage!.fiveHour!.pct, 0);
    expect(last.sevenDayPct).toBeCloseTo(active.usage!.sevenDay!.pct, 0);
  });

  it('valori in 0..100, ordinati nel tempo e deterministici', async () => {
    const list = loadScenario('default', NOW);
    const a = generateHistory(list, NOW);
    const b = generateHistory(list, NOW);
    expect(a).toEqual(b);
    for (let i = 0; i < a.length; i++) {
      const p = a[i]!;
      for (const v of [p.fiveHourPct, p.sevenDayPct, p.fiveHourExpectedPct, p.sevenDayExpectedPct]) {
        if (v !== null) {
          expect(v).toBeGreaterThanOrEqual(0);
          expect(v).toBeLessThanOrEqual(100);
        }
      }
      if (i > 0) expect(p.ts).toBeGreaterThanOrEqual(a[i - 1]!.ts);
    }
  });

  it('la curva del ritmo cresce linearmente dentro la finestra e si azzera al reset', () => {
    const list = loadScenario('default', NOW);
    const points = generateHistory(list, NOW).filter((p) => p.account === 6);
    const expected5 = points.map((p) => p.fiveHourExpectedPct!);
    const drops = expected5.filter((v, i) => i > 0 && v < expected5[i - 1]! - 50).length;
    expect(drops).toBeGreaterThanOrEqual(1);
  });

  it('account senza consumi noti non ha punti; dato vecchio si ferma all’ultima misura', () => {
    const list = loadScenario('stale', NOW);
    const points = generateHistory(list, NOW);
    for (const account of list.accounts) {
      const own = points.filter((p) => p.account === account.number);
      if (!account.usage && !account.lastGoodUsage) expect(own).toHaveLength(0);
      if (!account.usage && account.lastGoodFetchedAt) {
        for (const p of own) expect(p.ts * 1000).toBeLessThanOrEqual(Date.parse(account.lastGoodFetchedAt) + 1000);
      }
    }
  });
});
