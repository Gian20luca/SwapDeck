import type { HistoryRecord } from '@swapdeck/types';
import { describe, expect, it, vi } from 'vitest';
import { createRequestHandler, describeAction, toHistoryPoints } from './bridge.js';
import type { CliRunner, RunOptions } from './cli.js';
import type { CliState } from './generated/index.js';
import { cliState, fixtureList } from './testing.js';

function setup(responses: Record<string, unknown> = {}) {
  const calls: Array<{ args: string[]; options?: RunOptions }> = [];
  const cli = (async (args: string[], options?: RunOptions) => {
    calls.push(options ? { args, options } : { args });
    return responses[args[0]!] ?? null;
  }) as CliRunner;
  const refresh = vi.fn(async () => cliState(fixtureList()));
  const onOutcome = vi.fn();
  const confirm = vi.fn(async () => true);
  let last: CliState = cliState(fixtureList());
  const handle = createRequestHandler({
    cli,
    lastState: () => last,
    refresh,
    onOutcome,
    confirm,
    now: () => 1_000_000_000,
  });
  const setLast = (s: CliState) => {
    last = s;
  };
  return { handle, calls, refresh, onOutcome, confirm, setLast };
}

describe('createRequestHandler', () => {
  it("listAccounts usa l'ultimo stato, con refresh rilegge", async () => {
    const { handle, refresh } = setup();
    await handle('listAccounts', {});
    expect(refresh).not.toHaveBeenCalled();
    await handle('listAccounts', { refresh: true });
    expect(refresh).toHaveBeenCalledOnce();
  });

  it("listAccounts senza lista rifiuta con l'errore dello stato", async () => {
    const { handle, setLast, refresh } = setup();
    setLast(cliState(null));
    refresh.mockResolvedValueOnce(cliState(null));
    await expect(handle('listAccounts', {})).rejects.toMatchObject({
      info: { kind: 'notInstalled' },
    });
  });

  it('metodi e parametri non validi', async () => {
    const { handle, calls } = setup();
    const invalid = { info: { kind: 'invalidUsage' } };
    await expect(handle('rm', {})).rejects.toMatchObject(invalid);
    await expect(handle('getConfigValue', { key: 'a b' })).rejects.toMatchObject(invalid);
    await expect(handle('getHistory', { account: -1 })).rejects.toMatchObject(invalid);
    await expect(handle('runAction', { action: 'switch' })).rejects.toMatchObject(invalid);
    await expect(handle('runAction', { action: { kind: 'rmrf' } })).rejects.toMatchObject(invalid);
    await expect(handle('confirmationFor', { action: { kind: 1 } })).rejects.toMatchObject(invalid);
    for (const claudeArgs of [['--dangerously-skip-permissions'], ['--continue', 1], 'x']) {
      const action = { kind: 'run', options: { claudeArgs } };
      await expect(handle('runAction', { action })).rejects.toMatchObject(invalid);
    }
    expect(calls).toEqual([]);
  });

  it("run: claudeArgs vuoti o dell'elenco chiuso", async () => {
    const { handle, calls } = setup({ action: {}, confirm: { kind: 'none' } });
    await handle('runAction', { action: { kind: 'run', options: {} } });
    await handle('runAction', { action: { kind: 'run', options: { claudeArgs: ['--continue'] } } });
    expect(calls.filter((c) => c.args[0] === 'action')).toHaveLength(2);
  });

  it("la conferma dovuta la chiede l'host; se annullata l'azione non parte", async () => {
    const needed = { kind: 'typed', word: 'purge' };
    const { handle, calls, confirm, onOutcome } = setup({ action: {}, confirm: needed });
    confirm.mockResolvedValueOnce(false);
    const action = { kind: 'purge' };
    await expect(handle('runAction', { action })).rejects.toMatchObject({
      info: { kind: 'cancelled' },
    });
    expect(confirm).toHaveBeenCalledWith(action, needed);
    expect(calls.map((c) => c.args[0])).toEqual(['confirm']);
    expect(onOutcome).not.toHaveBeenCalled();
    await handle('runAction', { action });
    expect(calls.map((c) => c.args[0])).toEqual(['confirm', 'confirm', 'action']);
    expect(onOutcome).toHaveBeenCalledOnce();
  });

  it('describeAction: testo breve, mai il token', () => {
    expect(describeAction({ kind: 'remove', account: 3 })).toBe('remove #3');
    expect(describeAction({ kind: 'swap', a: 1, b: 2 })).toBe('swap #1 ↔ #2');
    expect(describeAction({ kind: 'move', account: 1, slot: 4 })).toBe('move #1 → 4');
    expect(describeAction({ kind: 'map', account: 1, path: '/p' })).toBe('map #1 /p');
    expect(describeAction({ kind: 'unmap', path: '/p' })).toBe('unmap /p');
    expect(describeAction({ kind: 'export', path: 'f' })).toBe('export → f');
    expect(describeAction({ kind: 'export', path: 'f', account: 2 })).toBe('export #2 → f');
    expect(describeAction({ kind: 'unclaimedPurge', id: 'x' })).toBe('unclaimed --purge x');
    expect(describeAction({ kind: 'configUnset', key: 'k' })).toBe('configUnset k');
    expect(describeAction({ kind: 'addToken', token: 'segreto' })).toBe('addToken');
  });

  it("azioni e conferme passano da stdin, con l'account attivo noto", async () => {
    const outcome = { message: 'ok' };
    const { handle, calls, onOutcome } = setup({ action: outcome, confirm: { kind: 'none' } });
    const action = { kind: 'addToken', token: 'segreto' };
    await handle('confirmationFor', { action });
    await expect(handle('runAction', { action })).resolves.toEqual(outcome);
    expect(calls[0]).toEqual({
      args: ['confirm', '-', '--active', '6'],
      options: { stdin: JSON.stringify(action) },
    });
    expect(calls[1]!.args).toEqual(calls[0]!.args);
    expect(calls[2]!.args).toEqual(['action', '-']);
    expect(calls.flatMap((c) => c.args).join(' ')).not.toContain('segreto');
    expect(onOutcome).toHaveBeenCalledWith(outcome);
  });

  it('getHistory: intervallo di default di 7 giorni e filtro', async () => {
    const sample: HistoryRecord = {
      type: 'sample',
      ts: 1,
      account: 3,
      active: true,
      status: 'ok',
      fiveHourPct: 10,
      sevenDayPct: 20,
      fiveHourResetsAt: null,
      sevenDayResetsAt: null,
      sevenDayExpectedPct: 15,
      fetchedAt: null,
    };
    const other = { type: 'switch', ts: 2, from: 3, to: 6 } as unknown as HistoryRecord;
    const records = [sample, other];
    const { handle, calls } = setup({ history: records });
    const points = await handle('getHistory', { account: 3 });
    const from = String(1_000_000 - 7 * 86400);
    expect(calls[0]!.args).toEqual([
      'history',
      '--from',
      from,
      '--to',
      '1000000',
      '--account',
      '3',
    ]);
    expect(points).toEqual(toHistoryPoints(records));
    expect(points).toEqual([
      {
        ts: 1,
        account: 3,
        fiveHourPct: 10,
        sevenDayPct: 20,
        fiveHourExpectedPct: null,
        sevenDayExpectedPct: 15,
      },
    ]);
  });

  it('letture di cswap', async () => {
    const { handle, calls } = setup();
    await handle('getStatus', {});
    await handle('getConfig', undefined);
    await handle('getConfigValue', { key: 'auto.threshold' });
    expect(calls.map((c) => c.args)).toEqual([
      ['status'],
      ['cswap-config'],
      ['cswap-config', 'auto.threshold'],
    ]);
  });
});
