import type { NotificationConfig } from '@swapdeck/types';
import type { NotificationPatch } from '../backend/types.js';

/** Default delle notifiche (specchio di `NotificationConfig::default` del Rust), solo per il FakeBackend. */
export function defaultNotifications(): NotificationConfig {
  return {
    thresholds: [80, 95],
    switchPreview: { enabled: true, pointsBefore: 5, minutesBefore: 10 },
    switched: { enabled: true },
    returnedToPrimary: { enabled: true },
    noReserve: { enabled: true },
    percent: { enabled: true, scope: 'active', fiveHour: [80, 95], sevenDay: [80, 95] },
    exhaustion: { enabled: true, scope: 'active' },
    resetSoon: { enabled: false, scope: 'active', minutesBefore: 15 },
    resetDone: { enabled: false, scope: 'active' },
    modelLimit: { enabled: true, scope: 'active', thresholds: [80, 95] },
    engineState: { enabled: true },
    login: { enabled: true, scope: 'all', daysBefore: 3 },
    stale: { enabled: true, scope: 'active', minutes: 15 },
    updates: { enabled: true },
    quietHours: { enabled: false, start: '22:00', end: '08:00', allowCritical: true },
    dailySummary: { enabled: false, time: '20:00' },
  };
}

/** Merge patch a un livello per avviso (come `patch_notifications`, senza la validazione completa). */
export function patchNotifications(
  current: NotificationConfig,
  patch: NotificationPatch,
): NotificationConfig {
  const next = structuredClone(current) as unknown as Record<string, Record<string, unknown>>;
  for (const [key, value] of Object.entries(patch)) {
    if (!(key in next) || key === 'thresholds') throw new Error(`campo sconosciuto: ${key}`);
    next[key] = { ...next[key], ...(value as Record<string, unknown>) };
  }
  const out = next as unknown as NotificationConfig;
  out.thresholds = [...out.percent.fiveHour];
  return out;
}
