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

  it('gestione: comandi della CLI, scritture da stdin e parametri validati', async () => {
    const { handle, calls } = setup();
    const invalid = { info: { kind: 'invalidUsage' } };
    await handle('listMappings', {});
    await handle('getEngineState', {});
    await handle('setEngineRules', { rules: { enabled: false, primary: 6 } });
    await handle('getPreferences', {});
    await handle('setPreferences', { prefs: { theme: 'dark' } });
    await handle('getHistoryEvents', {});
    await handle('getHistoryEvents', { sinceSeconds: 100, account: 3 });
    expect(calls).toEqual([
      { args: ['mappings'] },
      { args: ['engine', 'state'] },
      {
        args: ['engine', 'rules', 'set', '-'],
        options: { stdin: '{"enabled":false,"primary":6}' },
      },
      { args: ['preferences', 'get'] },
      { args: ['preferences', 'set', '-'], options: { stdin: '{"theme":"dark"}' } },
      { args: ['history', 'events'] },
      { args: ['history', 'events', '--since', '100', '--account', '3'] },
    ]);
    for (const rules of [undefined, null, 'x', [1]]) {
      await expect(handle('setEngineRules', { rules })).rejects.toMatchObject(invalid);
    }
    await expect(handle('setPreferences', { prefs: 1 })).rejects.toMatchObject(invalid);
    await expect(handle('getHistoryEvents', { sinceSeconds: 1.5 })).rejects.toMatchObject(invalid);
    await handle('getUsageBreakdown', { query: { window: '7d', groupBy: 'agent', account: 3 } });
    await handle('getUsageBreakdown', {
      query: { window: 'custom', groupBy: 'model', fromSeconds: 10, toSeconds: 20 },
    });
    expect(calls.slice(-2).map((c) => c.args)).toEqual([
      ['usage', 'breakdown', '--window', '7d', '--group-by', 'agent', '--account', '3'],
      [
        'usage',
        'breakdown',
        '--window',
        'custom',
        '--group-by',
        'model',
        '--from',
        '10',
        '--to',
        '20',
      ],
    ]);
    expect(calls.at(-1)?.options?.timeoutMs).toBeGreaterThan(60_000);
    const before = calls.length;
    for (const query of [
      { window: '1h', groupBy: 'project' },
      { window: '5h', groupBy: '--evil' },
      { window: 'custom', groupBy: 'project' },
      { window: '5h', groupBy: 'project', account: -1 },
    ]) {
      await expect(handle('getUsageBreakdown', { query })).rejects.toMatchObject(invalid);
    }
    expect(calls).toHaveLength(before);
    expect(calls).toHaveLength(9);
  });
});

describe('mappe', () => {
  it('elenco e operazioni via CLI, comando da stdin e validato', async () => {
    const { handle, calls } = setup();
    await handle('getEngineProfiles', {});
    await handle('applyEngineProfile', { command: { op: 'select', id: 'eco' } });
    expect(calls).toEqual([
      { args: ['engine', 'profiles'] },
      { args: ['engine', 'profiles', '-'], options: { stdin: '{"op":"select","id":"eco"}' } },
    ]);
    for (const command of [undefined, 'x', [1]]) {
      await expect(handle('applyEngineProfile', { command })).rejects.toMatchObject({
        info: { kind: 'invalidUsage' },
      });
    }
    expect(calls).toHaveLength(2);
  });
});

describe('notifiche', () => {
  it('regole lette e patch da stdin, validata', async () => {
    const { handle, calls } = setup();
    await handle('getNotificationSettings', {});
    await handle('setNotificationSettings', { patch: { switched: { enabled: false } } });
    expect(calls).toEqual([
      { args: ['notifications', 'get'] },
      {
        args: ['notifications', 'set', '-'],
        options: { stdin: '{"switched":{"enabled":false}}' },
      },
    ]);
    await expect(handle('setNotificationSettings', { patch: 'x' })).rejects.toMatchObject({
      info: { kind: 'invalidUsage' },
    });
    expect(calls).toHaveLength(2);
  });
});

describe('selettori di file e cartelle', () => {
  function withPick(result: string | null = 'C:/scelto') {
    const pick = vi.fn(async (_kind: string, _options: unknown) => result);
    const handle = createRequestHandler({
      cli: (async () => null) as CliRunner,
      lastState: () => undefined,
      refresh: async () => cliState(fixtureList()),
      onOutcome: () => undefined,
      confirm: async () => true,
      pick,
    });
    return { handle, pick };
  }

  it('inoltra il tipo di selettore e solo le opzioni valide', async () => {
    const { handle, pick } = withPick();
    const filters = [{ name: 'JSON', extensions: ['json'] }];
    await expect(handle('pickOpenFile', { title: 'T', filters, extra: 1 })).resolves.toBe(
      'C:/scelto',
    );
    expect(pick).toHaveBeenLastCalledWith('openFile', { title: 'T', filters });
    await handle('pickSaveFile', { defaultPath: 'a.json' });
    expect(pick).toHaveBeenLastCalledWith('saveFile', { defaultPath: 'a.json' });
    await handle('pickFolder', {});
    expect(pick).toHaveBeenLastCalledWith('folder', {});
  });

  it('annullato: null; opzioni non valide: invalidUsage senza aprire nulla', async () => {
    const { handle, pick } = withPick(null);
    await expect(handle('pickFolder', {})).resolves.toBeNull();
    const invalid = { info: { kind: 'invalidUsage' } };
    pick.mockClear();
    await expect(handle('pickOpenFile', { title: 1 })).rejects.toMatchObject(invalid);
    await expect(handle('pickOpenFile', { filters: 'x' })).rejects.toMatchObject(invalid);
    await expect(
      handle('pickOpenFile', { filters: [{ name: 'x', extensions: ['a/b'] }] }),
    ).rejects.toMatchObject(invalid);
    await expect(handle('pickOpenFile', { filters: [null] })).rejects.toMatchObject(invalid);
    await expect(
      handle('pickOpenFile', { filters: Array(11).fill({ name: 'x', extensions: [] }) }),
    ).rejects.toMatchObject(invalid);
    expect(pick).not.toHaveBeenCalled();
  });

  it('senza il selettore dell’host: metodo non disponibile', async () => {
    const { handle } = setup();
    await expect(handle('pickFolder', {})).rejects.toMatchObject({
      info: { kind: 'invalidUsage', message: 'selettore non disponibile' },
    });
  });
});

describe('setup di cswap', () => {
  function withSetup(confirmed: boolean, engineRunning = true) {
    const calls: Array<{ args: string[]; options?: RunOptions }> = [];
    const cli = (async (args: string[], options?: RunOptions) => {
      calls.push(options ? { args, options } : { args });
      options?.onEvent?.({ event: 'stepStarted', step: 'upgradeCswap' });
      return { args };
    }) as CliRunner;
    const order: string[] = [];
    const confirmSetup = vi.fn(async () => confirmed);
    const resume = vi.fn(() => order.push('resume'));
    const pauseEngine = vi.fn(async () => {
      order.push('pause');
      return engineRunning ? resume : () => undefined;
    });
    const refresh = vi.fn(async () => cliState(fixtureList()));
    const onChanged = vi.fn(() => order.push('changed'));
    const handle = createRequestHandler({
      cli,
      lastState: () => undefined,
      refresh,
      onOutcome: vi.fn(),
      confirm: vi.fn(async () => true),
      confirmSetup,
      pauseEngine,
      onChanged,
    });
    return { handle, calls, confirmSetup, pauseEngine, resume, refresh, order };
  }

  it('letture e chiusura dell’avviso con la versione validata', async () => {
    const { handle, calls } = withSetup(true);
    await handle('diagnoseSetup', {});
    await handle('checkCswapUpdate', {});
    await handle('checkCswapUpdate', { force: true });
    await handle('dismissCswapUpdate', { version: '0.28.0rc1' });
    expect(calls.map((c) => c.args)).toEqual([
      ['setup', 'diagnose'],
      ['setup', 'check-update'],
      ['setup', 'check-update', '--force'],
      ['setup', 'dismiss-update', '0.28.0rc1'],
    ]);
    for (const version of ['--force', '../x', 1, 'a'.repeat(80)]) {
      await expect(handle('dismissCswapUpdate', { version })).rejects.toMatchObject({
        info: { kind: 'invalidUsage' },
      });
    }
  });

  it('aggiornamento: conferma dell’host, motore fermato e riavviato, eventi inoltrati', async () => {
    const { handle, calls, confirmSetup, order, refresh } = withSetup(true);
    const events: unknown[] = [];
    await handle('upgradeCswap', {}, (e) => events.push(e));
    expect(confirmSetup).toHaveBeenCalledWith('upgrade');
    // Dopo il riavvio del motore le altre finestre vengono avvisate (sincronia).
    expect(order).toEqual(['pause', 'resume', 'changed']);
    expect(calls[0]!.args).toEqual(['setup', 'upgrade']);
    expect(events).toEqual([{ event: 'stepStarted', step: 'upgradeCswap' }]);
    expect(refresh).toHaveBeenCalled();
  });

  it('installazione: annullata dall’host non lancia nulla', async () => {
    const { handle, calls, pauseEngine } = withSetup(false);
    await expect(handle('installCswap', {})).rejects.toMatchObject({
      info: { kind: 'cancelled' },
    });
    expect(calls).toEqual([]);
    expect(pauseEngine).not.toHaveBeenCalled();
  });

  it('senza conferma dell’host i metodi che scrivono non sono disponibili', async () => {
    const { handle } = setup();
    await expect(handle('installCswap', {})).rejects.toMatchObject({
      info: { kind: 'invalidUsage' },
    });
  });
});

describe('diagnostica', () => {
  it('getDiagnostics passa il client alla CLI; cartella dei log e appunti solo se l’host li ha', async () => {
    const calls: string[][] = [];
    const cli = (async (args: string[]) => {
      calls.push(args);
      return args[0] === 'diagnostics' ? { text: 'Diagnostica di SwapDeck' } : { dir: '/tmp/logs' };
    }) as CliRunner;
    const base = {
      cli,
      lastState: () => cliState(fixtureList()),
      refresh: async () => cliState(fixtureList()),
      onOutcome: () => undefined,
      confirm: async () => true,
    };
    const bare = createRequestHandler(base);
    await expect(bare('getDiagnostics', {})).resolves.toBe('Diagnostica di SwapDeck');
    expect(calls[0]).toEqual(['diagnostics', '--json']);
    const invalid = { info: { kind: 'invalidUsage' } };
    await expect(bare('openLogsFolder', {})).rejects.toMatchObject(invalid);
    await expect(bare('copyText', { text: 'x' })).rejects.toMatchObject(invalid);

    const openFolder = vi.fn(async () => undefined);
    const copyText = vi.fn(async () => undefined);
    const full = createRequestHandler({
      ...base,
      clientInfo: 'VS Code 1.99',
      openFolder,
      copyText,
    });
    await full('getDiagnostics', {});
    expect(calls.at(-1)).toEqual(['diagnostics', '--json', '--client', 'VS Code 1.99']);
    await full('openLogsFolder', {});
    expect(openFolder).toHaveBeenCalledWith('/tmp/logs');
    await full('copyText', { text: 'abc' });
    expect(copyText).toHaveBeenCalledWith('abc');
    await expect(full('copyText', { text: 1 })).rejects.toMatchObject(invalid);
  });
});
