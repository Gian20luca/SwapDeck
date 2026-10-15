import type { ListResponse } from '@swapdeck/types';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useBackend } from './context.js';
import { BackendError } from './types.js';

export type AccountsState =
  | { status: 'loading' }
  | { status: 'error'; message: string }
  | { status: 'ready'; data: ListResponse; refreshing: boolean; receivedAt: number };

export function errorMessage(error: unknown): string {
  if (error instanceof BackendError) return error.info.message;
  return error instanceof Error ? error.message : String(error);
}

/** Carica la lista degli account, si aggiorna agli eventi del backend e offre `refresh`. */
export function useAccounts(): { state: AccountsState; refresh: () => Promise<void> } {
  const backend = useBackend();
  const [state, setState] = useState<AccountsState>({ status: 'loading' });
  const alive = useRef(true);

  const load = useCallback(
    async (force: boolean) => {
      setState((prev) => (prev.status === 'ready' ? { ...prev, refreshing: true } : { status: 'loading' }));
      try {
        const data = await backend.listAccounts(force ? { refresh: true } : undefined);
        if (alive.current) setState({ status: 'ready', data, refreshing: false, receivedAt: Date.now() });
      } catch (error) {
        if (alive.current) setState({ status: 'error', message: errorMessage(error) });
      }
    },
    [backend],
  );

  useEffect(() => {
    alive.current = true;
    void load(false);
    const off = backend.onStateChanged((data) => {
      if (alive.current) setState({ status: 'ready', data, refreshing: false, receivedAt: Date.now() });
    });
    return () => {
      alive.current = false;
      off();
    };
  }, [backend, load]);

  const refresh = useCallback(() => load(true), [load]);
  return { state, refresh };
}
