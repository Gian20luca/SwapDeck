import { useEffect, useState } from 'react';
import type { SetupOutcome, UpdateInfo } from '@swapdeck/types';
import { useBackend } from '../backend/context.js';
import { hasSetup, type Backend, type SetupBackend } from '../backend/types.js';
import { useI18n } from '../i18n/index.js';
import { UPGRADE_STEPS } from '../lib/setup.js';
import { useSetupRun } from '../lib/useSetupRun.js';
import { ConfirmDialog } from './ConfirmDialog.js';
import { SetupRun } from './SetupRun.js';
import { ErrorText } from './SetupWizard.js';

export interface CswapUpdateNoticeProps {
  /** Aggiornamento riuscito (per esempio per riavviare il motore fermato prima). */
  onUpgraded?: (outcome: SetupOutcome) => void;
}

type Phase = 'idle' | 'confirm' | 'running' | 'done' | 'failed' | 'hidden';

/**
 * Avviso «nuova versione di cswap disponibile». Chiuderlo ricorda la scelta per quella versione
 * (`dismissCswapUpdate`); «Aggiorna» chiede conferma e mostra l'avanzamento. Un controllo non
 * riuscito (rete, cswap assente) o un host senza `SetupBackend` non mostrano nulla.
 */
export function CswapUpdateNotice(props: CswapUpdateNoticeProps) {
  const backend = useBackend();
  return hasSetup(backend) ? <Notice {...props} backend={backend} /> : null;
}

function Notice({
  onUpgraded,
  backend,
}: CswapUpdateNoticeProps & { backend: Backend & SetupBackend }) {
  const { t } = useI18n();
  const { run, start } = useSetupRun();
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [phase, setPhase] = useState<Phase>('idle');
  const [version, setVersion] = useState('');

  useEffect(() => {
    let alive = true;
    backend.checkCswapUpdate().then(
      (result) => {
        if (alive) setInfo(result);
      },
      () => undefined,
    );
    return () => {
      alive = false;
    };
  }, [backend]);

  if (!info || phase === 'hidden') return null;
  if (phase === 'idle' && (!info.updateAvailable || info.dismissed)) return null;

  function dismiss() {
    setPhase('hidden');
    if (info && phase !== 'done')
      void backend.dismissCswapUpdate(info.latest).catch(() => undefined);
  }

  async function upgrade() {
    setPhase('running');
    const outcome = await start((onProgress) => backend.upgradeCswap(onProgress));
    if (outcome) {
      setVersion(outcome.diagnosis.cswap?.version ?? info?.latest ?? '');
      setPhase('done');
      onUpgraded?.(outcome);
    } else {
      setPhase('failed');
    }
  }

  const closeButton = (
    <button
      type="button"
      className="sd-btn sd-btn--small"
      aria-label={
        phase === 'done' ? t('common.close') : t('update.dismiss', { latest: info.latest })
      }
      title={phase === 'done' ? t('common.close') : t('update.dismiss', { latest: info.latest })}
      onClick={dismiss}
    >
      ×
    </button>
  );

  return (
    <div
      className="sd-notice sd-update"
      role="status"
      data-tone={phase === 'failed' ? 'danger' : undefined}
    >
      <div className="sd-update__row">
        <span>
          {phase === 'running'
            ? t('update.upgrading')
            : phase === 'done'
              ? t('update.done', { version })
              : phase === 'failed'
                ? t('update.failed')
                : t('update.available', {
                    latest: info.latest,
                    installed: info.installed ?? '?',
                  })}
        </span>
        <span className="sd-toolbar__group">
          {(phase === 'idle' || phase === 'confirm' || phase === 'failed') && (
            <button
              type="button"
              className="sd-btn sd-btn--small sd-btn--primary"
              onClick={() => setPhase('confirm')}
            >
              {phase === 'failed' ? t('common.retry') : t('update.upgrade')}
            </button>
          )}
          {phase !== 'running' && closeButton}
        </span>
      </div>
      {(phase === 'running' || phase === 'failed') && (
        <SetupRun steps={UPGRADE_STEPS} run={run} logOpen={phase === 'failed'} />
      )}
      {phase === 'failed' && run.error && <ErrorText error={run.error} />}
      <ConfirmDialog
        open={phase === 'confirm'}
        title={t('update.confirm.title', { latest: info.latest })}
        message={t('update.confirm.body')}
        confirmLabel={t('update.upgrade')}
        onConfirm={() => void upgrade()}
        onCancel={() => setPhase(run.error ? 'failed' : 'idle')}
      />
    </div>
  );
}
