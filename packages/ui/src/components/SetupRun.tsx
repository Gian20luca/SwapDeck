import type { SetupStep } from '@swapdeck/types';
import { useI18n } from '../i18n/index.js';
import { stepKey, type RunState } from '../lib/setup.js';

export interface SetupRunProps {
  /** Passi da mostrare, nell'ordine. */
  steps: readonly SetupStep[];
  run: RunState;
  /** Dettagli (log) aperti all'inizio. */
  logOpen?: boolean;
}

const MARK = { pending: '○', running: '◌', done: '✓', skipped: '–', failed: '✕' } as const;

/** Avanzamento a passi di installazione o aggiornamento, con l'output dei processi. */
export function SetupRun({ steps, run, logOpen = false }: SetupRunProps) {
  const { t } = useI18n();
  // Passi fuori elenco ma avviati (per esempio il ripristino dopo un aggiornamento) in coda.
  const extra = (Object.keys(run.steps) as SetupStep[]).filter((s) => !steps.includes(s));
  return (
    <div className="sd-setup__run">
      <ol className="sd-steps">
        {[...steps, ...extra].map((step) => {
          const state = run.steps[step] ?? 'pending';
          return (
            <li key={step} data-state={state}>
              <span className="sd-steps__mark" aria-hidden="true">
                {MARK[state]}
              </span>
              <span>{t(stepKey(step))}</span>
              <span className="sd-visually-hidden">: {t(`setup.state.${state}`)}</span>
            </li>
          );
        })}
      </ol>
      {run.lines.length > 0 && (
        <details className="sd-setup__details" open={logOpen}>
          <summary>{t('setup.log')}</summary>
          <pre className="sd-log" role="log" aria-label={t('setup.logLabel')}>
            {run.lines.join('\n')}
          </pre>
        </details>
      )}
    </div>
  );
}
