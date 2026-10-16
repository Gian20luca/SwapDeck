/** Funzioni pure di formattazione e soglie, senza dipendenze da React né dai tipi dello schema. */

export type Level = 'ok' | 'warn' | 'danger' | 'unknown';

export interface Thresholds {
  warn: number;
  danger: number;
}

/** Soglie di default, allineate alle notifiche (80% e 95%, F2). */
export const DEFAULT_THRESHOLDS: Thresholds = { warn: 80, danger: 95 };

/** Livello di colore di una percentuale di consumo. */
export function levelFor(
  pct: number | null | undefined,
  thresholds: Thresholds = DEFAULT_THRESHOLDS,
): Level {
  if (pct === null || pct === undefined || !Number.isFinite(pct)) return 'unknown';
  if (pct >= thresholds.danger) return 'danger';
  if (pct >= thresholds.warn) return 'warn';
  return 'ok';
}

/** Percentuale limitata a 0..100 e arrotondata all'intero per la visualizzazione. */
export function displayPct(pct: number): number {
  return Math.round(Math.min(100, Math.max(0, pct)));
}

/**
 * Accorcia un'email mantenendo il dominio: `lungonomeutente@example.com` → `lungonome…@example.com`.
 * Se non c'è `@` tronca il testo.
 */
export function shortEmail(email: string, max = 24): string {
  if (email.length <= max) return email;
  const at = email.lastIndexOf('@');
  if (at <= 0) return `${email.slice(0, Math.max(1, max - 1))}…`;
  const local = email.slice(0, at);
  const domain = email.slice(at);
  const room = max - domain.length - 1;
  if (room >= 3) return `${local.slice(0, room)}…${domain}`;
  return `${local.slice(0, 3)}…${domain.slice(0, Math.max(1, max - 4))}…`;
}

/** Millisecondi mancanti → `3d 21h`, `1h 57m`, `12m`, `0m`. Valori negativi o non finiti: `0m`. */
export function formatCountdown(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return '0m';
  const totalMinutes = Math.floor(ms / 60_000);
  const days = Math.floor(totalMinutes / 1440);
  const hours = Math.floor((totalMinutes % 1440) / 60);
  const minutes = totalMinutes % 60;
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

/** Tempo al reset da un ISO 8601; `null` se assente o non valido. */
export function countdownTo(resetsAt: string | null | undefined, now: number): string | null {
  if (!resetsAt) return null;
  const target = Date.parse(resetsAt);
  if (Number.isNaN(target)) return null;
  return formatCountdown(target - now);
}

/** Età di un dato in secondi → `<1m`, `5m`, `3h`, `9d`. */
export function formatAge(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 60) return '<1m';
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)}h`;
  return `${Math.floor(seconds / 86_400)}d`;
}

/** Oltre questa età (secondi) il dato si segnala come vecchio. */
export const STALE_AGE_SECONDS = 600;

export function isStale(ageSeconds: number | null | undefined): boolean {
  return ageSeconds !== null && ageSeconds !== undefined && ageSeconds > STALE_AGE_SECONDS;
}

/** Ora locale `HH:MM:SS` (24 ore, indipendente dalla lingua) di un istante ISO 8601; `null` se manca o non è valido. */
export function formatClock(iso: string | null | undefined): string | null {
  if (!iso) return null;
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return null;
  const d = new Date(ms);
  const two = (n: number) => String(n).padStart(2, '0');
  return `${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`;
}

/** Parole che dipendono dalla lingua per [`formatWhen`]. */
export interface WhenWords {
  today: string;
  tomorrow: string;
}

/** Numero del giorno di calendario di un istante nel fuso dato (locale se assente). */
function dayNumber(ms: number, timeZone: string | undefined): number {
  const parts = new Intl.DateTimeFormat('en-US', {
    year: 'numeric',
    month: 'numeric',
    day: 'numeric',
    ...(timeZone ? { timeZone } : {}),
  }).formatToParts(ms);
  const get = (type: string) => Number(parts.find((p) => p.type === type)?.value);
  return Date.UTC(get('year'), get('month') - 1, get('day')) / 86_400_000;
}

/**
 * Data e ora esatte di un istante ISO 8601 nel fuso locale, con `Intl.DateTimeFormat`:
 * `oggi 15:12`, `domani 09:00`, altrimenti `ven 16/10 15:12` (`Fri 10/16 15:12` in inglese).
 * `null` se manca o non è valido. `timeZone` (IANA) serve ai test; di norma vale il fuso locale.
 */
export function formatWhen(
  iso: string | null | undefined,
  now: number,
  locale: string,
  words: WhenWords,
  timeZone?: string,
): string | null {
  if (!iso) return null;
  const ms = Date.parse(iso);
  if (Number.isNaN(ms)) return null;
  const tz = timeZone ? { timeZone } : {};
  const time = new Intl.DateTimeFormat(locale, {
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
    ...tz,
  }).format(ms);
  const days = dayNumber(ms, timeZone) - dayNumber(now, timeZone);
  if (days === 0) return `${words.today} ${time}`;
  if (days === 1) return `${words.tomorrow} ${time}`;
  const weekday = new Intl.DateTimeFormat(locale, { weekday: 'short', ...tz }).format(ms);
  const date = new Intl.DateTimeFormat(locale, { day: '2-digit', month: '2-digit', ...tz }).format(
    ms,
  );
  return `${weekday} ${date} ${time}`;
}
