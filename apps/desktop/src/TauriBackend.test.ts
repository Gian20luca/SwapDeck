import type { ListResponse } from '@swapdeck/types';
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
