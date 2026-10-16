import type { Account, CswapAction, ListResponse } from '@swapdeck/types';
import { useMemo, useState, type ReactNode } from 'react';
import { actions } from '../backend/actions.js';
import { useBackend } from '../backend/context.js';
import { errorMessage, useAccounts } from '../backend/useAccounts.js';
import { useI18n } from '../i18n/index.js';
import { accountName } from '../lib/account.js';
import { formatAge } from '../lib/format.js';
import { useEngineMarks } from '../lib/useEngineMarks.js';
import { useNow } from '../lib/useNow.js';
import { AccountCard } from './AccountCard.js';
import { useNavigate } from './navigation.js';
import { AccountActions } from './manage/AccountActions.js';
import { AccountDetailPage } from './manage/AccountDetailPage.js';
import { AddAccount } from './manage/AddAccount.js';
import { QuickSwitch, type QuickStrategy } from './QuickSwitch.js';
import { Tooltip } from './Tooltip.js';

export interface DashboardProps {
  /** Contenuto extra nella barra degli strumenti (per esempio le preferenze). */
  toolbarExtra?: ReactNode;
  /** Mostra «Aggiungi account» e le azioni di gestione su ogni scheda. */
  manage?: boolean;
}

interface Notice {
  tone: 'info' | 'danger';
  text: string;
}

function sortAccounts(accounts: Account[]): Account[] {
  return [...accounts].sort((a, b) => Number(b.active) - Number(a.active) || a.number - b.number);
}

function warningsOf(data: ListResponse): string[] {
  return [...(data.duplicateAccountWarnings ?? []), ...(data.lockstepUsageWarnings ?? [])];
}

/** Dashboard degli account: schede con barre e ritmo, cambio rapido, aggiornamento e storico. */
export function Dashboard({ toolbarExtra, manage = false }: DashboardProps) {
  const { t } = useI18n();
  const backend = useBackend();
  const { state, refresh } = useAccounts();
  const now = useNow();
  const engineMarks = useEngineMarks(state.status === 'ready' ? state.data.accounts : undefined);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const navigate = useNavigate();
  /** Utenza aperta nella pagina di dettaglio (solo con `manage`). */
  const [detail, setDetail] = useState<number | null>(null);

  const accounts = useMemo(
    () => (state.status === 'ready' ? sortAccounts(state.data.accounts) : []),
    [state],
  );

  async function runSwitch(action: CswapAction) {
    setBusy(true);
    setNotice(null);
    try {
      const outcome = await backend.runAction(action);
      const result = outcome.switch;
      if (result?.switched && result.to) {
        const target = accounts.find((a) => a.number === result.to?.number);
        const to = target ? accountName(target, t) : `#${result.to.number ?? '?'}`;
        setNotice({ tone: 'info', text: t('quick.result.switched', { to }) });
      } else if (result) {
        setNotice({ tone: 'info', text: t('quick.result.stay', { message: result.message }) });
      }
    } catch (error) {
      setNotice({
        tone: 'danger',
        text: t('dashboard.switchFailed', { message: errorMessage(error) }),
      });
    } finally {
      setBusy(false);
    }
  }

  const onQuick = (strategy: QuickStrategy) => void runSwitch(actions.switchBy(strategy));
  const onSwitch = (account: Account) => void runSwitch(actions.switchTo(account.number));

  const cardFor = (account: Account, withManage: boolean) => (
    <AccountCard
      key={account.number}
      account={account}
      now={now}
      engine={engineMarks}
      busy={busy}
      onSwitch={onSwitch}
      {...(navigate
        ? {
            onOpenHistory: (a: Account) => navigate({ tab: 'history', account: a.number }),
          }
        : {})}
      {...(withManage ? { extra: <AccountActions account={account} onManage={setDetail} /> } : {})}
    />
  );

  const liveNotice = (
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

  if (manage && detail !== null && state.status === 'ready') {
    const account = accounts.find((a) => a.number === detail);
    return (
      <main className="sd-app">
        {liveNotice}
        <AccountDetailPage
          account={account}
          accounts={accounts}
          card={account ? cardFor(account, false) : null}
          onBack={(text) => {
            setDetail(null);
            setNotice(text ? { tone: 'info', text } : null);
          }}
          onNumberChange={setDetail}
        />
      </main>
    );
  }

  const updated =
    state.status === 'ready'
      ? t('dashboard.updated', { age: formatAge(Math.max(0, (now - state.receivedAt) / 1000)) })
      : null;

  return (
    <main className="sd-app" aria-labelledby="sd-dashboard-title">
      <div className="sd-toolbar">
        <h1 id="sd-dashboard-title">{t('dashboard.title')}</h1>
        <div className="sd-toolbar__group">
          <QuickSwitch onSwitch={onQuick} disabled={busy || state.status !== 'ready'} />
          <Tooltip text={t('dashboard.refresh.tip')}>
            <button
              type="button"
              className="sd-btn sd-btn--small"
              disabled={
                state.status === 'loading' || (state.status === 'ready' && state.refreshing)
              }
              onClick={() => void refresh()}
            >
              {state.status === 'ready' && state.refreshing
                ? t('dashboard.refreshing')
                : t('common.refresh')}
            </button>
          </Tooltip>
        </div>
      </div>
      {(updated || toolbarExtra) && (
        <div className="sd-toolbar">
          <span className="sd-updated">{updated}</span>
          {toolbarExtra}
        </div>
      )}

      {liveNotice}

      {manage && <AddAccount />}

      {state.status === 'loading' && (
        <div className="sd-state" role="status">
          {t('dashboard.loading')}
        </div>
      )}

      {state.status === 'error' && (
        <div className="sd-state" role="alert">
          <h2>{t('dashboard.error.title')}</h2>
          <p>{state.message}</p>
          <p>{t('dashboard.error.hint')}</p>
          <Tooltip text={t('dashboard.refresh.tip')}>
            <button type="button" className="sd-btn" onClick={() => void refresh()}>
              {t('common.retry')}
            </button>
          </Tooltip>
        </div>
      )}

      {state.status === 'ready' && accounts.length === 0 && (
        <div className="sd-state">
          <h2>{t('dashboard.empty.title')}</h2>
          <p>{t('dashboard.empty.hint')}</p>
        </div>
      )}

      {state.status === 'ready' && accounts.length > 0 && (
        <>
          {warningsOf(state.data).length > 0 && (
            <div className="sd-notice" role="status">
              <strong>{t('dashboard.warnings')}</strong>
              <ul>
                {warningsOf(state.data).map((w) => (
                  <li key={w}>{w}</li>
                ))}
              </ul>
            </div>
          )}
          {state.data.activeAccountNumber === null && (
            <div className="sd-notice" role="status">
              {t('dashboard.unmanaged')}
            </div>
          )}
          <ul className="sd-list" aria-label={t('dashboard.title')}>
            {accounts.map((account) => cardFor(account, manage))}
          </ul>
        </>
      )}
    </main>
  );
}
