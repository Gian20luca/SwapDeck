import { useI18n } from '../i18n/index.js';

export interface PaceIndicatorProps {
  /** Consumo atteso a questo punto della finestra (0–100). Senza, il componente non si mostra. */
  expectedPct: number | null | undefined;
  aheadOfPace?: boolean | null | undefined;
  willLastToReset?: boolean | null | undefined;
  /** ISO 8601: solo se la finestra si esaurisce prima del reset. */
  projectedExhaustionAt?: string | null | undefined;
}

/** Indicatore del ritmo di consumo: testo e simbolo, mai solo colore. */
export function PaceIndicator({
  expectedPct,
  aheadOfPace,
  willLastToReset,
  projectedExhaustionAt,
}: PaceIndicatorProps) {
  const { t, locale } = useI18n();
  if (expectedPct === null || expectedPct === undefined || !Number.isFinite(expectedPct)) {
    return null;
  }
  const ahead = aheadOfPace === true;
  const exhausts = willLastToReset === false;
  const tone = exhausts ? 'danger' : ahead ? 'warn' : 'ok';
  const when =
    exhausts && projectedExhaustionAt && !Number.isNaN(Date.parse(projectedExhaustionAt))
      ? new Date(projectedExhaustionAt).toLocaleString(locale, {
          day: 'numeric',
          month: 'short',
          hour: '2-digit',
          minute: '2-digit',
        })
      : null;

  return (
    <div className="sd-pace" data-tone={tone} aria-label={t('pace.label')} role="group">
      <span className="sd-pace__main">
        <span aria-hidden="true">{ahead ? '▲' : '●'}</span>{' '}
        <span>{ahead ? t('pace.ahead') : t('pace.onTrack')}</span>
        <span className="sd-pace__expected">
          {' '}
          · {t('pace.expected', { pct: Math.round(expectedPct) })}
        </span>
      </span>
      {willLastToReset !== null && willLastToReset !== undefined && (
        <span className="sd-pace__last">
          {exhausts ? t('pace.willNotLast') : t('pace.willLast')}
          {when ? ` · ${t('pace.exhaustion', { when })}` : ''}
        </span>
      )}
    </div>
  );
}
