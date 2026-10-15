import type { Account, ListResponse, Usage } from '@swapdeck/types';
import type { HistoryPoint } from '../backend/types.js';

const FIVE_HOURS = 5 * 3600;
const SEVEN_DAYS = 7 * 86400;
const STEP = 30 * 60;

/** Generatore pseudo-casuale deterministico (mulberry32): stesso storico a ogni avvio. */
export function prng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Curva crescente (mai decrescente) con leggere ondulazioni; g(0)=0, g(1)=1. */
function shape(f: number): number {
  const g = (x: number) => x + 0.05 * Math.sin(4 * Math.PI * x);
  return g(f) / g(1);
}

interface WindowModel {
  /** Fine della finestra corrente, secondi Unix. */
  reset: number;
  length: number;
  /** Consumo finale della finestra corrente, 0–100. */
  currentPct: number;
}

function windowValue(
  t: number,
  w: WindowModel,
  now: number,
  rand: () => number,
  cache: Map<number, number>,
): { pct: number; expected: number } {
  const start0 = w.reset - w.length;
  const k = Math.floor((t - start0) / w.length);
  const start = start0 + k * w.length;
  const frac = Math.min(1, Math.max(0, (t - start) / w.length));
  if (k === 0) {
    // Finestra corrente: arriva esattamente al valore attuale all'istante `now`.
    const fracNow = Math.min(1, Math.max(0, (now - start) / w.length));
    const denom = fracNow > 0 ? shape(fracNow) : 1;
    const pct = fracNow > 0 ? w.currentPct * (shape(frac) / denom) : 0;
    return { pct: Math.min(100, pct), expected: frac * 100 };
  }
  let final = cache.get(k);
  if (final === undefined) {
    final = rand() < 0.2 ? rand() * 15 : 25 + rand() * 70;
    cache.set(k, final);
  }
  return { pct: Math.min(100, final * shape(frac)), expected: frac * 100 };
}

function usageOf(account: Account): { usage: Usage; until: number } | null {
  if (account.usage) {
    const at = account.usageFetchedAt ? Date.parse(account.usageFetchedAt) / 1000 : undefined;
    return { usage: account.usage, until: at ?? Date.now() / 1000 };
  }
  if (account.lastGoodUsage) {
    const at = account.lastGoodFetchedAt ? Date.parse(account.lastGoodFetchedAt) / 1000 : undefined;
    return { usage: account.lastGoodUsage, until: at ?? Date.now() / 1000 };
  }
  return null;
}

/**
 * Storico sintetico di 7 giorni a passi di 30 minuti per ogni account, coerente con la lista:
 * l'ultimo punto coincide col consumo attuale e le finestre si azzerano ai reset.
 * Gli account senza consumi noti non hanno punti; quelli con dato vecchio si fermano all'ultima misura.
 */
export function generateHistory(
  list: ListResponse,
  nowMs: number,
  sinceSeconds?: number,
): HistoryPoint[] {
  const now = Math.floor(nowMs / 1000);
  const since = sinceSeconds ?? now - SEVEN_DAYS;
  const points: HistoryPoint[] = [];
  for (const account of list.accounts) {
    const found = usageOf(account);
    if (!found) continue;
    const { usage, until } = found;
    const rand = prng(account.number * 7919 + 13);
    const five = usage.fiveHour;
    const seven = usage.sevenDay;
    const fiveModel: WindowModel = {
      reset: five?.resetsAt
        ? Date.parse(five.resetsAt) / 1000
        : until + (((account.number * 53) % 300) + 30) * 60,
      length: FIVE_HOURS,
      currentPct: five?.pct ?? 0,
    };
    const sevenModel: WindowModel | null = seven?.resetsAt
      ? { reset: Date.parse(seven.resetsAt) / 1000, length: SEVEN_DAYS, currentPct: seven.pct }
      : null;
    const fiveCache = new Map<number, number>();
    const sevenCache = new Map<number, number>();
    const end = Math.min(until, now);
    const first = Math.ceil(Math.max(since, end - SEVEN_DAYS) / STEP) * STEP;
    const timestamps: number[] = [];
    for (let t = first; t < end; t += STEP) timestamps.push(t);
    timestamps.push(Math.floor(end));
    for (const t of timestamps) {
      const f = windowValue(t, fiveModel, until, rand, fiveCache);
      const s = sevenModel ? windowValue(t, sevenModel, until, rand, sevenCache) : null;
      points.push({
        ts: t,
        account: account.number,
        fiveHourPct: round1(f.pct),
        sevenDayPct: s ? round1(s.pct) : null,
        fiveHourExpectedPct: round1(f.expected),
        sevenDayExpectedPct: s ? round1(s.expected) : null,
      });
    }
  }
  return points.sort((a, b) => a.ts - b.ts || a.account - b.account);
}

function round1(v: number): number {
  return Math.round(v * 10) / 10;
}
