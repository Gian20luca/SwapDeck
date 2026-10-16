import type { Account } from '@swapdeck/types';
import { useEffect, useMemo, useState } from 'react';
import { useBackend } from '../backend/context.js';
import { engineMarks, withTargetNames, type EngineMarks } from './engineMarks.js';

/**
 * Soglie del motore dal Backend; rilette a ogni aggiornamento dello stato. `null` se l'host non le
 * offre. Con `accounts` i trattini portano anche il nome dell'utenza di destinazione.
 */
export function useEngineMarks(accounts?: readonly Account[]): EngineMarks | null {
  const backend = useBackend();
  const [marks, setMarks] = useState<EngineMarks | null>(null);
  useEffect(() => {
    if (!backend.getEngineState) return;
    let alive = true;
    const load = () => {
      backend.getEngineState?.().then(
        (state) => alive && setMarks(engineMarks(state)),
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
  return useMemo(
    () => (marks && accounts ? withTargetNames(marks, accounts) : marks),
    [marks, accounts],
  );
}
