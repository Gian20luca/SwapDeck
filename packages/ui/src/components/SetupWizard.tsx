import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import type { CswapError, SetupDiagnosis, SetupOutcome } from '@swapdeck/types';
import { useBackend } from '../backend/context.js';
import { BackendError, hasSetup, type Backend, type SetupBackend } from '../backend/types.js';
import { useI18n } from '../i18n/index.js';
import { INSTALL_STEPS, UPGRADE_STEPS, setupErrorKey } from '../lib/setup.js';
import { useSetupRun } from '../lib/useSetupRun.js';
import { ConfirmDialog } from './ConfirmDialog.js';
import { SetupRun } from './SetupRun.js';

export interface SetupWizardProps {
  /** Diagnosi già fatta dall'host; senza, il componente chiama `diagnoseSetup`. */
  initial?: SetupDiagnosis;
  /** cswap è pronto: con almeno un account, o l'utente ha scelto di continuare senza. */
  onReady: (diagnosis: SetupDiagnosis) => void;
}

const UV_SCRIPT_URL = 'https://astral.sh/uv/';

type Phase =
  | { name: 'checking' }
  | { name: 'show'; diagnosis: SetupDiagnosis }
  | { name: 'running'; kind: 'install' | 'upgrade'; diagnosis: SetupDiagnosis }
  | { name: 'failed'; kind: 'install' | 'upgrade'; diagnosis: SetupDiagnosis }
  | { name: 'error'; error: CswapError };

/**
 * Procedura di primo avvio: diagnosi di cswap; se manca, spiegazione, conferma, installazione con
 * avanzamento ed esito; se non ci sono account, guida a `/login` e ad «Aggiungi account corrente».
 * Propone anche l'aggiornamento quando la versione installata è sotto la minima.
 * Non mostra nulla se l'host non offre `SetupBackend`.
 */
export function SetupWizard(props: SetupWizardProps) {
  const backend = useBackend();
  return hasSetup(backend) ? <Wizard {...props} backend={backend} /> : null;
}

function Wizard({
  initial,
  onReady,
  backend,
}: SetupWizardProps & { backend: Backend & SetupBackend }) {
  const { t } = useI18n();
  const { run, start } = useSetupRun();
  const [phase, setPhase] = useState<Phase>(
    initial ? { name: 'show', diagnosis: initial } : { name: 'checking' },
  );
  const [confirming, setConfirming] = useState<'install' | 'upgrade' | null>(null);
  const [addState, setAddState] = useState<{ busy: boolean; message?: string }>({ busy: false });
  const readyRef = useRef(onReady);
  readyRef.current = onReady;

  const show = useCallback((diagnosis: SetupDiagnosis) => {
    if (diagnosis.status === 'ready' && (diagnosis.accountCount ?? 0) > 0) {
      readyRef.current(diagnosis);
    }
    setPhase({ name: 'show', diagnosis });
  }, []);

  const diagnose = useCallback(async () => {
    setPhase({ name: 'checking' });
    try {
      show(await backend.diagnoseSetup());
    } catch (error) {
      setPhase({
        name: 'error',
        error:
          error instanceof BackendError ? error.info : { kind: 'failed', message: String(error) },
      });
    }
  }, [backend, show]);

  useEffect(() => {
    if (initial) show(initial);
    else void diagnose();
    // Solo al montaggio: `initial` è il valore di partenza.
  }, []);

  async function execute(kind: 'install' | 'upgrade', diagnosis: SetupDiagnosis) {
    setConfirming(null);
    setPhase({ name: 'running', kind, diagnosis });
    const outcome: SetupOutcome | null | 'cancelled' = await start((onProgress) =>
      kind === 'install' ? backend.installCswap(onProgress) : backend.upgradeCswap(onProgress),
    );
    if (outcome === 'cancelled') setPhase({ name: 'show', diagnosis });
    else if (outcome) show(outcome.diagnosis);
    else setPhase({ name: 'failed', kind, diagnosis });
  }

  /** Conferma nella UI, o direttamente all'host se è lui a chiederla (VS Code). */
  function ask(kind: 'install' | 'upgrade', diagnosis: SetupDiagnosis) {
    if (backend.hostConfirms) void execute(kind, diagnosis);
    else setConfirming(kind);
  }

  async function addCurrent(diagnosis: SetupDiagnosis) {
    setAddState({ busy: true });
    try {
      await backend.runAction({ kind: 'add' });
      const next = await backend.diagnoseSetup();
      setAddState({ busy: false, message: t('setup.added') });
      show(next.status === 'ready' ? next : diagnosis);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setAddState({ busy: false, message: t('setup.addFailed', { message }) });
    }
  }

  if (phase.name === 'checking') {
    return (
      <section className="sd-setup sd-state" role="status">
        <p>{t('setup.checking')}</p>
      </section>
    );
  }

  if (phase.name === 'error') {
    return (
      <section className="sd-setup">
        <h2>{t('setup.error.title')}</h2>
        <ErrorText error={phase.error} />
        <Actions>
          <button type="button" className="sd-btn" onClick={() => void diagnose()}>
            {t('common.retry')}
          </button>
        </Actions>
      </section>
    );
  }

  if (phase.name === 'running' || phase.name === 'failed') {
    const steps = phase.kind === 'install' ? INSTALL_STEPS : UPGRADE_STEPS;
    const failed = phase.name === 'failed';
    return (
      <section className="sd-setup" aria-busy={!failed}>
        <h2>
          {failed
            ? phase.kind === 'install'
              ? t('setup.failed.title')
              : t('update.failed')
            : phase.kind === 'install'
              ? t('setup.installing')
              : t('update.upgrading')}
        </h2>
        <SetupRun steps={steps} run={run} logOpen={failed} />
        {failed && run.error && <ErrorText error={run.error} />}
        {failed && (
          <Actions>
            <button type="button" className="sd-btn" onClick={() => void diagnose()}>
              {t('common.back')}
            </button>
            <button
              type="button"
              className="sd-btn sd-btn--primary"
              onClick={() => void execute(phase.kind, phase.diagnosis)}
            >
              {t('common.retry')}
            </button>
          </Actions>
        )}
      </section>
    );
  }

  const d = phase.diagnosis;
  const confirmDialog = (
    <ConfirmDialog
      open={confirming !== null}
      title={confirming === 'upgrade' ? t('setup.upgrade.title') : t('setup.confirm.title')}
      message={
        confirming === 'upgrade'
          ? t('update.confirm.body')
          : d.uv
            ? t('setup.confirm.bodyUv')
            : t('setup.confirm.body')
      }
      confirmLabel={confirming === 'upgrade' ? t('update.upgrade') : t('setup.install')}
      onConfirm={() => void execute(confirming ?? 'install', d)}
      onCancel={() => setConfirming(null)}
    />
  );

  switch (d.status) {
    case 'cswapMissing':
      return (
        <section className="sd-setup">
          <h2>{t('setup.missing.title')}</h2>
          <p>{t('setup.missing.body')}</p>
          <p className="sd-setup__hint">
            {d.uv
              ? t('setup.missing.uvFound', { path: d.uv.path })
              : t('setup.missing.uvMissing', { url: UV_SCRIPT_URL })}
          </p>
          <Actions>
            <button
              type="button"
              className="sd-btn sd-btn--primary"
              onClick={() => ask('install', d)}
            >
              {t('setup.install')}
            </button>
          </Actions>
          {confirmDialog}
        </section>
      );
    case 'unsupportedVersion':
      return (
        <section className="sd-setup">
          <h2>{t('setup.unsupportedVersion.title')}</h2>
          <p>
            {t('setup.unsupportedVersion.body', {
              version: d.cswap?.version ?? '?',
              min: d.minVersion,
            })}
          </p>
          {!d.uv && <p className="sd-setup__hint">{t('setup.unsupportedVersion.noUv')}</p>}
          <Actions>
            <button type="button" className="sd-btn" onClick={() => void diagnose()}>
              {t('common.retry')}
            </button>
            {d.uv && (
              <button
                type="button"
                className="sd-btn sd-btn--primary"
                onClick={() => ask('upgrade', d)}
              >
                {t('update.upgrade')}
              </button>
            )}
          </Actions>
          {confirmDialog}
        </section>
      );
    case 'unsupportedSchema':
      return (
        <section className="sd-setup">
          <h2>{t('setup.unsupportedSchema.title')}</h2>
          <p>{t('setup.unsupportedSchema.body', { found: d.schemaVersion ?? '?' })}</p>
          <Actions>
            <button type="button" className="sd-btn" onClick={() => void diagnose()}>
              {t('common.retry')}
            </button>
          </Actions>
        </section>
      );
    case 'error':
      return (
        <section className="sd-setup">
          <h2>{t('setup.error.title')}</h2>
          {d.error && <ErrorText error={d.error} />}
          <Actions>
            <button type="button" className="sd-btn" onClick={() => void diagnose()}>
              {t('common.retry')}
            </button>
          </Actions>
        </section>
      );
    case 'ready':
      return (
        <section className="sd-setup">
          <h2>{t('setup.done.title', { version: d.cswap?.version ?? '' })}</h2>
          {(d.accountCount ?? 0) === 0 && (
            <>
              <h3>{t('setup.noAccounts.title')}</h3>
              <p>{t('setup.noAccounts.body')}</p>
              <ol className="sd-setup__guide">
                <li>{t('setup.noAccounts.step1')}</li>
                <li>{t('setup.noAccounts.step2')}</li>
              </ol>
            </>
          )}
          {addState.message && <p role="status">{addState.message}</p>}
          <Actions>
            <button type="button" className="sd-btn" onClick={() => onReady(d)}>
              {t('common.continue')}
            </button>
            {(d.accountCount ?? 0) === 0 && (
              <button
                type="button"
                className="sd-btn sd-btn--primary"
                disabled={addState.busy}
                onClick={() => void addCurrent(d)}
              >
                {addState.busy ? t('common.working') : t('setup.addCurrent')}
              </button>
            )}
          </Actions>
        </section>
      );
  }
}

function Actions({ children }: { children: ReactNode }) {
  return <div className="sd-setup__actions">{children}</div>;
}

/** Testo localizzato dell'errore e, sotto, il messaggio (già ripulito dal Rust). */
export function ErrorText({ error }: { error: CswapError }) {
  const { t } = useI18n();
  return (
    <div className="sd-notice" data-tone="danger" role="alert">
      <p>{t(setupErrorKey(error))}</p>
      {error.message && <p className="sd-setup__detail">{error.message}</p>}
    </div>
  );
}
