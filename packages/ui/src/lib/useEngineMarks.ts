import type { Account, EngineState } from '@swapdeck/types';
import { useEffect, useMemo, useState } from 'react';
import { useBackend } from '../backend/context.js';
import { engineMarks, withTargetNames, type EngineMarks } from './engineMarks.js';

/**
 * Soglie del motore dal Backend; rilette a ogni aggiornamento dello stato. `null` se l'host non le
 * offre. Con `accounts` i trattini usano le soglie effettive dell'utenza attiva (F22) e portano il
 * nome dell'utenza di destinazione.
 */
export function useEngineMarks(accounts?: readonly Account[]): EngineMarks | null {
  const backend = useBackend();
  const [state, setState] = useState<EngineState | null>(null);
  useEffect(() => {
    if (!backend.getEngineState) return;
    let alive = true;
    const load = () => {
      backend.getEngineState?.().then(
        (next) => alive && setState(next),
        () => {},
      );
    };
    load();
    const off = backend.onStateChanged(load);
    return () => {
      alive = false;
      off();
    };
  }, [backend]);
  return useMemo(() => {
    if (!state) return null;
    const marks = engineMarks(state, accounts?.find((a) => a.active)?.number);
    return accounts ? withTargetNames(marks, accounts) : marks;
  }, [state, accounts]);
}
