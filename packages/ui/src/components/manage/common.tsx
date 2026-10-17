import { useId, type ReactNode } from 'react';
import type { Notice } from '../../backend/useActionRunner.js';
import { useI18n } from '../../i18n/index.js';
import type { Loadable } from '../../lib/useLoad.js';
import { InfoTip, WithInfo } from '../InfoTip.js';
import { Tooltip } from '../Tooltip.js';

/** Esito di un'azione: riga di stato (alert se è un errore). */
export function NoticeView({ notice }: { notice: Notice | null }) {
  return (
    <div aria-live="polite">
      {notice && (
        <div
          className="sd-notice"
          data-tone={notice.tone}
          role={notice.tone === 'danger' ? 'alert' : 'status'}
        >
          {notice.text}
        </div>
      )}
    </div>
  );
}

export function Section({
  title,
  hint,
  info,
  children,
}: {
  title: string;
  hint?: ReactNode;
  /** Spiegazione del concetto: mostra la «i» accanto al titolo. */
  info?: string;
  children: ReactNode;
}) {
  const id = useId();
  return (
    <section className="sd-section" aria-labelledby={id}>
      <WithInfo info={info} name={title}>
        <h3 id={id}>{title}</h3>
      </WithInfo>
      {hint && <p className="sd-hint">{hint}</p>}
      {children}
    </section>
  );
}

export interface TextFieldProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  type?: 'text' | 'password' | 'number';
  hint?: string;
  placeholder?: string;
  disabled?: boolean;
  min?: number;
  max?: number;
  step?: number | 'any';
  error?: string | null;
  /** Campo sensibile: niente completamento, niente correzione. */
  secret?: boolean;
  /** Tooltip: cosa fa il campo e cosa succede. */
  tip?: string;
  /** Spiegazione del concetto: mostra la «i» accanto all'etichetta. */
  info?: string;
}

/** Campo di testo con etichetta, suggerimento ed errore collegati per l'accessibilità. */
export function TextField({
  label,
  value,
  onChange,
  type = 'text',
  hint,
  placeholder,
  disabled,
  min,
  max,
  step,
  error,
  secret,
  tip,
  info,
}: TextFieldProps) {
  const id = useId();
  const describedBy = [hint ? `${id}-hint` : '', error ? `${id}-err` : '']
    .filter(Boolean)
    .join(' ');
  return (
    <div className="sd-form__field">
      <WithInfo info={info} name={label}>
        <label htmlFor={id}>{label}</label>
      </WithInfo>
      <Tooltip text={tip ?? ''} block>
        <input
          id={id}
          className="sd-input"
          type={type}
          value={value}
          placeholder={placeholder}
          disabled={disabled}
          min={min}
          max={max}
          step={step}
          aria-invalid={error ? true : undefined}
          aria-describedby={describedBy || undefined}
          autoComplete={secret ? 'off' : undefined}
          spellCheck={secret ? false : undefined}
          onChange={(event) => onChange(event.target.value)}
        />
      </Tooltip>
      {hint && (
        <span id={`${id}-hint`} className="sd-hint">
          {hint}
        </span>
      )}
      {error && (
        <span id={`${id}-err`} className="sd-field-error">
          {error}
        </span>
      )}
    </div>
  );
}

/** Percorso scritto a mano, con «Sfoglia…» quando l'host ha il selettore nativo. */
export function PathField({
  label,
  value,
  onChange,
  onBrowse,
  disabled,
  placeholder,
  tip,
  browseTip,
  info,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onBrowse?: () => Promise<string | null>;
  disabled?: boolean;
  placeholder?: string;
  tip?: string;
  browseTip?: string;
  info?: string;
}) {
  const { t } = useI18n();
  const id = useId();
  return (
    <div className="sd-form__field">
      <WithInfo info={info} name={label}>
        <label htmlFor={id}>{label}</label>
      </WithInfo>
      <div className="sd-row">
        <Tooltip text={tip ?? ''}>
          <input
            id={id}
            className="sd-input"
            value={value}
            placeholder={placeholder}
            disabled={disabled}
            spellCheck={false}
            onChange={(event) => onChange(event.target.value)}
          />
        </Tooltip>
        {onBrowse && (
          <Tooltip text={browseTip ?? t('common.browse.tip')}>
            <button
              type="button"
              className="sd-btn"
              disabled={disabled}
              onClick={() => {
                void onBrowse()
                  .then((picked) => {
                    if (picked) onChange(picked);
                  })
                  .catch(() => undefined);
              }}
            >
              {t('common.browse')}
            </button>
          </Tooltip>
        )}
      </div>
    </div>
  );
}

export function CheckField({
  label,
  checked,
  onChange,
  disabled,
  hint,
  tip,
  info,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  hint?: string;
  tip?: string;
  info?: string;
}) {
  const id = useId();
  return (
    <div className="sd-form__check">
      <Tooltip text={tip ?? ''}>
        <input
          id={id}
          type="checkbox"
          checked={checked}
          disabled={disabled}
          aria-describedby={hint ? `${id}-hint` : undefined}
          onChange={(event) => onChange(event.target.checked)}
        />
      </Tooltip>
      <label htmlFor={id}>{label}</label>
      {info && <InfoTip name={label} text={info} />}
      {hint && (
        <span id={`${id}-hint`} className="sd-hint">
          {hint}
        </span>
      )}
    </div>
  );
}

export function SelectField({
  label,
  value,
  onChange,
  options,
  disabled,
  tip,
  hint,
  info,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string }[];
  disabled?: boolean;
  tip?: string;
  /** Riga sotto il campo (per esempio la descrizione della scelta). */
  hint?: string;
  info?: string;
}) {
  const id = useId();
  return (
    <div className="sd-form__field">
      <WithInfo info={info} name={label}>
        <label htmlFor={id}>{label}</label>
      </WithInfo>
      <Tooltip text={tip ?? ''} block>
        <select
          id={id}
          className="sd-input"
          value={value}
          disabled={disabled}
          aria-describedby={hint ? `${id}-hint` : undefined}
          onChange={(event) => onChange(event.target.value)}
        >
          {options.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </select>
      </Tooltip>
      {hint && (
        <span id={`${id}-hint`} className="sd-hint">
          {hint}
        </span>
      )}
    </div>
  );
}

/** Funzione non offerta da questo host (metodo facoltativo del `Backend` assente). */
export function Unavailable() {
  const { t } = useI18n();
  return (
    <div className="sd-state" role="status">
      <p>{t('common.unavailable')}</p>
    </div>
  );
}

/** Caricamento, errore con «Riprova» o contenuto. */
export function LoadGate<T>({
  state,
  onRetry,
  loadingText,
  loading,
  children,
}: {
  state: Loadable<T>;
  onRetry: () => void;
  loadingText?: string;
  /** Al posto del semplice «Caricamento…» (per esempio con l'avanzamento). */
  loading?: ReactNode;
  children: (data: T) => ReactNode;
}) {
  const { t } = useI18n();
  if (state.status === 'loading') {
    if (loading) return <>{loading}</>;
    return (
      <div className="sd-state" role="status">
        {loadingText ?? t('common.loading')}
      </div>
    );
  }
  if (state.status === 'error') {
    return (
      <div className="sd-state" role="alert">
        <p>{state.message}</p>
        <button type="button" className="sd-btn" onClick={onRetry}>
          {t('common.retry')}
        </button>
      </div>
    );
  }
  return <>{children(state.data)}</>;
}
