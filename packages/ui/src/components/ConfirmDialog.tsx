import { useEffect, useId, useRef, useState, type KeyboardEvent, type ReactNode } from 'react';
import { useI18n } from '../i18n/index.js';

export interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  /** `danger` colora il pulsante di conferma e porta il focus iniziale su «Annulla». */
  tone?: 'default' | 'danger';
  /**
   * Doppia conferma: dopo il primo passo (conferma semplice) chiede di digitare questa parola
   * esatta (maiuscole e minuscole contano). Serve per le azioni irreversibili come `purge`.
   */
  requireWord?: string;
  /** Terza scelta, tra «Annulla» e la conferma (per esempio «Solo osservazione»); come la conferma può essere asincrona. */
  alt?: { label: string; onSelect: () => void | Promise<void> };
  /** Può essere asincrona: il dialogo resta aperto e occupato finché non termina. */
  onConfirm: () => void | Promise<void>;
  onCancel: () => void;
}

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex="-1"])';

/** Finestra di conferma riusabile: focus intrappolato, Esc annulla, il focus torna a chi l'ha aperta. */
export function ConfirmDialog(props: ConfirmDialogProps) {
  const { open } = props;
  if (!open) return null;
  return <OpenDialog {...props} />;
}

function OpenDialog({
  title,
  message,
  confirmLabel,
  cancelLabel,
  tone = 'default',
  requireWord,
  alt,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const { t } = useI18n();
  const titleId = useId();
  const bodyId = useId();
  const inputId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [step, setStep] = useState<1 | 2>(1);
  const [typed, setTyped] = useState('');
  const [busy, setBusy] = useState(false);
  const totalSteps = requireWord ? 2 : 1;
  const wordOk = !requireWord || typed === requireWord;

  // Ripristina il focus a chi ha aperto il dialogo.
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    return () => previous?.focus?.();
  }, []);

  // Focus iniziale: l'azione sicura se pericolosa, il campo di testo al secondo passo.
  useEffect(() => {
    if (step === 2) inputRef.current?.focus();
    else if (tone === 'danger') cancelRef.current?.focus();
    else confirmRef.current?.focus();
  }, [step, tone]);

  async function run(action: () => void | Promise<void>) {
    setBusy(true);
    try {
      await action();
    } finally {
      setBusy(false);
    }
  }

  async function confirm() {
    if (requireWord && step === 1) {
      setStep(2);
      return;
    }
    if (!wordOk || busy) return;
    await run(onConfirm);
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === 'Escape') {
      event.stopPropagation();
      if (!busy) onCancel();
      return;
    }
    if (event.key === 'Enter' && step === 2 && event.target === inputRef.current) {
      event.preventDefault();
      void confirm();
      return;
    }
    if (event.key !== 'Tab') return;
    const nodes = dialogRef.current?.querySelectorAll<HTMLElement>(FOCUSABLE);
    if (!nodes || nodes.length === 0) return;
    const first = nodes[0]!;
    const last = nodes[nodes.length - 1]!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  const isFinal = step === totalSteps;
  const confirmText = isFinal ? (confirmLabel ?? t('common.confirm')) : t('common.continue');

  return (
    <div
      className="sd-overlay"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onCancel();
      }}
    >
      <div
        ref={dialogRef}
        className="sd-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={bodyId}
        onKeyDown={onKeyDown}
      >
        <h2 id={titleId}>{title}</h2>
        {totalSteps > 1 && (
          <div className="sd-dialog__step">
            {t('confirm.step', { current: step, total: totalSteps })}
          </div>
        )}
        <div id={bodyId}>
          {message && <p>{message}</p>}
          {step === 2 && requireWord && (
            <>
              <p>{t('confirm.again', { word: requireWord })}</p>
              <label className="sd-visually-hidden" htmlFor={inputId}>
                {t('confirm.wordLabel')}
              </label>
              <input
                id={inputId}
                ref={inputRef}
                className="sd-input"
                value={typed}
                autoComplete="off"
                spellCheck={false}
                onChange={(event) => setTyped(event.target.value)}
              />
            </>
          )}
        </div>
        <div className="sd-dialog__actions">
          <button
            type="button"
            ref={cancelRef}
            className="sd-btn"
            disabled={busy}
            onClick={onCancel}
          >
            {cancelLabel ?? t('common.cancel')}
          </button>
          {alt && isFinal && (
            <button
              type="button"
              className="sd-btn"
              disabled={busy}
              onClick={() => void (busy ? undefined : run(alt.onSelect))}
            >
              {alt.label}
            </button>
          )}
          <button
            type="button"
            ref={confirmRef}
            className={`sd-btn ${tone === 'danger' ? 'sd-btn--danger' : 'sd-btn--primary'}`}
            disabled={busy || (step === 2 && !wordOk)}
            onClick={() => void confirm()}
          >
            {busy ? t('common.working') : confirmText}
          </button>
        </div>
      </div>
    </div>
  );
}
