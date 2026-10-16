import type { SetupProgress } from '@swapdeck/types';
import { BackendError, hasSetup } from '@swapdeck/ui';
import { describe, expect, it, vi } from 'vitest';
import {
  SETUP_PROGRESS_EVENT,
  TauriBackend,
  type InvokeFn,
  type ListenFn,
} from './TauriBackend.js';

function harness(fail = false) {
  const calls: Array<{ cmd: string; args: Record<string, unknown> | undefined }> = [];
  let handler: ((event: { payload: SetupProgress }) => void) | undefined;
  const unlisten = vi.fn();
  const listen = vi.fn(async (event: string, h: (event: { payload: SetupProgress }) => void) => {
    expect(event).toBe(SETUP_PROGRESS_EVENT);
    handler = h;
    return unlisten;
  }) as unknown as ListenFn;
  const invoke = vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
    calls.push({ cmd, args });
    if (cmd === 'install_cswap' || cmd === 'upgrade_cswap') {
      handler?.({ payload: { event: 'stepStarted', step: 'installCswap' } });
      if (fail) throw { kind: 'busy', message: 'os error 32' };
      return { uvInstalled: false };
    }
    return { cmd };
  }) as unknown as InvokeFn;
  return { backend: new TauriBackend(invoke, listen), calls, unlisten };
}

describe('TauriBackend: SetupBackend', () => {
  it('offre la procedura di primo avvio e mappa i comandi', async () => {
    const { backend, calls } = harness();
    expect(hasSetup(backend)).toBe(true);
    await backend.diagnoseSetup();
    await backend.checkCswapUpdate();
    await backend.checkCswapUpdate({ force: true });
    await backend.dismissCswapUpdate('0.28.0');
    expect(calls).toEqual([
      { cmd: 'diagnose_setup', args: undefined },
      { cmd: 'check_cswap_update', args: { force: false } },
      { cmd: 'check_cswap_update', args: { force: true } },
      { cmd: 'dismiss_cswap_update', args: { version: '0.28.0' } },
    ]);
  });

  it("riceve l'avanzamento e smette di ascoltare alla fine", async () => {
    const { backend, unlisten } = harness();
    const events: SetupProgress[] = [];
    await backend.installCswap((e) => events.push(e));
    expect(events).toEqual([{ event: 'stepStarted', step: 'installCswap' }]);
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("smette di ascoltare anche se l'aggiornamento non riesce", async () => {
    const { backend, unlisten } = harness(true);
    const error = await backend.upgradeCswap(() => undefined).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(BackendError);
    expect((error as BackendError).info.kind).toBe('busy');
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
