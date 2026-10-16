import { useCallback, useEffect, useLayoutEffect, useRef, useState, type MouseEvent } from 'react';
import { useBackend } from '../backend/context.js';
import { useAccounts } from '../backend/useAccounts.js';
import type { MiniWindowControls } from '../backend/types.js';
import { useI18n } from '../i18n/index.js';
import { accountName, displayUsage } from '../lib/account.js';
import { displayPct, formatClock, isStale, levelFor } from '../lib/format.js';
import { useEngineMarks } from '../lib/useEngineMarks.js';
import { useNow } from '../lib/useNow.js';
import { UsageBar } from './UsageBar.js';
import { Tooltip } from './Tooltip.js';

/** Durata minima dell'indicazione «in aggiornamento»: una lettura dalla cache sarebbe invisibile. */
const MIN_REFRESH_MS = 600;

/** Percorsi (viewBox 12×12) delle icone dei pulsanti. */
const ICONS = {
  refresh: 'M10 6a4 4 0 1 1-1.2-2.85M10 1.5V4H7.5', // freccia circolare
  compact: 'M2.5 8 6 4.5 9.5 8', // freccia in su: la finestra si riduce a una striscia
  expand: 'M2.5 4.5 6 8l3.5-3.5', // freccia in giù: la finestra si riespande
  hide: 'M3 3l6 6M9 3 3 9', // croce: nasconde la finestra (l'app resta nell'area di notifica)
} as const;

function ToolButton({
  icon,
  label,
  onClick,
  busy = false,
}: {
  icon: keyof typeof ICONS;
  label: string;
  onClick: () => void;
  busy?: boolean;
}) {
  return (
    <Tooltip text={label}>
      <button
        type="button"
        className="sd-mini__tool"
        aria-label={label}
        aria-busy={busy || undefined}
        data-busy={busy || undefined}
        onClick={onClick}
      >
        <svg viewBox="0 0 12 12" width="12" height="12" aria-hidden="true" focusable="false">
          <path
            d={ICONS[icon]}
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
    </Tooltip>
  );
}

/**
 * Pulsanti in alto a destra: «Aggiorna» (solo nella vista estesa), «Comprimi/Espandi» e «Nascondi»
 * (solo se l'host ha `miniWindow`). Il clic non arriva alla striscia.
 */
function Tools({
  win,
  compact,
  refreshing,
  onRefresh,
  onCompact,
}: {
  win: MiniWindowControls | undefined;
  compact: boolean;
  refreshing: boolean;
  onRefresh: () => void;
  onCompact: (next: boolean) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="sd-mini__tools" onClick={(event: MouseEvent) => event.stopPropagation()}>
      {!compact && (
        <ToolButton
          icon="refresh"
          label={t(refreshing ? 'mini.refreshing' : 'mini.refresh')}
          busy={refreshing}
          onClick={() => {
            if (!refreshing) onRefresh();
          }}
        />
      )}
      {win && (
        <>
          <ToolButton
            icon={compact ? 'expand' : 'compact'}
            label={t(compact ? 'mini.expand' : 'mini.compact')}
            onClick={() => onCompact(!compact)}
          />
          <ToolButton
            icon="hide"
            label={t('mini.hide')}
            onClick={() => void win.hide().catch(() => {})}
          />
        </>
      )}
    </div>
  );
}

/**
 * Vista compatta (circa 280×160, più alta se serve): account attivo con le barre 5h, 7d e dei
 * limiti per modello, trattini alle soglie del motore e ora dell'ultimo aggiornamento. Se l'host ha
 * `miniWindow` (app desktop) ci sono i pulsanti per nasconderla e ridurla a una striscia
 * (circa 280×28) con alias e percentuale 5h nel colore della soglia; lo stato compresso lo ricorda
 * l'host, che adatta anche l'altezza della finestra al contenuto.
 */
export function MiniWidget() {
  const { t } = useI18n();
  const win = useBackend().miniWindow;
  const { state, refresh } = useAccounts();
  const now = useNow();
  const marks = useEngineMarks();
  const [pending, setPending] = useState(false);
  /** `null` finché l'host non ha detto com'era (una finestra già ridotta non deve lampeggiare espansa). */
  const [compact, setCompactState] = useState<boolean | null>(win ? null : false);
  const sectionRef = useRef<HTMLElement | null>(null);
  const lastFit = useRef(0);

  useEffect(() => {
    if (!win) {
      setCompactState(false);
      return;
    }
    let live = true;
    win.isCompact().then(
      (value) => live && setCompactState(value),
      () => live && setCompactState(false),
    );
    return () => {
      live = false;
    };
  }, [win]);

  // L'altezza della finestra segue il contenuto (limiti per modello, righe che vanno a capo).
  useLayoutEffect(() => {
    if (!win?.fitHeight || compact !== false) return;
    const height = Math.ceil(sectionRef.current?.offsetHeight ?? 0);
    if (height > 0 && height !== lastFit.current) {
      lastFit.current = height;
      void win.fitHeight(height).catch(() => {});
    }
  });

  const changeCompact = useCallback(
    (next: boolean) => {
      // L'host ridimensiona la finestra prima che la vista cambi; se non riesce resta com'è.
      win?.setCompact(next).then(
        () => setCompactState(next),
        () => {},
      );
    },
    [win],
  );

  const onRefresh = useCallback(() => {
    setPending(true);
    const minimum = new Promise((resolve) => setTimeout(resolve, MIN_REFRESH_MS));
    void Promise.allSettled([refresh(), minimum]).then(() => setPending(false));
  }, [refresh]);
  const refreshing = pending || (state.status === 'ready' && state.refreshing);

  if (compact === null) return <section className="sd-mini" aria-label={t('mini.aria')} />;

  const tools = (
    <Tools
      win={win}
      compact={compact}
      refreshing={refreshing}
      onRefresh={onRefresh}
      onCompact={changeCompact}
    />
  );

  if (compact) {
    let name: string | null = null;
    let text = t('common.loading');
    let level = 'unknown';
    let pct: number | null = null;
    if (state.status === 'error') {
      text = state.message;
    } else if (state.status === 'ready') {
      const active = state.data.accounts.find((a) => a.number === state.data.activeAccountNumber);
      if (!active) {
        text = t('mini.none');
      } else {
        const five = displayUsage(active)?.usage.fiveHour;
        name = accountName(active, t);
        pct = five ? displayPct(five.pct) : null;
        level = levelFor(five?.pct);
        text = `${t('usage.fiveHour')} ${pct === null ? t('usage.unknown') : t('usage.pct', { pct })}`;
      }
    }
    return (
      <section
        className="sd-mini sd-mini--compact"
        aria-label={t('mini.aria')}
        role={state.status === 'error' ? 'alert' : undefined}
        onClick={win ? () => changeCompact(false) : undefined}
      >
        {name !== null && <span className="sd-mini__name">{name}</span>}
        <span className="sd-mini__pct" data-level={level}>
          {text}
        </span>
        {pct !== null && (
          <span className="sd-mini__strip" data-level={level} aria-hidden="true">
            <span style={{ width: `${pct}%` }} />
          </span>
        )}
        {tools}
      </section>
    );
  }

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
        <Tooltip text={t('mini.refresh.tip')}>
          <button type="button" className="sd-btn sd-btn--small" onClick={() => void refresh()}>
            {t('common.retry')}
          </button>
        </Tooltip>
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
      const clock = formatClock(shown?.fetchedAt);
      const stale = shown ? shown.lastKnown || isStale(shown.ageSeconds) : false;
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
              engineMark={marks?.fiveHour}
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
              exhaustionAt={usage.sevenDay.projectedExhaustionAt}
              now={now}
              engineMark={marks?.sevenDay ?? undefined}
              compact
            />
          ) : null}
          {usage?.scoped?.map((window, index) => (
            <UsageBar
              key={window.name ?? index}
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
          {!usage && <div className="sd-mini__foot">{t('account.unavailable')}</div>}
          {shown?.lastKnown && <div className="sd-mini__foot">{t('account.lastKnown')}</div>}
          {clock && (
            <div className="sd-mini__foot sd-mini__updated" data-stale={stale}>
              {t(stale ? 'mini.staleAt' : 'mini.updatedAt', { time: clock })}
            </div>
          )}
        </>
      );
    }
  }

  return (
    <section ref={sectionRef} className="sd-mini" aria-label={t('mini.aria')} data-tools="">
      {body}
      {tools}
    </section>
  );
}
