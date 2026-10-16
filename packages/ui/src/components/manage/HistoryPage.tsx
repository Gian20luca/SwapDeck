import type { Account, Decision, IdleReason, SuspendReason, SwitchCause } from '@swapdeck/types';
import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { useBackend } from '../../backend/context.js';
import { useAccounts } from '../../backend/useAccounts.js';
import { useI18n, type MessageKey, type Translate } from '../../i18n/index.js';
import { accountName } from '../../lib/account.js';
import { mergeEvents, type EventRow } from '../../lib/historyEvents.js';
import { useLoad } from '../../lib/useLoad.js';
import { HistoryPanel } from '../HistoryPanel.js';
import { LoadGate, Section } from './common.js';
import { Tooltip } from '../Tooltip.js';

const RANGES = [
  { hours: 24, label: 'history.range.24h' },
  { hours: 168, label: 'history.range.7d' },
  { hours: 720, label: 'history.range.30d' },
] as const satisfies readonly { hours: number; label: MessageKey }[];

const CAUSE_KEYS = {
  primaryAtLimit: 'history.cause.primaryAtLimit',
  reserveAtLimit: 'history.cause.reserveAtLimit',
  activeNeedsRelogin: 'history.cause.activeNeedsRelogin',
  primaryReset: 'history.cause.primaryReset',
  reservesAtLimit: 'history.cause.reservesAtLimit',
  earlierAvailable: 'history.cause.earlierAvailable',
} as const satisfies Record<SwitchCause, MessageKey>;

const IDLE_KEYS = {
  belowThreshold: 'history.idle.belowThreshold',
  onReserve: 'history.idle.onReserve',
  cooldown: 'history.idle.cooldown',
  staleUsage: 'history.idle.staleUsage',
  activeExcluded: 'history.idle.activeExcluded',
  activeOutsideRules: 'history.idle.activeOutsideRules',
} as const satisfies Record<IdleReason, MessageKey>;

const SUSPEND_KEYS = {
  disabled: 'history.suspend.disabled',
  invalidRules: 'history.suspend.invalidRules',
  conflict: 'history.suspend.conflict',
  noActiveAccount: 'history.suspend.noActiveAccount',
  noViableTarget: 'history.suspend.noViableTarget',
} as const satisfies Record<SuspendReason, MessageKey>;

const PAGE_SIZE = 25;

export function decisionText(decision: Decision, t: Translate): string {
  switch (decision.kind) {
    case 'noAction':
      return t('history.decision.noAction', { reason: t(IDLE_KEYS[decision.reason]) });
    case 'switch':
      return t('history.decision.switch', {
        from: decision.from,
        to: decision.to,
        cause: t(CAUSE_KEYS[decision.cause]),
      });
    case 'returnToPrimary':
      return t('history.decision.return', {
        from: decision.from,
        to: decision.to,
        cause: t(CAUSE_KEYS[decision.cause]),
      });
    case 'suspended':
      return t('history.decision.suspended', { reason: t(SUSPEND_KEYS[decision.reason]) });
  }
}

/** Storico: grafici per account con scelta dell'intervallo, cambi di account e decisioni del motore. */
export function HistoryPage({ target }: { target?: { account: number; key: number } | null }) {
  const { t } = useI18n();
  const { state } = useAccounts();
  const [hours, setHours] = useState<number>(168);
  const [openAccounts, setOpenAccounts] = useState<Set<number> | null>(null);
  const accounts: Account[] = state.status === 'ready' ? state.data.accounts : [];
  const open = openAccounts ?? new Set(accounts.filter((a) => a.active).map((a) => a.number));
  const rangeId = useId();
  const toggles = useRef<Map<number, HTMLButtonElement>>(new Map());
  const [placed, setPlaced] = useState<number | null>(null);

  // Arrivo da «Storico» di un account: solo il suo pannello aperto, scorrimento fin lì e focus al pulsante.
  useEffect(() => {
    if (target) setOpenAccounts(new Set([target.account]));
  }, [target]);
  useEffect(() => {
    if (!target || placed === target.key) return;
    const button = toggles.current.get(target.account);
    if (!button) return;
    setPlaced(target.key);
    button.scrollIntoView?.({ block: 'start' });
    button.focus();
  }, [target, placed, accounts.length]);

  function toggle(n: number) {
    const next = new Set(open);
    if (next.has(n)) next.delete(n);
    else next.add(n);
    setOpenAccounts(next);
  }

  return (
    <div className="sd-page">
      <h2>{t('history.title')}</h2>
      <div className="sd-row" role="group" aria-labelledby={rangeId}>
        <span id={rangeId} className="sd-label">
          {t('history.range')}
        </span>
        {RANGES.map((range) => (
          <Tooltip key={range.hours} text={t('history.range.tip')}>
            <button
              type="button"
              className={`sd-btn sd-btn--small${hours === range.hours ? ' sd-btn--primary' : ''}`}
              aria-pressed={hours === range.hours}
              onClick={() => setHours(range.hours)}
            >
              {t(range.label)}
            </button>
          </Tooltip>
        ))}
      </div>

      <Section title={t('history.charts')}>
        {state.status === 'loading' && (
          <div className="sd-state" role="status">
            {t('common.loading')}
          </div>
        )}
        {state.status === 'error' && (
          <div className="sd-state" role="alert">
            <p>{state.message}</p>
          </div>
        )}
        {state.status === 'ready' && accounts.length === 0 && (
          <div className="sd-state">
            <p>{t('dashboard.empty.title')}</p>
          </div>
        )}
        <ul className="sd-list" aria-label={t('history.charts')}>
          {accounts.map((account) => {
            const name = accountName(account, t);
            const isOpen = open.has(account.number);
            return (
              <li key={account.number} className="sd-card">
                <div className="sd-toolbar">
                  <span className="sd-card__name">{name}</span>
                  <Tooltip text={t('history.toggle.tip')}>
                    <button
                      type="button"
                      ref={(node) => {
                        if (node) toggles.current.set(account.number, node);
                        else toggles.current.delete(account.number);
                      }}
                      className="sd-btn sd-btn--small"
                      aria-expanded={isOpen}
                      aria-label={t(isOpen ? 'account.historyHide' : 'account.historyLabel', {
                        name,
                      })}
                      onClick={() => toggle(account.number)}
                    >
                      {t(isOpen ? 'history.hide' : 'history.show')}
                    </button>
                  </Tooltip>
                </div>
                {isOpen && (
                  <div className="sd-card__history">
                    <HistoryPanel account={account.number} rangeHours={hours} />
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      </Section>

      <EventsSection hours={hours} />
    </div>
  );
}

function EventsSection({ hours }: { hours: number }) {
  const { t, locale } = useI18n();
  const backend = useBackend();
  const supported = typeof backend.getHistoryEvents === 'function';
  const load = useCallback(
    () =>
      backend.getHistoryEvents
        ? backend.getHistoryEvents({ sinceSeconds: Math.floor(Date.now() / 1000) - hours * 3600 })
        : Promise.reject(new Error('n/d')),
    [backend, hours],
  );
  const { state, reload } = useLoad(load, [load]);
  const [shown, setShown] = useState(PAGE_SIZE);
  const format = useMemo(
    () => new Intl.DateTimeFormat(locale, { dateStyle: 'short', timeStyle: 'short' }),
    [locale],
  );
  if (!supported) return null;

  return (
    <Section title={t('history.events')}>
      <LoadGate state={state} onRetry={() => void reload()}>
        {(events) => {
          const rows = mergeEvents(events);
          if (rows.length === 0) {
            return (
              <div className="sd-state">
                <p>{t('history.events.empty')}</p>
              </div>
            );
          }
          return (
            <>
              <ul className="sd-list" aria-label={t('history.events')}>
                {rows.slice(0, shown).map((row) => (
                  <EventItem
                    key={`${row.kind}:${row.ts}:${eventId(row)}`}
                    row={row}
                    time={format.format(new Date(row.ts * 1000))}
                  />
                ))}
              </ul>
              {rows.length > shown && (
                <div className="sd-row">
                  <Tooltip text={t('history.more.tip')}>
                    <button
                      type="button"
                      className="sd-btn sd-btn--small"
                      onClick={() => setShown((n) => n + PAGE_SIZE)}
                    >
                      {t('history.events.more', { count: rows.length - shown })}
                    </button>
                  </Tooltip>
                </div>
              )}
            </>
          );
        }}
      </LoadGate>
    </Section>
  );
}

function eventId(row: EventRow): string {
  return row.kind === 'switch'
    ? `${row.record.from ?? 'x'}-${row.record.to}`
    : JSON.stringify(row.record.decision);
}

function EventItem({ row, time }: { row: EventRow; time: string }) {
  const { t } = useI18n();
  if (row.kind === 'switch') {
    const { record } = row;
    return (
      <li className="sd-card sd-event" data-kind="switch">
        <span className="sd-event__time">{time}</span>
        <span className="sd-badge">{t('history.kind.switch')}</span>
        <span className="sd-event__text">
          {t('history.switch.text', {
            from: record.from ?? '—',
            to: record.to,
            cause: t(CAUSE_KEYS[record.cause]),
          })}
          {' · '}
          {t(record.switched ? 'history.switch.ok' : 'history.switch.failed')}
          {record.message ? ` (${record.message})` : ''}
        </span>
      </li>
    );
  }
  const { record } = row;
  return (
    <li className="sd-card sd-event" data-kind="decision">
      <span className="sd-event__time">{time}</span>
      <span className="sd-badge">{t('history.kind.decision')}</span>
      <span className="sd-event__text">
        {decisionText(record.decision, t)}
        {record.dryRun ? ` · ${t('history.dryRun')}` : ''}
      </span>
    </li>
  );
}
