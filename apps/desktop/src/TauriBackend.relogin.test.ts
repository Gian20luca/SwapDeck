import type { ListResponse, ReloginEvent } from '@swapdeck/types';
import { hasRelogin } from '@swapdeck/ui';
import { describe, expect, it, vi } from 'vitest';
import {
  RELOGIN_PROGRESS_EVENT,
  TauriBackend,
  type InvokeFn,
  type ListenFn,
} from './TauriBackend.js';

describe('TauriBackend: ReloginBackend', () => {
  it('mappa i comandi e inoltra gli eventi solo durante l’operazione', async () => {
    const calls: Array<{ cmd: string; args: Record<string, unknown> | undefined }> = [];
    let handler: ((event: { payload: ReloginEvent }) => void) | undefined;
    const unlisten = vi.fn();
    const listen = vi.fn(async (event: string, h: (event: { payload: ReloginEvent }) => void) => {
      expect(event).toBe(RELOGIN_PROGRESS_EVENT);
      handler = h;
      return unlisten;
    }) as unknown as ListenFn;
    const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      calls.push({ cmd, args });
      if (cmd === 'start_relogin') {
        handler?.({ payload: { event: 'stepStarted', step: 'startLogin' } });
        return { result: 'cancelled', account: 2, restored: true };
      }
      if (cmd === 'relogin_accounts_for') return [2];
      return null;
    }) as unknown as InvokeFn;
    const backend = new TauriBackend(invoke, listen);
    expect(hasRelogin(backend)).toBe(true);
    const list = { schemaVersion: 1, activeAccountNumber: null, accounts: [] } as ListResponse;
    expect(await backend.reloginAccounts(list)).toEqual([2]);
    const events: ReloginEvent[] = [];
    const outcome = await backend.startRelogin(2, (e) => events.push(e));
    expect(outcome.result).toBe('cancelled');
    expect(events).toEqual([{ event: 'stepStarted', step: 'startLogin' }]);
    expect(unlisten).toHaveBeenCalledOnce();
    await backend.submitReloginCode('abc');
    await backend.cancelRelogin();
    expect(calls.map((c) => [c.cmd, c.args])).toEqual([
      ['relogin_accounts_for', { list }],
      ['start_relogin', { account: 2 }],
      ['submit_relogin_code', { code: 'abc' }],
      ['cancel_relogin', undefined],
    ]);
  });
});
