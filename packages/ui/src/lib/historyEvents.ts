import type { DecisionRecord, SwitchRecord } from '@swapdeck/types';
import type { HistoryEvents } from '../backend/types.js';

export type EventRow =
  | { kind: 'switch'; ts: number; record: SwitchRecord }
  | { kind: 'decision'; ts: number; record: DecisionRecord };

/** Cambi e decisioni in un solo elenco, dal più recente. */
export function mergeEvents(events: HistoryEvents): EventRow[] {
  const rows: EventRow[] = [
    ...events.switches.map((record) => ({ kind: 'switch' as const, ts: record.ts, record })),
    ...events.decisions.map((record) => ({ kind: 'decision' as const, ts: record.ts, record })),
  ];
  return rows.sort((a, b) => b.ts - a.ts);
}
