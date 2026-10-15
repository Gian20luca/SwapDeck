import { countdownTo, displayPct, levelFor, type Thresholds } from '../lib/format.js';
import { useI18n } from '../i18n/index.js';

export interface UsageBarProps {
  /** Nome della finestra, es. «5 ore». */
  label: string;
  /** Nome dell'account, solo per l'etichetta accessibile. */
  accountName: string;
  pct: number | null | undefined;
  /** ISO 8601; il countdown si ricalcola da qui. */
  resetsAt?: string | null | undefined;
  /** Istante di riferimento (ms) per il countdown. */
  now: number;
  /** Ritmo atteso: segna un tacco sulla barra. */
  expectedPct?: number | null | undefined;
  thresholds?: Thresholds;
  compact?: boolean;
}

/** Barra di consumo con colore di soglia, valore numerico (mai solo colore) e countdown al reset. */
export function UsageBar({
  label,
  accountName,
  pct,
  resetsAt,
  now,
  expectedPct,
  thresholds,
  compact = false,
}: UsageBarProps) {
  const { t } = useI18n();
  const known = pct !== null && pct !== undefined && Number.isFinite(pct);
  const shown = known ? displayPct(pct) : null;
  const level = levelFor(pct, thresholds);
  const countdown = countdownTo(resetsAt, now);
  const pctText = shown === null ? t('usage.unknown') : t('usage.pct', { pct: shown });
  const hasPace = expectedPct !== null && expectedPct !== undefined && Number.isFinite(expectedPct);

  return (
    <div className="sd-bar" data-level={level} data-compact={compact || undefined}>
      <div className="sd-bar__head">
        <span>{label}</span>
        <span className="sd-bar__pct">{pctText}</span>
      </div>
      <div
        className="sd-bar__track"
        role="progressbar"
        aria-label={t('usage.barLabel', {
          window: label,
          name: accountName,
          pct: shown ?? t('usage.unknown'),
        })}
        aria-valuemin={0}
        aria-valuemax={100}
        {...(shown !== null ? { 'aria-valuenow': shown } : {})}
      >
        <div className="sd-bar__fill" style={{ width: `${shown ?? 0}%` }} />
        {hasPace && (
          <div
            className="sd-bar__pace"
            data-testid="pace-mark"
            style={{ left: `${Math.min(100, Math.max(0, expectedPct))}%` }}
          />
        )}
      </div>
      {(countdown !== null || !compact) && (
        <div className="sd-bar__foot">
          <span>
            {countdown !== null ? t('usage.resetsIn', { time: countdown }) : t('usage.noReset')}
          </span>
        </div>
      )}
    </div>
  );
}
