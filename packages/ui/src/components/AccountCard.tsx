import type { Account } from '@swapdeck/types';
import type { ReactNode } from 'react';
import { accountName, displayUsage, statusBadge, switchBlockedReason } from '../lib/account.js';
import type { EngineMarks } from '../lib/engineMarks.js';
import { formatAge, formatClock, isStale, shortEmail } from '../lib/format.js';
import { useI18n } from '../i18n/index.js';
import { PaceIndicator } from './PaceIndicator.js';
import { UsageBar } from './UsageBar.js';
import { Tooltip } from './Tooltip.js';

export interface AccountCardProps {
  account: Account;
  /** Istante di riferimento (ms) per i countdown. */
  now: number;
  /** Cambio in corso: disabilita il pulsante. */
  busy?: boolean;
  onSwitch: (account: Account) => void;
  /** Apre lo storico di questo account (scheda Storico); senza, il pulsante non c'è. */
  onOpenHistory?: (account: Account) => void;
  /** Azioni di gestione (alias, slot, rimozione, ...), sotto i pulsanti della scheda. */
  extra?: ReactNode;
  /** Trattini alle soglie del motore: solo sulle barre dell'account attivo. */
  engine?: EngineMarks | null | undefined;
}

/** Scheda di un account: identità, stato, barre 5h e 7d, ritmo, limiti per modello, età del dato, azioni. */
export function AccountCard({
  account,
  now,
  busy = false,
  onSwitch,
  onOpenHistory,
  extra,
  engine,
}: AccountCardProps) {
  const { t } = useI18n();
  const name = accountName(account, t);
  const badge = statusBadge(account.usageStatus);
  const shown = displayUsage(account);
  const usage = shown?.usage;
  const blocked = switchBlockedReason(account);
  const disabled = account.disabled === true;
  const stale = shown ? shown.lastKnown || isStale(shown.ageSeconds) : false;
  const headingId = `account-${account.number}-name`;
  const org =
    account.isOrganization && account.organizationName
      ? account.organizationName
      : t('account.personal');

  let ageText: string | null = null;
  if (shown && shown.ageSeconds !== null) {
    const age = formatAge(shown.ageSeconds);
    ageText = stale ? t('usage.stale', { age }) : t('usage.age', { age });
    const clock = formatClock(shown.fetchedAt);
    if (clock) ageText += ' · ' + t('usage.atTime', { time: clock });
  }

  return (
    <li
      className="sd-card"
      data-active={account.active}
      data-disabled={disabled}
      aria-labelledby={headingId}
    >
      <div className="sd-card__head">
        <div className="sd-card__title">
          <span id={headingId} className="sd-card__name">
            {name}
          </span>
          <span className="sd-card__email" title={account.email}>
            {shortEmail(account.email)} · {org}
          </span>
        </div>
        <div className="sd-card__badges">
          {account.active && <span className="sd-badge sd-badge--ok">{t('account.active')}</span>}
          {disabled && <span className="sd-badge sd-badge--warn">{t('account.disabled')}</span>}
          {badge && (
            <span
              className={`sd-badge${badge.tone === 'neutral' ? '' : ` sd-badge--${badge.tone}`}`}
            >
              {t(badge.key)}
            </span>
          )}
        </div>
      </div>

      {usage && (
        <div className="sd-card__bars">
          {usage.fiveHour && (
            <UsageBar
              label={t('usage.fiveHour')}
              accountName={name}
              pct={usage.fiveHour.pct}
              resetsAt={usage.fiveHour.resetsAt}
              now={now}
              engineMark={account.active ? engine?.fiveHour : undefined}
            />
          )}
          {usage.sevenDay && (
            <UsageBar
              label={t('usage.sevenDay')}
              accountName={name}
              pct={usage.sevenDay.pct}
              resetsAt={usage.sevenDay.resetsAt}
              expectedPct={usage.sevenDay.expectedPct}
              now={now}
              engineMark={account.active ? (engine?.sevenDay ?? undefined) : undefined}
            />
          )}
          {usage.sevenDay && (
            <PaceIndicator
              expectedPct={usage.sevenDay.expectedPct}
              aheadOfPace={usage.sevenDay.aheadOfPace}
              willLastToReset={usage.sevenDay.willLastToReset}
              projectedExhaustionAt={usage.sevenDay.projectedExhaustionAt}
            />
          )}
        </div>
      )}

      {usage?.scoped && usage.scoped.length > 0 && (
        <div className="sd-card__scoped">
          <h3>{t('usage.scoped')}</h3>
          {usage.scoped.map((window, index) => (
            <UsageBar
              key={`${window.name ?? index}`}
              label={window.name ?? t('usage.sevenDay')}
              accountName={name}
              pct={window.pct}
              resetsAt={window.resetsAt}
              expectedPct={window.expectedPct}
              exhaustionAt={window.projectedExhaustionAt}
              now={now}
              compact
            />
          ))}
        </div>
      )}

      <div className="sd-card__foot">
        <span className="sd-card__age" data-stale={stale}>
          {shown?.lastKnown ? `${t('account.lastKnown')}${ageText ? ' · ' : ''}` : null}
          {ageText}
        </span>
        <span className="sd-card__actions">
          {onOpenHistory && usage && (
            <Tooltip text={t('account.history.tip')}>
              <button
                type="button"
                className="sd-btn sd-btn--small"
                aria-label={t('account.historyOpen', { name })}
                onClick={() => onOpenHistory(account)}
              >
                {t('account.history')}
              </button>
            </Tooltip>
          )}
          <Tooltip text={blocked ? t(blocked) : t('account.switchTo.tip')}>
            <button
              type="button"
              className="sd-btn sd-btn--small sd-btn--primary"
              disabled={busy || blocked !== null}
              aria-label={t('account.switchToLabel', { name })}
              onClick={() => onSwitch(account)}
            >
              {t('account.switchTo')}
            </button>
          </Tooltip>
        </span>
      </div>

      {extra}
    </li>
  );
}
