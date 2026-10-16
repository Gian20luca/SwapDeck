import type { SuspendReason } from '@swapdeck/types';
import { useI18n } from '@swapdeck/ui';
import { useEffect, useState } from 'react';
import type { Snapshot, TauriBackend } from './TauriBackend.js';

interface Texts {
  suspended: string;
  observer: string;
  dryRun: string;
  auto: string;
  reasons: Record<SuspendReason, string>;
}

const TEXT: Record<'it' | 'en', Texts> = {
  it: {
    suspended: 'Motore sospeso',
    observer: 'Motore attivo in un’altra istanza: qui solo osservazione',
    dryRun: 'Motore in sola osservazione',
    auto: 'cswap auto in esecuzione',
    reasons: {
      disabled: 'spento',
      invalidRules: 'regole non valide',
      conflict: 'altro motore attivo',
      noActiveAccount: 'nessun account attivo gestito',
      noViableTarget: 'nessun account utilizzabile',
    },
  },
  en: {
    suspended: 'Engine suspended',
    observer: 'Engine running in another instance: observing only',
    dryRun: 'Engine in observe-only mode',
    auto: 'cswap auto running',
    reasons: {
      disabled: 'off',
      invalidRules: 'invalid rules',
      conflict: 'another engine is active',
      noActiveAccount: 'no managed active account',
      noViableTarget: 'no usable account',
    },
  },
};

/** Righe di stato del motore e di `cswap auto`; vuoto se non c'è nulla da segnalare. */
export function engineLines(snap: Snapshot | null, locale: keyof typeof TEXT): string[] {
  if (!snap) return [];
  const t = TEXT[locale];
  const lines: string[] = [];
  const engine = snap.engine;
  if (engine && snap.engineSuspended && engine.suspended) {
    const detail = engine.detail.length > 0 ? ` (${engine.detail.join(', ')})` : '';
    lines.push(`${t.suspended}: ${t.reasons[engine.suspended]}${detail}`);
  } else if (engine && (engine.enabled || engine.dryRun) && !engine.holder) {
    lines.push(t.observer);
  } else if (engine && engine.dryRun && engine.holder) {
    lines.push(t.dryRun);
  }
  if (snap.autoRunning) lines.push(t.auto);
  return lines;
}

/** Stato del motore (sospeso, osservazione) e di `cswap auto` sopra la dashboard. */
export function EngineStatus({
  backend,
}: {
  backend: Pick<TauriBackend, 'getSnapshot' | 'onSnapshot'>;
}) {
  const { locale } = useI18n();
  const [snap, setSnap] = useState<Snapshot | null>(null);

  useEffect(() => {
    let alive = true;
    const off = backend.onSnapshot((s) => alive && setSnap(s));
    backend
      .getSnapshot()
      .then((s) => alive && setSnap((prev) => prev ?? s))
      .catch(() => undefined);
    return () => {
      alive = false;
      off();
    };
  }, [backend]);

  const lines = engineLines(snap, locale);
  if (lines.length === 0) return null;
  return (
    <div className="sd-desktop-engine" role="status" aria-live="polite">
      {lines.map((line, i) => (
        <div
          key={line}
          className="sd-notice"
          data-tone={i === 0 && snap?.engineSuspended ? 'danger' : undefined}
        >
          {line}
        </div>
      ))}
    </div>
  );
}
