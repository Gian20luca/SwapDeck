import type {
  Account,
  Candidate,
  EngineProfiles,
  EngineRules,
  PresetKind,
  ProfileCommand,
} from '@swapdeck/types';
import { configuredOrder } from '../lib/engineRules.js';

/** Specchio semplificato delle mappe del Rust (`engine/profiles.rs`), solo per il FakeBackend. */
const PRESETS: readonly { id: string; name: string; kind: PresetKind }[] = [
  { id: 'cswap-guard', name: 'Come cswap-guard', kind: 'cswapGuard' },
  { id: 'eco', name: 'Eco', kind: 'eco' },
  { id: 'sport', name: 'Sport', kind: 'sport' },
  { id: 'safe', name: 'Sicura', kind: 'safe' },
];

function presetRules(kind: PresetKind, base: EngineRules): EngineRules {
  const r: EngineRules = {
    ...base,
    order: configuredOrder(base),
    fiveHourThreshold: 95,
    sevenDayThreshold: 98,
    reserveCriterion: 'lowest-weekly',
    returnHysteresis: 10,
  };
  if (kind === 'cswapGuard') Object.assign(r, { order: [6, 1, 3], excluded: [], returnHysteresis: 45 });
  if (kind === 'eco') Object.assign(r, { reserveCriterion: 'reset-soonest', fiveHourThreshold: 90 });
  if (kind === 'sport') Object.assign(r, { reserveCriterion: 'lowest5h', fiveHourThreshold: 98 });
  if (kind === 'safe') r.reserveCriterion = 'pace-safe';
  return r;
}

function fail(message: string): never {
  throw new Error(message);
}

export function seedProfiles(rules: EngineRules): EngineProfiles {
  return {
    activeId: 'personal',
    profiles: [
      { id: 'personal', name: 'Personale', rules: structuredClone(rules) },
      ...PRESETS.map((p) => ({ id: p.id, name: p.name, preset: p.kind, rules: presetRules(p.kind, rules) })),
    ],
  };
}

/** Applica un'operazione; restituisce le nuove mappe (le regole attive sono quelle della mappa attiva). */
export function applyProfile(
  current: EngineProfiles,
  cmd: ProfileCommand,
  activeRules: EngineRules,
): EngineProfiles {
  const next = structuredClone(current);
  const active = next.profiles.find((p) => p.id === next.activeId);
  if (active) active.rules = structuredClone(activeRules);
  const index = (id: string) => {
    const i = next.profiles.findIndex((p) => p.id === id);
    return i >= 0 ? i : fail(`mappa «${id}» inesistente`);
  };
  const freeName = (name: string, except?: number) => {
    const n = name.trim();
    if (!n || n.length > 60) fail('nome della mappa vuoto o troppo lungo');
    if (next.profiles.some((p, i) => i !== except && p.name.toLowerCase() === n.toLowerCase()))
      fail(`esiste già una mappa «${n}»`);
    return n;
  };
  const newId = () => {
    for (let n = 1; ; n++) if (!next.profiles.some((p) => p.id === `map-${n}`)) return `map-${n}`;
  };
  switch (cmd.op) {
    case 'create':
      next.profiles.push({ id: newId(), name: freeName(cmd.name), rules: structuredClone(activeRules) });
      break;
    case 'rename': {
      const i = index(cmd.id);
      next.profiles[i]!.name = freeName(cmd.name, i);
      break;
    }
    case 'duplicate': {
      const src = next.profiles[index(cmd.id)]!;
      const name = cmd.name ? freeName(cmd.name) : freeName(`${src.name} (copia)`);
      next.profiles.push({ ...structuredClone(src), id: newId(), name });
      break;
    }
    case 'delete': {
      const i = index(cmd.id);
      if (next.profiles.length === 1) fail("l'ultima mappa non si può eliminare");
      next.profiles.splice(i, 1);
      if (next.activeId === cmd.id) next.activeId = next.profiles[0]!.id;
      break;
    }
    case 'restore': {
      const p = next.profiles[index(cmd.id)]!;
      if (!p.preset) fail('la mappa non viene da un preset');
      p.rules = presetRules(p.preset, p.rules);
      break;
    }
    case 'select':
      index(cmd.id);
      next.activeId = cmd.id;
      break;
  }
  return next;
}

/** Candidato finto: la prima utenza inclusa, diversa dall'attiva e senza limiti. */
export function fakeCandidate(rules: EngineRules, accounts: readonly Account[]): Candidate | undefined {
  const order = configuredOrder(rules).filter((n) => !rules.excluded.includes(n));
  const free = order
    .map((n) => accounts.find((a) => a.number === n))
    .find(
      (a) =>
        a !== undefined &&
        !a.active &&
        a.disabled !== true &&
        a.usageStatus === 'ok' &&
        (a.usage?.fiveHour?.pct ?? 100) < rules.fiveHourThreshold,
    );
  if (!free) return undefined;
  return {
    number: free.number,
    criterion: rules.reserveCriterion,
    fallback: false,
    why: 'prima utenza libera della lista',
  };
}
