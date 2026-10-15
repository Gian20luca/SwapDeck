import { useI18n } from '@swapdeck/ui';
import { useEffect, useId, useState } from 'react';
import type { AppSettings, TauriBackend } from './TauriBackend.js';

const LABELS = {
  it: { label: 'Avvio al login', dev: 'build di sviluppo: il sistema non viene modificato' },
  en: { label: 'Start at login', dev: 'development build: the system is not changed' },
} as const;

/** Casella dell'avvio automatico al login (F6), attivo di default. */
export function AutostartToggle({
  backend,
}: {
  backend: Pick<TauriBackend, 'getAppSettings' | 'setAutostart'>;
}) {
  const { locale } = useI18n();
  const text = LABELS[locale];
  const id = useId();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    backend
      .getAppSettings()
      .then((s) => alive && setSettings(s))
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [backend]);

  if (!settings) return null;
  return (
    <span className="sd-field" title={settings.autostartManaged ? undefined : text.dev}>
      <input
        id={id}
        type="checkbox"
        checked={settings.autostart}
        disabled={busy}
        onChange={(event) => {
          const enabled = event.target.checked;
          setBusy(true);
          backend
            .setAutostart(enabled)
            .then(setSettings)
            .catch(() => undefined)
            .finally(() => setBusy(false));
        }}
      />
      <label htmlFor={id}>{text.label}</label>
    </span>
  );
}
