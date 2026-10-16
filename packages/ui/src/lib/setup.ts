import type { CswapError, SetupProgress, SetupStep } from '@swapdeck/types';
import type { MessageKey } from '../i18n/index.js';

/** Stato di un passo mostrato nell'avanzamento. */
export type StepState = 'pending' | 'running' | 'done' | 'skipped' | 'failed';

export interface RunState {
  steps: Partial<Record<SetupStep, StepState>>;
  /** Output dei processi (ultime `MAX_LOG_LINES` righe). */
  lines: string[];
  error?: CswapError;
}

/** Passi dell'installazione e dell'aggiornamento, nell'ordine in cui il Rust li esegue. */
export const INSTALL_STEPS: readonly SetupStep[] = [
  'downloadUv',
  'installUv',
  'installCswap',
  'verifyCswap',
];
export const UPGRADE_STEPS: readonly SetupStep[] = ['upgradeCswap', 'verifyCswap'];

/** Righe di log conservate: le più vecchie si scartano. */
export const MAX_LOG_LINES = 400;

export const EMPTY_RUN: RunState = { steps: {}, lines: [] };

/** Applica un evento di avanzamento allo stato (funzione pura, per `useReducer`). */
export function reduceProgress(state: RunState, event: SetupProgress): RunState {
  switch (event.event) {
    case 'stepStarted':
      return { ...state, steps: { ...state.steps, [event.step]: 'running' } };
    case 'stepDone':
      return { ...state, steps: { ...state.steps, [event.step]: 'done' } };
    case 'stepSkipped':
      return { ...state, steps: { ...state.steps, [event.step]: 'skipped' } };
    case 'stepFailed':
      return {
        ...state,
        steps: { ...state.steps, [event.step]: 'failed' },
        error: event.error,
      };
    case 'line': {
      const lines = [...state.lines, event.text];
      return {
        ...state,
        lines: lines.length > MAX_LOG_LINES ? lines.slice(-MAX_LOG_LINES) : lines,
      };
    }
  }
}

/** Testo localizzato per un errore di installazione o aggiornamento (il dettaglio è `error.message`). */
export function setupErrorKey(error: CswapError): MessageKey {
  switch (error.kind) {
    case 'busy':
      return 'setup.error.busy';
    case 'unsupportedSchema':
      return 'setup.error.schema';
    case 'timeout':
      return 'setup.error.timeout';
    case 'notInstalled':
      return 'setup.error.notInstalled';
    default:
      return 'setup.error.generic';
  }
}

/** Chiave del nome di un passo. */
export function stepKey(step: SetupStep): MessageKey {
  return `setup.step.${step}`;
}
