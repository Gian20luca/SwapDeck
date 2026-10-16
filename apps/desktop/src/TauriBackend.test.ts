import type { EngineRules, ListResponse } from '@swapdeck/types';
import { BackendError } from '@swapdeck/ui';
import { describe, expect, it, vi } from 'vitest';
import list from '../../../fixtures/cswap/list.json';
import {
  STATE_EVENT,
  TauriBackend,
  toBackendError,
  type InvokeFn,
  type ListenFn,
  type Snapshot,
} from './TauriBackend.js';

const fixture = list as unknown as ListResponse;

function setup(responses: Record<string, unknown> = {}) {
  const calls: Array<{ cmd: string; args: Record<string, unknown> | undefined }> = [];
  const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    calls.push({ cmd, args });
    const value = responses[cmd];
    if (value instanceof Error || (typeof value === 'object' && value && 'reject' in value)) {
      throw (value as { reject?: unknown }).reject ?? value;
    }
    return value;
  }) as unknown as InvokeFn;
  const handlers: Array<(event: { payload: Snapshot }) => void> = [];
  const unlisten = vi.fn();
  const listen = vi.fn(async (event: string, handler: (event: { payload: Snapshot }) => void) => {
    expect(event).toBe(STATE_EVENT);
    handlers.push(handler);
    return unlisten;
  }) as unknown as ListenFn;
  const emit = (snap: Partial<Snapshot>) =>
    handlers.forEach((h) =>
      h({
        payload: {
          list: null,
          error: null,
          updatedAtMs: 1,
          engineSuspended: false,
          autoRunning: false,
          engine: null,
          ...snap,
        },
      }),
    );
  return { backend: new TauriBackend(invoke, listen), calls, emit, unlisten, listen };
}

describe('TauriBackend', () => {
  it('maps every Backend method to its command and arguments', async () => {
    const { backend, calls } = setup({
      list_accounts: fixture,
      confirmation_for: { kind: 'simple' },
      get_history: [],
    });
    await expect(backend.listAccounts()).resolves.toBe(fixture);
    await backend.listAccounts({ refresh: true });
    await backend.getStatus();
    await backend.getConfig();
    await backend.getConfigValue('autoSwitch');
    await backend.getHistory({ account: 3 });
    await backend.getHistory();
    await expect(backend.confirmationFor({ kind: 'remove', account: 1 })).resolves.toEqual({
      kind: 'simple',
    });
    expect(calls).toEqual([
      { cmd: 'list_accounts', args: { refresh: false } },
      { cmd: 'list_accounts', args: { refresh: true } },
      { cmd: 'get_status', args: undefined },
      { cmd: 'get_config', args: undefined },
      { cmd: 'get_config_value', args: { key: 'autoSwitch' } },
      { cmd: 'get_history', args: { query: { account: 3 } } },
      { cmd: 'get_history', args: { query: {} } },
      { cmd: 'confirmation_for', args: { action: { kind: 'remove', account: 1 } } },
    ]);
  });

  it('keeps only switch and message from the Rust outcome', async () => {
    const switched = { switched: true, message: 'ok' };
    const { backend, calls } = setup({
      run_action: { message: 'fatto', list: fixture, switch: switched, command: { program: 'x' } },
    });
    const outcome = await backend.runAction({ kind: 'switch', target: 3 });
    expect(outcome).toEqual({ message: 'fatto', switch: switched });
    expect(calls[0]).toEqual({
      cmd: 'run_action',
      args: { action: { kind: 'switch', target: 3 } },
    });
    const plain = setup({ run_action: { message: 'alias impostato' } });
    const warned = setup({ run_action: { message: 'cambiato', warning: 'non verificato' } });
    await expect(warned.backend.runAction({ kind: 'switch', target: 2 })).resolves.toEqual({
      message: 'cambiato — non verificato',
    });
    await expect(plain.backend.runAction({ kind: 'enable', account: 2 })).resolves.toEqual({
      message: 'alias impostato',
    });
  });

  it('rejects with BackendError carrying the Rust CswapError', async () => {
    const info = { kind: 'rateLimited', message: '429 dai consumi', exitCode: 1 };
    const { backend } = setup({ list_accounts: { reject: info } });
    const error = await backend.listAccounts().catch((e: unknown) => e);
    expect(error).toBeInstanceOf(BackendError);
    expect((error as BackendError).info).toEqual(info);
  });

  it('wraps unknown rejections as failed', () => {
    expect(toBackendError('comando non permesso').info).toEqual({
      kind: 'failed',
      message: 'comando non permesso',
    });
    expect(toBackendError(new Error('rotto')).info.message).toBe('rotto');
    const same = new BackendError({ kind: 'timeout', message: 't' });
    expect(toBackendError(same)).toBe(same);
  });

  it('forwards only snapshots with a list and unsubscribes', async () => {
    const { backend, emit, unlisten } = setup();
    const seen: ListResponse[] = [];
    const snaps: Snapshot[] = [];
    const off = backend.onStateChanged((l) => seen.push(l));
    const offSnap = backend.onSnapshot((s) => snaps.push(s));
    await Promise.resolve();
    emit({ list: fixture });
    emit({ error: { kind: 'timeout', message: 'lento' } });
    expect(seen).toEqual([fixture]);
    expect(snaps).toHaveLength(2);
    off();
    offSnap();
    expect(unlisten).toHaveBeenCalledTimes(2);
    emit({ list: fixture });
    expect(seen).toHaveLength(1);
  });

  it('unlistens when unsubscribed before listen resolves', async () => {
    const { backend, unlisten } = setup();
    const off = backend.onStateChanged(() => undefined);
    off();
    await new Promise((r) => setTimeout(r, 0));
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it('calls the app commands', async () => {
    const settings = { autostart: false, autostartManaged: true, version: '0.1.0' };
    const { backend, calls } = setup({ get_app_settings: settings, set_autostart: settings });
    await expect(backend.getAppSettings()).resolves.toEqual(settings);
    await backend.setAutostart(false);
    await backend.showMain();
    expect(calls.map((c) => [c.cmd, c.args])).toEqual([
      ['get_app_settings', undefined],
      ['set_autostart', { enabled: false }],
      ['show_main', undefined],
    ]);
  });
});

describe('TauriBackend: finestre', () => {
  function withWindow(label: string) {
    const { backend: _unused, calls } = setup({ mini_is_compact: true });
    const setTheme = vi.fn(async () => undefined);
    const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      calls.push({ cmd, args });
      return cmd === 'mini_is_compact' ? true : undefined;
    }) as unknown as InvokeFn;
    const listen = vi.fn(async () => () => undefined) as unknown as ListenFn;
    return {
      backend: new TauriBackend(invoke, listen, undefined, { label, setTheme }),
      calls,
      setTheme,
    };
  }

  it('la finestra mini ha i controlli, mappati sui comandi del Rust', async () => {
    const { backend, calls } = withWindow('mini');
    const win = backend.miniWindow!;
    await win.hide();
    await win.setCompact(true);
    await expect(win.isCompact()).resolves.toBe(true);
    await win.fitHeight!(212);
    expect(calls.map((c) => [c.cmd, c.args])).toEqual([
      ['mini_hide', undefined],
      ['mini_set_compact', { compact: true }],
      ['mini_is_compact', undefined],
      ['mini_fit_height', { height: 212 }],
    ]);
  });

  it('la finestra principale non ha i controlli del mini-widget', () => {
    expect(withWindow('main').backend.miniWindow).toBeUndefined();
  });

  it('il tema della cornice passa a setTheme della finestra (null = sistema)', async () => {
    const { backend, setTheme } = withWindow('main');
    await backend.setWindowTheme!('dark');
    await backend.setWindowTheme!(null);
    expect(setTheme.mock.calls).toEqual([['dark'], [null]]);
  });

  it('senza la finestra (test, anteprima) il metodo non c’è', () => {
    expect(setup().backend.setWindowTheme).toBeUndefined();
    expect(setup().backend.miniWindow).toBeUndefined();
  });
});

describe('TauriBackend: gestione completa', () => {
  it('mappa i metodi di gestione sui comandi del Rust', async () => {
    const { backend, calls } = setup({});
    const rules = { enabled: true } as unknown as EngineRules;
    await backend.listMappings();
    await backend.getEngineState();
    await backend.setEngineRules(rules);
    await backend.getPreferences();
    await backend.setPreferences({ pollIntervalSeconds: 30 });
    await backend.getHistoryEvents({ sinceSeconds: 5 });
    await backend.getHistoryEvents();
    await backend.getUsageBreakdown({ window: '7d', groupBy: 'agent', account: 2 });
    expect(calls.map((c) => [c.cmd, c.args])).toEqual([
      ['list_mappings', undefined],
      ['get_engine_state', undefined],
      ['set_engine_rules', { rules }],
      ['get_preferences', undefined],
      ['set_preferences', { prefs: { pollIntervalSeconds: 30 } }],
      ['get_history_events', { query: { sinceSeconds: 5 } }],
      ['get_history_events', { query: {} }],
      ['get_usage_breakdown', { query: { window: '7d', groupBy: 'agent', account: 2 } }],
    ]);
  });

  it('i selettori usano tauri-plugin-dialog e restituiscono null se annullati', async () => {
    const open = vi.fn(async (_o: unknown) => 'C:/scelto');
    const save = vi.fn(async (_o: unknown) => null);
    const invoke = vi.fn() as unknown as InvokeFn;
    const listen = vi.fn() as unknown as ListenFn;
    const backend = new TauriBackend(invoke, listen, { open, save });
    expect(await backend.pickFolder({ title: 'Cartella' })).toBe('C:/scelto');
    expect(open).toHaveBeenLastCalledWith({ title: 'Cartella', directory: true, multiple: false });
    expect(
      await backend.pickOpenFile({
        defaultPath: 'a.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      }),
    ).toBe('C:/scelto');
    expect(open).toHaveBeenLastCalledWith({
      defaultPath: 'a.json',
      filters: [{ name: 'JSON', extensions: ['json'] }],
      multiple: false,
    });
    expect(await backend.pickSaveFile({ title: 'Salva' })).toBeNull();
    expect(save).toHaveBeenCalledWith({ title: 'Salva' });
    expect(invoke).not.toHaveBeenCalled();
  });

  it('senza il plugin i selettori rifiutano con BackendError', async () => {
    const { backend } = setup({});
    await expect(backend.pickFolder()).rejects.toBeInstanceOf(BackendError);
  });
});
