import { useCallback, useReducer } from 'react';
import type { CswapError, SetupOutcome, SetupProgress } from '@swapdeck/types';
import { BackendError } from '../backend/types.js';
import { EMPTY_RUN, reduceProgress, type RunState } from './setup.js';

type Action =
  | { type: 'reset' }
  | { type: 'event'; event: SetupProgress }
  | { type: 'error'; error: CswapError };

function reducer(state: RunState, action: Action): RunState {
  switch (action.type) {
    case 'reset':
      return EMPTY_RUN;
    case 'event':
      return reduceProgress(state, action.event);
    case 'error':
      return state.error ? state : { ...state, error: action.error };
  }
}

function toCswapError(error: unknown): CswapError {
  if (error instanceof BackendError) return error.info;
  return { kind: 'failed', message: error instanceof Error ? error.message : String(error) };
}

/**
 * Stato di un'installazione o di un aggiornamento: `start` azzera, passa gli eventi al reducer e
 * restituisce l'esito, oppure `null` con l'errore in `run.error`.
 */
export function useSetupRun() {
  const [run, dispatch] = useReducer(reducer, EMPTY_RUN);
  const start = useCallback(
    async (
      operation: (onProgress: (event: SetupProgress) => void) => Promise<SetupOutcome>,
    ): Promise<SetupOutcome | null> => {
      dispatch({ type: 'reset' });
      try {
        return await operation((event) => dispatch({ type: 'event', event }));
      } catch (error) {
        dispatch({ type: 'error', error: toCswapError(error) });
        return null;
      }
    },
    [],
  );
  return { run, start };
}
