import type { HistoryRecord } from '@swapdeck/types';
import { describe, expect, it, vi } from 'vitest';
import { createRequestHandler, toHistoryPoints } from './bridge.js';
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
  let last: CliState = cliState(fixtureList());
  const handle = createRequestHandler({
    cli,
    lastState: () => last,
    refresh,
    onOutcome,
    now: () => 1_000_000_000,
  });
  const setLast = (s: CliState) => {
    last = s;
  };
  return { handle, calls, refresh, onOutcome, setLast };
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
    expect(calls).toEqual([]);
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
    expect(calls[1]!.args).toEqual(['action', '-']);
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
