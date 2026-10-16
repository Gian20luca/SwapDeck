import type { Account } from '@swapdeck/types';
import { useI18n } from '../i18n/index.js';
import { accountName, displayUsage, statusBadge } from '../lib/account.js';
import { displayPct } from '../lib/format.js';
import { Tooltip } from './Tooltip.js';
import { resetLabel } from './UsageBar.js';

/**
 * Intestazione compatta di un account per le viste di dettaglio (per esempio lo Storico): nome,
 * badge di stato e «5h 2% · 7d 37%» con i reset nei tooltip. Le barre complete con le soglie
 * restano nella dashboard e nel mini-widget.
 */
export function AccountSummary({ account, now }: { account: Account; now: number }) {
  const { t, locale } = useI18n();
  const usage = displayUsage(account)?.usage;
  const status = statusBadge(account.usageStatus);
  const badge = account.disabled
    ? { text: t('account.disabled'), tone: 'warn' }
    : status
      ? { text: t(status.key), tone: status.tone }
      : account.active
        ? { text: t('account.active'), tone: 'ok' }
        : null;
  const part = (label: string, pct: number | undefined, resetsAt: string | null | undefined) => (
    <Tooltip text={resetLabel(resetsAt, now, t, locale)}>
      <span className="sd-summary__pct" tabIndex={0}>
        {label} {pct === undefined ? '–' : `${displayPct(pct)}%`}
      </span>
    </Tooltip>
  );
  return (
    <span className="sd-summary" data-testid={`summary-${account.number}`}>
      <span className="sd-card__name">{accountName(account, t)}</span>
      {badge && <span className={`sd-badge sd-badge--${badge.tone}`}>{badge.text}</span>}
      <span className="sd-summary__usage">
        {part('5h', usage?.fiveHour?.pct, usage?.fiveHour?.resetsAt)}
        {' · '}
        {part('7d', usage?.sevenDay?.pct, usage?.sevenDay?.resetsAt)}
      </span>
    </span>
  );
}
