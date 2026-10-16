import { useEffect, useState } from 'react';
import { useBackend } from '../backend/context.js';
import { engineMarks, type EngineMarks } from './engineMarks.js';

/** Soglie del motore dal Backend; rilette a ogni aggiornamento dello stato. `null` se l'host non le offre. */
export function useEngineMarks(): EngineMarks | null {
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
  return marks;
}
