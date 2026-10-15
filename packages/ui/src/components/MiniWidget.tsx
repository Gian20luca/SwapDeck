import { useAccounts } from '../backend/useAccounts.js';
import { useI18n } from '../i18n/index.js';
import { accountName, displayUsage } from '../lib/account.js';
import { useNow } from '../lib/useNow.js';
import { UsageBar } from './UsageBar.js';

/** Vista compatta (circa 280×160): account attivo con le barre 5h e 7d. */
export function MiniWidget() {
  const { t } = useI18n();
  const { state, refresh } = useAccounts();
  const now = useNow();

  let body;
  if (state.status === 'loading') {
    body = (
      <div className="sd-mini__foot" role="status">
        {t('common.loading')}
      </div>
    );
  } else if (state.status === 'error') {
    body = (
      <div role="alert">
        <div className="sd-mini__foot">{state.message}</div>
        <button type="button" className="sd-btn sd-btn--small" onClick={() => void refresh()}>
          {t('common.retry')}
        </button>
      </div>
    );
  } else {
    const active = state.data.accounts.find((a) => a.number === state.data.activeAccountNumber);
    const shown = active ? displayUsage(active) : null;
    if (!active) {
      body = <div className="sd-mini__foot">{t('mini.none')}</div>;
    } else {
      const name = accountName(active, t);
      const usage = shown?.usage;
      body = (
        <>
          <div className="sd-mini__head">
            <span className="sd-mini__name" title={active.email}>
              {name}
            </span>
            <span className="sd-badge sd-badge--ok">{t('account.active')}</span>
          </div>
          {usage?.fiveHour ? (
            <UsageBar
              label={t('usage.fiveHour')}
              accountName={name}
              pct={usage.fiveHour.pct}
              resetsAt={usage.fiveHour.resetsAt}
              now={now}
              compact
            />
          ) : null}
          {usage?.sevenDay ? (
            <UsageBar
              label={t('usage.sevenDay')}
              accountName={name}
              pct={usage.sevenDay.pct}
              resetsAt={usage.sevenDay.resetsAt}
              expectedPct={usage.sevenDay.expectedPct}
              now={now}
              compact
            />
          ) : null}
          {!usage && <div className="sd-mini__foot">{t('account.unavailable')}</div>}
          {shown?.lastKnown && <div className="sd-mini__foot">{t('account.lastKnown')}</div>}
        </>
      );
    }
  }

  return (
    <section className="sd-mini" aria-label={t('mini.aria')}>
      {body}
    </section>
  );
}
