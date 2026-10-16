import type { EngineProfiles, ProfileCommand } from '@swapdeck/types';
import { useId, useState } from 'react';
import { useI18n } from '../../i18n/index.js';
import { ConfirmDialog } from '../ConfirmDialog.js';
import { Tooltip } from '../Tooltip.js';

export interface MapSelectorProps {
  profiles: EngineProfiles;
  busy: boolean;
  /** Modifiche non salvate alla mappa attiva: cambiarla chiede conferma. */
  dirty: boolean;
  /** Esegue l'operazione; `true` se è riuscita (l'esito lo mostra chi chiama). */
  onCommand: (command: ProfileCommand) => Promise<boolean>;
}

type NameMode = 'create' | 'rename' | 'duplicate';
type Pending = { kind: 'delete' | 'restore' } | { kind: 'select'; id: string };

const MAX_NAME = 60;

/** Selettore della mappa attiva con crea, rinomina, duplica, elimina (non l'ultima) e ripristina. */
export function MapSelector({ profiles, busy, dirty, onCommand }: MapSelectorProps) {
  const { t } = useI18n();
  const selectId = useId();
  const nameId = useId();
  const [mode, setMode] = useState<NameMode | null>(null);
  const [name, setName] = useState('');
  const [nameError, setNameError] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);
  const active = profiles.profiles.find((p) => p.id === profiles.activeId) ?? profiles.profiles[0];
  if (!active) return null;
  const last = profiles.profiles.length <= 1;

  function openName(next: NameMode) {
    setMode(next);
    setNameError(false);
    setName(next === 'rename' ? active!.name : '');
  }

  async function submitName() {
    const value = name.trim();
    if (!mode) return;
    if ((mode !== 'duplicate' || value !== '') && (value === '' || value.length > MAX_NAME)) {
      setNameError(true);
      return;
    }
    const command: ProfileCommand =
      mode === 'create'
        ? { op: 'create', name: value }
        : mode === 'rename'
          ? { op: 'rename', id: active!.id, name: value }
          : value === ''
            ? { op: 'duplicate', id: active!.id }
            : { op: 'duplicate', id: active!.id, name: value };
    if (await onCommand(command)) setMode(null);
  }

  function select(id: string) {
    if (id === active!.id) return;
    if (dirty) setPending({ kind: 'select', id });
    else void onCommand({ op: 'select', id });
  }

  async function confirm() {
    const p = pending;
    setPending(null);
    if (!p) return;
    if (p.kind === 'select') await onCommand({ op: 'select', id: p.id });
    else if (p.kind === 'delete') await onCommand({ op: 'delete', id: active!.id });
    else await onCommand({ op: 'restore', id: active!.id });
  }

  const dialog =
    pending?.kind === 'delete'
      ? {
          title: t('maps.confirmDelete', { name: active.name }),
          message: t('maps.confirmDelete.message'),
          tone: 'danger' as const,
          label: t('maps.delete'),
        }
      : pending?.kind === 'restore'
        ? {
            title: t('maps.confirmRestore', { name: active.name }),
            message: t('maps.confirmRestore.message'),
            tone: 'danger' as const,
            label: t('maps.restore'),
          }
        : {
            title: t('maps.confirmSwitch'),
            message: t('maps.confirmSwitch.message'),
            tone: 'default' as const,
            label: t('maps.select'),
          };

  return (
    <section className="sd-section sd-maps">
      <div className="sd-form__field">
        <label htmlFor={selectId}>{t('maps.title')}</label>
        <Tooltip text={t('maps.select.tip')} block>
          <select
            id={selectId}
            className="sd-input"
            value={active.id}
            disabled={busy}
            onChange={(event) => select(event.target.value)}
          >
            {profiles.profiles.map((p) => (
              <option key={p.id} value={p.id}>
                {p.preset ? `${p.name} (${t('maps.preset')})` : p.name}
              </option>
            ))}
          </select>
        </Tooltip>
      </div>
      <div className="sd-row sd-maps__actions">
        <Tooltip text={t('maps.new.tip')}>
          <button
            type="button"
            className="sd-btn sd-btn--small"
            disabled={busy}
            onClick={() => openName('create')}
          >
            {t('maps.new')}
          </button>
        </Tooltip>
        <Tooltip text={t('maps.rename.tip')}>
          <button
            type="button"
            className="sd-btn sd-btn--small"
            disabled={busy}
            onClick={() => openName('rename')}
          >
            {t('maps.rename')}
          </button>
        </Tooltip>
        <Tooltip text={t('maps.duplicate.tip')}>
          <button
            type="button"
            className="sd-btn sd-btn--small"
            disabled={busy}
            onClick={() => openName('duplicate')}
          >
            {t('maps.duplicate')}
          </button>
        </Tooltip>
        <Tooltip text={last ? t('maps.delete.last') : t('maps.delete.tip')}>
          <button
            type="button"
            className="sd-btn sd-btn--small"
            disabled={busy || last}
            onClick={() => setPending({ kind: 'delete' })}
          >
            {t('maps.delete')}
          </button>
        </Tooltip>
        {active.preset && (
          <Tooltip text={t('maps.restore.tip')}>
            <button
              type="button"
              className="sd-btn sd-btn--small"
              disabled={busy}
              onClick={() => setPending({ kind: 'restore' })}
            >
              {t('maps.restore')}
            </button>
          </Tooltip>
        )}
      </div>
      {mode && (
        <form
          className="sd-row sd-maps__name"
          onSubmit={(event) => {
            event.preventDefault();
            void submitName();
          }}
        >
          <label htmlFor={nameId} className="sd-visually-hidden">
            {t('maps.name')}
          </label>
          <Tooltip text={t('maps.name.tip')}>
            <input
              id={nameId}
              className="sd-input"
              value={name}
              maxLength={MAX_NAME}
              placeholder={t('maps.name')}
              aria-invalid={nameError}
              disabled={busy}
              autoFocus
              onChange={(event) => {
                setName(event.target.value);
                setNameError(false);
              }}
              onKeyDown={(event) => {
                if (event.key === 'Escape') setMode(null);
              }}
            />
          </Tooltip>
          <Tooltip text={t('maps.name.confirm.tip')}>
            <button type="submit" className="sd-btn sd-btn--small sd-btn--primary" disabled={busy}>
              {t('maps.name.confirm')}
            </button>
          </Tooltip>
          <Tooltip text={t('maps.name.cancel.tip')}>
            <button
              type="button"
              className="sd-btn sd-btn--small"
              disabled={busy}
              onClick={() => setMode(null)}
            >
              {t('maps.name.cancel')}
            </button>
          </Tooltip>
          {nameError && (
            <span className="sd-field-error" role="alert">
              {t('maps.err.name')}
            </span>
          )}
        </form>
      )}
      <ConfirmDialog
        open={pending !== null}
        title={dialog.title}
        message={dialog.message}
        tone={dialog.tone}
        confirmLabel={dialog.label}
        onConfirm={() => void confirm()}
        onCancel={() => setPending(null)}
      />
    </section>
  );
}
