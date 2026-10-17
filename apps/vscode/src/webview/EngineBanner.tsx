import type { SuspendReason } from '@swapdeck/types';
import { useI18n, type MessageKey, type Translate } from '@swapdeck/ui';
import { useEffect, useState } from 'react';
import type { EngineSuspension } from '../protocol.js';
import type { VsCodeBackend } from './VsCodeBackend.js';

const SUSPEND_KEYS = {
  disabled: 'history.suspend.disabled',
  invalidRules: 'history.suspend.invalidRules',
  conflict: 'history.suspend.conflict',
  noActiveAccount: 'history.suspend.noActiveAccount',
  noViableTarget: 'history.suspend.noViableTarget',
} as const satisfies Record<SuspendReason, MessageKey>;

/** Testo della riga: «Motore sospeso: altro motore attivo (id, …)», come nell'app desktop. */
export function suspensionLine(suspended: EngineSuspension, t: Translate): string {
  const detail = suspended.detail.length > 0 ? ` (${suspended.detail.join(', ')})` : '';
  return t('history.decision.suspended', { reason: t(SUSPEND_KEYS[suspended.reason]) }) + detail;
}

/**
 * Riga «motore sospeso» sopra le schede, come nell'app desktop: motivo e dettagli (per esempio
 * gli id dei conflitti) dall'ultimo `swapdeck-cli state`. Niente se il motore non è sospeso.
 */
export function EngineBanner({ backend }: { backend: Pick<VsCodeBackend, 'onEngineSuspension'> }) {
  const { t } = useI18n();
  const [suspended, setSuspended] = useState<EngineSuspension | null>(null);
  useEffect(() => backend.onEngineSuspension(setSuspended), [backend]);
  if (!suspended) return null;
  return (
    <div className="sd-vscode-engine" role="status" aria-live="polite">
      <div className="sd-notice" data-tone="danger">
        {suspensionLine(suspended, t)}
      </div>
    </div>
  );
}
