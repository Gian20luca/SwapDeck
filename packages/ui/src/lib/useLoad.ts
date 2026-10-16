import { useCallback, useEffect, useRef, useState, type DependencyList } from 'react';
import { errorMessage } from '../backend/useAccounts.js';

export type Loadable<T> =
  { status: 'loading' } | { status: 'error'; message: string } | { status: 'ready'; data: T };

/** Carica un dato asincrono all'apertura e quando cambiano `deps`; `reload` lo rilegge senza il caricamento. */
export function useLoad<T>(
  load: () => Promise<T>,
  deps: DependencyList,
): { state: Loadable<T>; reload: () => Promise<void>; set: (data: T) => void } {
  const [state, setState] = useState<Loadable<T>>({ status: 'loading' });
  const alive = useRef(true);
  const loadRef = useRef(load);
  loadRef.current = load;

  const run = useCallback(async (keepData: boolean) => {
    if (!keepData) setState({ status: 'loading' });
    try {
      const data = await loadRef.current();
      if (alive.current) setState({ status: 'ready', data });
    } catch (error) {
      if (alive.current) setState({ status: 'error', message: errorMessage(error) });
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void run(false);
    return () => {
      alive.current = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [run, ...deps]);

  const reload = useCallback(() => run(true), [run]);
  const set = useCallback((data: T) => setState({ status: 'ready', data }), []);
  return { state, reload, set };
}
