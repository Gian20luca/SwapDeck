import type {
  BreakdownWindow,
  OtherDevicesEstimate,
  UsageBreakdown,
  UsageGroupBy,
  UsageQuery,
  UsageRow,
} from '@swapdeck/types';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useBackend } from '../../backend/context.js';
import { useAccounts } from '../../backend/useAccounts.js';
import { useI18n, type MessageKey, type Translate } from '../../i18n/index.js';
import { accountName } from '../../lib/account.js';
import { formatPct, formatTokens, rowLabel } from '../../lib/usageView.js';
import { useLoad } from '../../lib/useLoad.js';
import { Tooltip } from '../Tooltip.js';
import { LoadGate, Section, SelectField } from './common.js';

const WINDOWS = [
  { id: '5h', label: 'usage.window.5h' },
  { id: '7d', label: 'usage.window.7d' },
] as const satisfies readonly { id: BreakdownWindow; label: MessageKey }[];

const GROUPS = [
  { id: 'project', label: 'usage.group.project' },
  { id: 'session', label: 'usage.group.session' },
  { id: 'agent', label: 'usage.group.agent' },
  { id: 'model', label: 'usage.group.model' },
] as const satisfies readonly { id: UsageGroupBy; label: MessageKey }[];

const CONFIDENCE_KEYS = {
  low: 'usage.confidence.low',
  medium: 'usage.confidence.medium',
  high: 'usage.confidence.high',
} as const satisfies Record<'low' | 'medium' | 'high', MessageKey>;

/** Dopo quanto un caricamento è la «prima analisi» (le letture successive durano millisecondi). */
const FIRST_SCAN_AFTER_MS = 600;

/**
 * Chi consuma (F20): classifica dei consumi di Claude Code di questo PC per progetto, sessione,
 * agente o modello, con la stima degli altri dispositivi. `target` arriva dai link «Chi consuma
 * questa utenza» (dashboard e Storico); `key` cambia a ogni richiesta.
 */
export function UsagePage({ target }: { target?: { account: number; key: number } | null }) {
  const { t, locale } = useI18n();
  const backend = useBackend();
  const { state: accountsState } = useAccounts();
  const accounts = accountsState.status === 'ready' ? accountsState.data.accounts : [];
  const [account, setAccount] = useState<string>(target ? String(target.account) : 'all');
  const [window, setWindow] = useState<BreakdownWindow>('5h');
  const [groupBy, setGroupBy] = useState<UsageGroupBy>('project');
  const supported = typeof backend.getUsageBreakdown === 'function';

  useEffect(() => {
    if (target) setAccount(String(target.account));
  }, [target]);

  // Una lettura lenta (la prima) non deve sovrascrivere una richiesta più recente.
  const seq = useRef(0);
  const load = useCallback(async (): Promise<UsageBreakdown> => {
    if (!backend.getUsageBreakdown) throw new Error('n/d');
    const mine = ++seq.current;
    const query: UsageQuery = {
      window,
      groupBy,
      ...(account === 'all' ? {} : { account: Number(account) }),
    };
    const result = await backend.getUsageBreakdown(query);
    // Superata da una richiesta più recente: non si risolve, così non tocca lo stato.
    if (mine !== seq.current) return new Promise<UsageBreakdown>(() => undefined);
    return result;
  }, [backend, window, groupBy, account]);
  const { state, reload } = useLoad(load, [load]);

  const nameOf = useCallback(
    (n: number) => {
      const found = accounts.find((a) => a.number === n);
      return found ? accountName(found, t) : `#${n}`;
    },
    [accounts, t],
  );
  const options = useMemo(() => {
    const list = accounts.map((a) => ({
      value: String(a.number),
      label: `${accountName(a, t)} (#${a.number})`,
    }));
    if (account !== 'all' && !list.some((o) => o.value === account)) {
      list.push({ value: account, label: `#${account}` });
    }
    return [{ value: 'all', label: t('usage.account.all') }, ...list];
  }, [accounts, account, t]);

  return (
    <div className="sd-page sd-usage">
      <h2>{t('usage.title')}</h2>
      {supported && (
        <div className="sd-usage__controls">
          <SelectField
            label={t('usage.account')}
            tip={t('usage.account.tip')}
            value={account}
            onChange={setAccount}
            options={options}
          />
          <Choice
            label={t('usage.window')}
            tip={t('usage.window.tip')}
            items={WINDOWS}
            value={window}
            onChange={setWindow}
          />
          <Choice
            label={t('usage.groupBy')}
            tip={t('usage.group.tip')}
            items={GROUPS}
            value={groupBy}
            onChange={setGroupBy}
          />
        </div>
      )}

      {!supported ? (
        <div className="sd-state">
          <p>{t('common.unavailable')}</p>
        </div>
      ) : (
        <LoadGate state={state} onRetry={() => void reload()} loading={<FirstScan />}>
          {(data) => <Result data={data} groupBy={groupBy} locale={locale} t={t} nameOf={nameOf} />}
        </LoadGate>
      )}

      <p className="sd-hint sd-usage__privacy" role="note">
        {t('usage.privacy')}
      </p>
    </div>
  );
}

/** Gruppo di pulsanti a scelta singola (come l'intervallo dello Storico). */
function Choice<T extends string>({
  label,
  tip,
  items,
  value,
  onChange,
}: {
  label: string;
  tip: string;
  items: readonly { id: T; label: MessageKey }[];
  value: T;
  onChange: (id: T) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="sd-form__field">
      <span className="sd-label">{label}</span>
      <div className="sd-row" role="group" aria-label={label}>
        {items.map((item) => (
          <Tooltip key={item.id} text={tip}>
            <button
              type="button"
              className={`sd-btn sd-btn--small${value === item.id ? ' sd-btn--primary' : ''}`}
              aria-pressed={value === item.id}
              onClick={() => onChange(item.id)}
            >
              {t(item.label)}
            </button>
          </Tooltip>
        ))}
      </div>
    </div>
  );
}

/** Caricamento: subito «Caricamento…», dopo poco la prima analisi, con avanzamento indeterminato. */
function FirstScan() {
  const { t } = useI18n();
  const [slow, setSlow] = useState(false);
  useEffect(() => {
    const timer = setTimeout(() => setSlow(true), FIRST_SCAN_AFTER_MS);
    return () => clearTimeout(timer);
  }, []);
  return (
    <div className="sd-state" role="status">
      <p>{slow ? t('usage.firstScan') : t('common.loading')}</p>
      {slow && (
        <>
          <progress className="sd-usage__progress" aria-label={t('usage.firstScan')} />
          <p className="sd-hint">{t('usage.firstScan.hint')}</p>
        </>
      )}
    </div>
  );
}

function Bar({ pct, label }: { pct: number; label: string }) {
  return (
    <div className="sd-bar__track" role="img" aria-label={label}>
      <div
        className="sd-bar__fill"
        style={{ width: `${Math.min(100, Math.max(pct > 0 ? 1 : 0, pct))}%` }}
      />
    </div>
  );
}

function Result({
  data,
  groupBy,
  locale,
  t,
  nameOf,
}: {
  data: UsageBreakdown;
  groupBy: UsageGroupBy;
  locale: string;
  t: Translate;
  nameOf: (n: number) => string;
}) {
  const date = useMemo(
    () => new Intl.DateTimeFormat(locale, { dateStyle: 'short', timeStyle: 'short' }),
    [locale],
  );
  const when = (seconds: number) => date.format(new Date(seconds * 1000));
  const tokens = (n: number) => formatTokens(n, locale);

  if (!data.sourceAvailable) {
    return (
      <div className="sd-state">
        <p>{t('usage.noSource')}</p>
      </div>
    );
  }
  return (
    <>
      <ScanNotices data={data} t={t} />
      <Section
        title={t('usage.ranking')}
        hint={
          <>
            {t('usage.total', { tokens: tokens(data.total.equivalent) })}
            {' · '}
            {t('usage.range', { from: when(data.from), to: when(data.to) })}
          </>
        }
      >
        {data.rows.length === 0 ? (
          <div className="sd-state">
            <p>{t('usage.empty')}</p>
          </div>
        ) : (
          <ul className="sd-list sd-usage__list" aria-label={t('usage.ranking')}>
            {data.rows.map((row) => (
              <RankRow
                key={row.key}
                row={row}
                groupBy={groupBy}
                locale={locale}
                t={t}
                tokens={tokens}
                when={when}
              />
            ))}
          </ul>
        )}
        {data.truncatedRows > 0 && (
          <p className="sd-hint">{t('usage.truncated', { count: data.truncatedRows })}</p>
        )}
        {data.unattributed.messages > 0 && (
          <div className="sd-notice sd-usage__unknown" role="note">
            <strong>{t('usage.unknown.title')}</strong>
            <p>{t('usage.unknown.text', { tokens: tokens(data.unattributed.equivalent) })}</p>
          </div>
        )}
      </Section>
      <OtherDevices data={data} locale={locale} t={t} tokens={tokens} nameOf={nameOf} when={when} />
    </>
  );
}

function ScanNotices({ data, t }: { data: UsageBreakdown; t: Translate }) {
  const keys: MessageKey[] = [];
  if (data.scan.indexReset) keys.push('usage.scan.indexReset');
  if (data.scan.saveFailed) keys.push('usage.scan.saveFailed');
  if (data.scan.incomplete) keys.push('usage.scan.incomplete');
  if (keys.length === 0) return null;
  return (
    <div className="sd-notice" role="note">
      <ul>
        {keys.map((key) => (
          <li key={key}>{t(key)}</li>
        ))}
      </ul>
    </div>
  );
}

function RankRow({
  row,
  groupBy,
  locale,
  t,
  tokens,
  when,
}: {
  row: UsageRow;
  groupBy: UsageGroupBy;
  locale: string;
  t: Translate;
  tokens: (n: number) => string;
  when: (s: number) => string;
}) {
  const label = rowLabel(row, groupBy, t('usage.main'));
  const pct = formatPct(row.sharePct, locale);
  return (
    <li className="sd-card sd-bar sd-usage__row">
      <div className="sd-bar__head">
        <span className="sd-usage__name">{label.title}</span>
        <span className="sd-bar__pct">{pct}%</span>
      </div>
      {label.sub && <div className="sd-usage__sub">{label.sub}</div>}
      <Bar pct={row.sharePct} label={t('usage.row.bar', { pct })} />
      <div className="sd-usage__foot">
        {t('usage.row.foot', {
          tokens: tokens(row.totals.equivalent),
          messages: row.totals.messages,
          when: when(row.lastUsed),
        })}
      </div>
    </li>
  );
}

function OtherDevices({
  data,
  locale,
  t,
  tokens,
  nameOf,
  when,
}: {
  data: UsageBreakdown;
  locale: string;
  t: Translate;
  tokens: (n: number) => string;
  nameOf: (n: number) => string;
  when: (s: number) => string;
}) {
  const estimates = data.otherDevices.filter((e) => e.window === data.window);
  if (estimates.length === 0) return null;
  return (
    <Section title={t('usage.other.title')}>
      <ul className="sd-list" aria-label={t('usage.other.title')}>
        {estimates.map((e) => (
          <li key={`${e.account}:${e.window}`} className="sd-card sd-usage__other">
            <div className="sd-usage__otherhead">
              <strong>{nameOf(e.account)}</strong>
              {e.status === 'available' && e.confidence && (
                <Tooltip text={t('usage.confidence.tip')}>
                  <span className="sd-badge" data-confidence={e.confidence} tabIndex={0}>
                    {t(CONFIDENCE_KEYS[e.confidence])}
                  </span>
                </Tooltip>
              )}
            </div>
            <EstimateBody
              e={e}
              share={data.account === e.account ? data.otherDevicesSharePct : null}
              locale={locale}
              t={t}
              tokens={tokens}
              when={when}
            />
          </li>
        ))}
      </ul>
    </Section>
  );
}

function EstimateBody({
  e,
  share,
  locale,
  t,
  tokens,
  when,
}: {
  e: OtherDevicesEstimate;
  share: number | null;
  locale: string;
  t: Translate;
  tokens: (n: number) => string;
  when: (s: number) => string;
}) {
  if (e.status === 'noData') return <p className="sd-hint">{t('usage.other.noData')}</p>;
  if (e.status === 'notCalibrated' || e.otherPct === null) {
    return (
      <>
        <p>{t('usage.other.unavailable')}</p>
        <p className="sd-hint">{t('usage.other.unavailableHint')}</p>
      </>
    );
  }
  const pct = formatPct(e.otherPct, locale);
  return (
    <>
      <p>{t('usage.other.value', { pct, tokens: tokens(e.otherEquivalent ?? 0) })}</p>
      {share !== null && (
        <Bar pct={share} label={t('usage.other.bar', { pct: formatPct(share, locale) })} />
      )}
      <p className="sd-hint">
        {t('usage.other.detail', {
          server: formatPct(e.serverPct ?? 0, locale),
          local: formatPct(e.localPct ?? 0, locale),
          other: pct,
        })}
        {e.to !== null ? ` · ${when(e.to)}` : ''}
      </p>
      {e.historyGaps && <p className="sd-hint">{t('usage.other.gaps')}</p>}
    </>
  );
}
