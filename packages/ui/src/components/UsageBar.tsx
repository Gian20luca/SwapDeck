import { useId } from 'react';
import type { EngineMark } from '../lib/engineMarks.js';
import { countdownTo, displayPct, formatWhen, levelFor, type Thresholds } from '../lib/format.js';
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
  /** Ritmo atteso: segna un tratteggio sulla barra. */
  expectedPct?: number | null | undefined;
  thresholds?: Thresholds;
  /** Soglia di uscita del motore: trattino pieno (rosso se il motore cambia davvero, grigio altrimenti). */
  engineMark?: EngineMark | undefined;
  /** Esaurimento previsto (ISO 8601): se cade prima del reset, una riga lo dice. Solo sulle settimanali. */
  exhaustionAt?: string | null | undefined;
  compact?: boolean;
}

type Translate = ReturnType<typeof useI18n>['t'];

/** Testo del tooltip della soglia del motore, secondo come il motore la usa. */
export function engineTip(mark: EngineMark, t: Translate): string {
  const pct = mark.pct;
  switch (mark.mode) {
    case 'active':
      return t('engineMark.active', {
        pct,
        criterion: t(`engineMark.criterion.${mark.criterion}`),
      });
    case 'observe':
      return t('engineMark.observe', { pct });
    case 'suspended':
      return t('engineMark.suspended', {
        pct,
        reason: mark.reason ? t(`history.suspend.${mark.reason}`) : '',
      });
    default:
      return t('engineMark.off', { pct });
  }
}

interface MarkerProps {
  kind: 'pace' | 'engine';
  pct: number;
  tone?: 'active' | 'muted';
  label: string;
  tip: string;
  testId?: string;
}

/** Trattino sulla barra con tooltip al passaggio del mouse e da tastiera (`aria-describedby`). */
function Marker({ kind, pct, tone, label, tip, testId }: MarkerProps) {
  const tipId = useId();
  return (
    <span
      className="sd-bar__mark"
      data-kind={kind}
      data-tone={tone}
      data-side={pct > 50 ? 'end' : 'start'}
      data-testid={testId}
      role="note"
      tabIndex={0}
      aria-label={label}
      aria-describedby={tipId}
      style={{ left: `${Math.min(100, Math.max(0, pct))}%` }}
    >
      <span id={tipId} role="tooltip" className="sd-bar__tip">
        {tip}
      </span>
    </span>
  );
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
  engineMark,
  exhaustionAt,
  compact = false,
}: UsageBarProps) {
  const { t, locale } = useI18n();
  const known = pct !== null && pct !== undefined && Number.isFinite(pct);
  const shown = known ? displayPct(pct) : null;
  const level = levelFor(pct, thresholds);
  const countdown = countdownTo(resetsAt, now);
  const words = { today: t('date.today'), tomorrow: t('date.tomorrow') };
  const resetWhen = formatWhen(resetsAt, now, locale, words);
  const resetText =
    countdown === null
      ? t('usage.noReset')
      : resetWhen
        ? t('usage.resetsAtWhen', { time: countdown, when: resetWhen })
        : t('usage.resetsIn', { time: countdown });
  const exhaustMs = exhaustionAt ? Date.parse(exhaustionAt) : Number.NaN;
  const resetMs = resetsAt ? Date.parse(resetsAt) : Number.NaN;
  const exhaustWhen =
    !Number.isNaN(exhaustMs) && (Number.isNaN(resetMs) || exhaustMs < resetMs)
      ? formatWhen(exhaustionAt, now, locale, words)
      : null;
  const pctText = shown === null ? t('usage.unknown') : t('usage.pct', { pct: shown });
  const hasPace = expectedPct !== null && expectedPct !== undefined && Number.isFinite(expectedPct);

  return (
    <div className="sd-bar" data-level={level} data-compact={compact || undefined}>
      <div className="sd-bar__head">
        <span>{label}</span>
        <span className="sd-bar__pct">{pctText}</span>
      </div>
      <div className="sd-bar__rail">
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
        </div>
        {hasPace && (
          <Marker
            kind="pace"
            pct={expectedPct}
            label={t('usage.paceLabel', { pct: displayPct(expectedPct) })}
            tip={t('usage.paceTip')}
            testId="pace-mark"
          />
        )}
        {engineMark && (
          <Marker
            kind="engine"
            pct={engineMark.pct}
            tone={engineMark.mode === 'active' ? 'active' : 'muted'}
            label={t('engineMark.label', { pct: engineMark.pct })}
            tip={engineTip(engineMark, t)}
          />
        )}
      </div>
      {(countdown !== null || !compact) && (
        <div className="sd-bar__foot">
          <span title={resetText}>{resetText}</span>
        </div>
      )}
      {exhaustWhen && (
        <div className="sd-bar__exhaust">{t('usage.exhaustsAt', { when: exhaustWhen })}</div>
      )}
    </div>
  );
}
