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
