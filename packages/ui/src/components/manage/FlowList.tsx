import { useEffect, useId, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react';
import { useI18n } from '../../i18n/index.js';
import { displayUsage, switchBlockedReason } from '../../lib/account.js';
import type { FlowRow } from '../../lib/engineRules.js';
import { displayPct } from '../../lib/format.js';
import { Tooltip } from '../Tooltip.js';

export interface FlowListProps {
  rows: FlowRow[];
  nameOf: (number: number) => string;
  disabled?: boolean;
  /**
   * Sposta l'utenza `number` in posizione `to`. Per numero e non per indice: durante un
   * trascinamento più movimenti arrivano prima che la lista si ridisegni.
   */
  onMove: (number: number, to: number) => void;
  onToggle: (number: number, included: boolean) => void;
  onUseNow: (number: number) => void;
}

function pct(value: number | null | undefined): string {
  return value === null || value === undefined ? '–' : `${displayPct(value)}%`;
}

/**
 * Lista ordinata delle utenze di una mappa: maniglia ⋮⋮ per il trascinamento col mouse (sulla sola
 * maniglia) e da tastiera (Spazio prende e lascia, frecce spostano, Esc annulla), con gli annunci
 * per gli screen reader in una regione `aria-live`; interruttore includi/escludi e «Usa ora».
 */
export function FlowList({ rows, nameOf, disabled, onMove, onToggle, onUseNow }: FlowListProps) {
  const { t } = useI18n();
  const helpId = useId();
  const items = useRef(new Map<number, HTMLLIElement>());
  const handles = useRef(new Map<number, HTMLButtonElement>());
  /** Utenza presa da tastiera e sua posizione di partenza (per Esc). */
  const [grabbed, setGrabbed] = useState<{ number: number; from: number } | null>(null);
  const [dragging, setDragging] = useState<number | null>(null);
  const [announce, setAnnounce] = useState('');
  const rowsRef = useRef(rows);
  rowsRef.current = rows;
  const total = rows.length;

  const indexOf = (n: number) => rowsRef.current.findIndex((r) => r.number === n);
  const say = (key: Parameters<typeof t>[0], n: number, position: number) =>
    setAnnounce(t(key, { name: nameOf(n), position: position + 1, total }));

  // Dopo uno spostamento da tastiera il focus resta sulla maniglia dell'utenza presa.
  useEffect(() => {
    if (grabbed) handles.current.get(grabbed.number)?.focus();
  }, [rows, grabbed]);

  // Trascinamento col mouse: la riga segue il puntatore, l'ordine cambia passando sulle altre righe.
  useEffect(() => {
    if (dragging === null) return;
    const onMoveEvent = (event: globalThis.MouseEvent) => {
      const from = indexOf(dragging);
      if (from < 0) return;
      let to = from;
      rowsRef.current.forEach((r, i) => {
        const el = items.current.get(r.number);
        if (!el) return;
        const box = el.getBoundingClientRect();
        const mid = box.top + box.height / 2;
        if (i < from && event.clientY < mid) to = Math.min(to, i);
        if (i > from && event.clientY > mid) to = Math.max(to, i);
      });
      if (to !== from) onMove(dragging, to);
    };
    const onUp = () => {
      const at = indexOf(dragging);
      if (at >= 0) say('flow.dropped', dragging, at);
      setDragging(null);
    };
    window.addEventListener('mousemove', onMoveEvent);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMoveEvent);
      window.removeEventListener('mouseup', onUp);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dragging, onMove]);

  function onHandleMouseDown(event: MouseEvent<HTMLButtonElement>, n: number) {
    if (disabled || event.button !== 0) return;
    event.preventDefault();
    setGrabbed(null);
    setDragging(n);
  }

  function onHandleKey(event: KeyboardEvent<HTMLButtonElement>, n: number) {
    if (disabled) return;
    const index = indexOf(n);
    if (event.key === ' ' || event.key === 'Enter') {
      event.preventDefault();
      if (grabbed?.number === n) {
        setGrabbed(null);
        say('flow.dropped', n, index);
      } else {
        setGrabbed({ number: n, from: index });
        say('flow.grabbed', n, index);
      }
    } else if ((event.key === 'ArrowUp' || event.key === 'ArrowDown') && grabbed?.number === n) {
      event.preventDefault();
      const to = event.key === 'ArrowUp' ? index - 1 : index + 1;
      if (to < 0 || to >= total) return;
      onMove(n, to);
      say('flow.moved', n, to);
    } else if (event.key === 'Escape' && grabbed?.number === n) {
      event.preventDefault();
      if (index !== grabbed.from) onMove(n, grabbed.from);
      setGrabbed(null);
      say('flow.cancelled', n, grabbed.from);
    }
  }

  if (rows.length === 0) return <p className="sd-hint">{t('flow.empty')}</p>;

  return (
    <>
      <span id={helpId} hidden>
        {t('flow.handle.help')}
      </span>
      <ol className="sd-flow" aria-label={t('flow.list')}>
        {rows.map((row, index) => {
          const name = nameOf(row.number);
          const usage = row.account ? displayUsage(row.account)?.usage : undefined;
          const blocked = row.account ? switchBlockedReason(row.account) : 'flow.missing';
          const isGrabbed = grabbed?.number === row.number || dragging === row.number;
          return (
            <li
              key={row.number}
              ref={(el) => {
                if (el) items.current.set(row.number, el);
                else items.current.delete(row.number);
              }}
              className="sd-flow__item"
              data-grabbed={isGrabbed}
              data-included={row.included}
              data-testid={`flow-row-${row.number}`}
            >
              <Tooltip text={t('flow.handle.tip')}>
                <button
                  type="button"
                  ref={(el) => {
                    if (el) handles.current.set(row.number, el);
                    else handles.current.delete(row.number);
                  }}
                  className="sd-flow__handle"
                  aria-label={t('flow.handle', { name, position: index + 1, total })}
                  aria-pressed={grabbed?.number === row.number}
                  aria-roledescription="maniglia"
                  aria-describedby={helpId}
                  disabled={disabled}
                  onMouseDown={(event) => onHandleMouseDown(event, row.number)}
                  onKeyDown={(event) => onHandleKey(event, row.number)}
                  onBlur={() => {
                    if (grabbed?.number === row.number) setGrabbed(null);
                  }}
                >
                  ⋮⋮
                </button>
              </Tooltip>
              <span className="sd-flow__main">
                <span className="sd-flow__name">
                  {name}
                  {row.account?.active && (
                    <span className="sd-badge sd-badge--ok">{t('flow.active')}</span>
                  )}
                  {!row.account && <span className="sd-badge">{t('flow.missing')}</span>}
                </span>
                <span className="sd-flow__usage">
                  5h {pct(usage?.fiveHour?.pct)} · 7d {pct(usage?.sevenDay?.pct)}
                </span>
              </span>
              <Tooltip text={t('flow.include.tip')}>
                <input
                  type="checkbox"
                  role="switch"
                  className="sd-switch"
                  aria-label={t('flow.include', { name })}
                  checked={row.included}
                  disabled={disabled}
                  onChange={(event) => onToggle(row.number, event.target.checked)}
                />
              </Tooltip>
              <Tooltip text={blocked ? t(blocked) : t('flow.useNow.tip')}>
                <button
                  type="button"
                  className="sd-btn sd-btn--small"
                  aria-label={t('flow.useNow.label', { name })}
                  disabled={disabled || blocked !== null}
                  onClick={() => onUseNow(row.number)}
                >
                  {t('flow.useNow')}
                </button>
              </Tooltip>
            </li>
          );
        })}
      </ol>
      <div className="sd-visually-hidden" aria-live="assertive">
        {announce}
      </div>
    </>
  );
}
