import type { ListResponse } from '@swapdeck/types';
import list from '../../../../fixtures/cswap/list.json';
import listDisabled from '../../../../fixtures/cswap/list-disabled.json';
import listNoAccounts from '../../../../fixtures/cswap/list-no-accounts.json';
import listRateLimited from '../../../../fixtures/cswap/list-rate-limited.json';
import listStale from '../../../../fixtures/cswap/list-usage-stale.json';
import listTokenExpired from '../../../../fixtures/cswap/list-token-expired.json';

/**
 * Scenari del FakeBackend: le fixture di `fixtures/cswap/` (anonimizzate), importate senza copiarle.
 * I JSON non portano i tipi stretti (`usageStatus` è `string`): il cast passa da `unknown`.
 */
export const SCENARIOS = {
  default: list,
  disabled: listDisabled,
  stale: listStale,
  rateLimited: listRateLimited,
  tokenExpired: listTokenExpired,
  noAccounts: listNoAccounts,
} as const;

export type ScenarioName = keyof typeof SCENARIOS;

export const SCENARIO_NAMES = Object.keys(SCENARIOS) as ScenarioName[];

/** Istante in cui sono state catturate le fixture (data di `list.json` più la sua età). */
export const FIXTURE_REFERENCE_MS = Date.parse('2026-10-15T19:32:50Z');

const TIME_KEYS = new Set([
  'resetsAt',
  'usageFetchedAt',
  'lastGoodFetchedAt',
  'projectedExhaustionAt',
  'usageRetryAt',
  'loginExpiresAt',
]);

function shiftTimes(value: unknown, delta: number): unknown {
  if (Array.isArray(value)) return value.map((v) => shiftTimes(v, delta));
  if (value && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value).map(([key, v]) => [
        key,
        TIME_KEYS.has(key) && typeof v === 'string' && !Number.isNaN(Date.parse(v))
          ? new Date(Date.parse(v) + delta).toISOString()
          : shiftTimes(v, delta),
      ]),
    );
  }
  return value;
}

/** Copia della fixture con tutti gli istanti spostati in modo che «ora» coincida con la cattura. */
export function loadScenario(name: ScenarioName, now: number): ListResponse {
  return shiftTimes(SCENARIOS[name], now - FIXTURE_REFERENCE_MS) as unknown as ListResponse;
}
