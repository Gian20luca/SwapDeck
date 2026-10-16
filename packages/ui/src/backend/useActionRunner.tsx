import type { Confirmation, CswapAction } from '@swapdeck/types';
import { useCallback, useRef, useState, type ReactNode } from 'react';
import { ConfirmDialog } from '../components/ConfirmDialog.js';
import { useI18n } from '../i18n/index.js';
import { useBackend } from './context.js';
import { errorMessage } from './useAccounts.js';
import type { ActionOutcome } from './types.js';

export interface ActionCopy {
  title: string;
  message?: ReactNode;
  confirmLabel?: string;
  /** Default `danger`: le azioni con conferma sono quelle delicate. */
  tone?: 'default' | 'danger';
}

export interface RunOptions {
  /** Testi del dialogo di conferma (usati solo se la conferma serve e l'host non la chiede da sé). */
  copy?: ActionCopy;
  /** Messaggio di esito; senza, quello dell'azione. */
  success?: string;
}

export interface Notice {
  tone: 'info' | 'danger';
  text: string;
}

interface Pending {
  copy: ActionCopy;
  word: string | undefined;
  settle: (confirmed: boolean) => void;
}

/**
 * Esegue le azioni di cswap con la conferma dovuta (`confirmationFor`, regola nel Rust): nessuna,
 * semplice o con la parola da digitare. Se l'host conferma da sé (`hostConfirms`) la UI non chiede.
 * `run` risolve `true` se l'azione è riuscita, `false` se annullata o fallita; l'esito sta in `notice`.
 */
export function useActionRunner() {
  const backend = useBackend();
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const busyRef = useRef(false);

  const run = useCallback(
    async (
      action: CswapAction,
      options: RunOptions = {},
    ): Promise<{ ok: boolean; outcome?: ActionOutcome }> => {
      if (busyRef.current) return { ok: false };
      busyRef.current = true;
      setBusy(true);
      setNotice(null);
      try {
        let confirmation: Confirmation = { kind: 'none' };
        if (!backend.hostConfirms) confirmation = await backend.confirmationFor(action);
        if (confirmation.kind !== 'none') {
          const copy = options.copy ?? { title: t('confirm.generic') };
          const confirmed = await new Promise<boolean>((resolve) =>
            setPending({
              copy,
              word: confirmation.kind === 'typed' ? confirmation.word : undefined,
              settle: resolve,
            }),
          );
          setPending(null);
          if (!confirmed) return { ok: false };
        }
        const outcome = await backend.runAction(action);
        const text = options.success ?? outcome.message ?? t('common.done');
        setNotice({ tone: 'info', text });
        return { ok: true, outcome };
      } catch (error) {
        setNotice({
          tone: 'danger',
          text: t('common.actionFailed', { message: errorMessage(error) }),
        });
        return { ok: false };
      } finally {
        busyRef.current = false;
        setBusy(false);
      }
    },
    [backend, t],
  );

  const dialog = (
    <ConfirmDialog
      open={pending !== null}
      title={pending?.copy.title ?? ''}
      message={pending?.copy.message}
      tone={pending?.copy.tone ?? 'danger'}
      {...(pending?.copy.confirmLabel ? { confirmLabel: pending.copy.confirmLabel } : {})}
      {...(pending?.word ? { requireWord: pending.word } : {})}
      onConfirm={() => pending?.settle(true)}
      onCancel={() => pending?.settle(false)}
    />
  );

  return { run, busy, notice, setNotice, dialog };
}
