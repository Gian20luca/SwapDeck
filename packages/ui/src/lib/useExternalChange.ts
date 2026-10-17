import { useEffect, useRef, useState } from 'react';
import { useBackend } from '../backend/context.js';

/**
 * Rilegge quando l'host segnala che config o stato sono cambiati fuori da questa vista
 * (altra finestra, app, CLI): `Backend.onConfigChanged`, facoltativo.
 */
export function useConfigChanged(reload: () => unknown): void {
  const backend = useBackend();
  const latest = useRef(reload);
  latest.current = reload;
  useEffect(() => backend.onConfigChanged?.(() => void latest.current()), [backend]);
}

/**
 * Form su dati salvati che possono cambiare fuori. Quando `saved` cambia: se il form era fermo
 * (uguale al valore di prima o già uguale al nuovo) si riallinea con `reset`; altrimenti resta com'è
 * e `stale` diventa vero, finché l'utente non sceglie `reloadNow` o il form torna uguale al salvato.
 * `matches(s)` dice se il form mostra esattamente `s`; `own(s)` segna un salvataggio fatto da qui,
 * che quindi non conta come cambio esterno.
 */
export function useSyncedForm<T>(
  saved: T,
  matches: (saved: T) => boolean,
  reset: (saved: T) => void,
): { stale: boolean; reloadNow: () => void; own: (saved: T) => void } {
  const [stale, setStale] = useState(false);
  const prev = useRef({ json: JSON.stringify(saved), value: saved });
  const latest = useRef({ saved, matches, reset });
  latest.current = { saved, matches, reset };
  const json = JSON.stringify(saved);
  const clean = matches(saved);

  useEffect(() => {
    const before = prev.current;
    if (json === before.json) return;
    prev.current = { json, value: latest.current.saved };
    const { saved: now, matches: same, reset: apply } = latest.current;
    if (same(before.value) || same(now)) {
      apply(now);
      setStale(false);
    } else setStale(true);
  }, [json]);

  useEffect(() => {
    if (stale && clean) setStale(false);
  }, [stale, clean]);

  return {
    stale,
    reloadNow: () => {
      latest.current.reset(latest.current.saved);
      setStale(false);
    },
    own: (value: T) => {
      prev.current = { json: JSON.stringify(value), value };
    },
  };
}
