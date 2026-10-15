import { useEffect, useState } from 'react';

/** Istante corrente (ms) aggiornato a intervalli: serve ai countdown, che si ricalcolano da `resetsAt`. */
export function useNow(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}
