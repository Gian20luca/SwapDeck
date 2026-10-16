import type { EngineProfiles } from '@swapdeck/types';
import { useEffect, useId, useState, type MouseEvent } from 'react';
import { useBackend } from '../backend/context.js';
import { errorMessage } from '../backend/useAccounts.js';
import { useI18n } from '../i18n/index.js';
import { Tooltip } from './Tooltip.js';

/**
 * Selettore della mappa attiva nel mini-widget: cambia mappa subito (come il menu dell'icona).
 * Non c'è niente da perdere: il mini-widget non modifica le regole. Senza i comandi delle mappe
 * (host che non li offre) non mostra nulla.
 */
export function MiniMapSelect() {
  const { t } = useI18n();
  const backend = useBackend();
  const id = useId();
  const [profiles, setProfiles] = useState<EngineProfiles | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const supported =
    typeof backend.getEngineProfiles === 'function' &&
    typeof backend.applyEngineProfile === 'function';

  useEffect(() => {
    if (!supported) return;
    let alive = true;
    const load = () =>
      backend.getEngineProfiles?.().then(
        (p) => alive && setProfiles(p),
        () => undefined,
      );
    void load();
    const off = backend.onStateChanged(() => void load());
    return () => {
      alive = false;
      off();
    };
  }, [backend, supported]);

  if (!supported || !profiles || profiles.profiles.length < 2) return null;

  async function select(next: string) {
    if (!backend.applyEngineProfile || !backend.getEngineProfiles) return;
    setBusy(true);
    setError(null);
    try {
      await backend.applyEngineProfile({ op: 'select', id: next });
      setProfiles(await backend.getEngineProfiles());
    } catch (e) {
      setError(t('common.actionFailed', { message: errorMessage(e) }));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="sd-mini__map" onClick={(event: MouseEvent) => event.stopPropagation()}>
      <label htmlFor={id}>{t('maps.select')}</label>
      <Tooltip text={t('maps.select.tip')} block>
        <select
          id={id}
          className="sd-input sd-input--inline"
          value={profiles.activeId}
          disabled={busy}
          onChange={(event) => void select(event.target.value)}
        >
          {profiles.profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      </Tooltip>
      {error && (
        <span className="sd-field-error" role="alert">
          {error}
        </span>
      )}
    </div>
  );
}
