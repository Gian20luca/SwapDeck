import type { EngineProfiles, ProfileCommand, RulesProfile } from '@swapdeck/types';
import { useId, useState } from 'react';
import { useI18n, type MessageKey } from '../../i18n/index.js';
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

type Translate = (key: MessageKey, values?: Record<string, string | number>) => string;

const MAX_NAME = 60;
const PRESET_NAMES: Record<string, MessageKey> = {
  priorityReserves: 'maps.presetName.priorityReserves',
  eco: 'maps.presetName.eco',
  sport: 'maps.presetName.sport',
  safe: 'maps.presetName.safe',
};

/** Nome da mostrare: un preset noto nella lingua dell'interfaccia, il resto com'è salvato. */
export function profileLabel(p: RulesProfile, t: Translate): string {
  const key = p.preset ? PRESET_NAMES[p.preset] : undefined;
  return key ? t(key) : p.name;
}

/** Elenco delle mappe diviso in personali e preset, ciascun gruppo nell'ordine salvato. */
export function groupProfiles(profiles: EngineProfiles) {
  return {
    personal: profiles.profiles.filter((p) => !p.preset),
    presets: profiles.profiles.filter((p) => p.preset),
  };
}

/** Opzioni di un `<select>` delle mappe: gruppi «Mappe personali» e «Preset». */
export function MapOptions({ profiles }: { profiles: EngineProfiles }) {
  const { t } = useI18n();
  const { personal, presets } = groupProfiles(profiles);
  const option = (p: RulesProfile) => (
    <option key={p.id} value={p.id}>
      {profileLabel(p, t)}
    </option>
  );
  return (
    <>
      {personal.length > 0 && (
        <optgroup label={t('maps.group.personal')}>{personal.map(option)}</optgroup>
      )}
      {presets.length > 0 && (
        <optgroup label={t('maps.group.presets')}>{presets.map(option)}</optgroup>
      )}
    </>
  );
}

export interface MapNameFormProps {
  initial: string;
  busy: boolean;
  /** Applica il nome (già senza spazi ai lati); `true` se è riuscito e il campo si chiude. */
  onSubmit: (name: string) => Promise<boolean>;
  onCancel: () => void;
}

/** Campo del nome di una mappa (non vuoto, fino a 60 caratteri) con Conferma e Annulla. */
export function MapNameForm({ initial, busy, onSubmit, onCancel }: MapNameFormProps) {
  const { t } = useI18n();
  const nameId = useId();
  const [name, setName] = useState(initial);
  const [error, setError] = useState(false);

  async function submit() {
    const value = name.trim();
    if (value === '' || value.length > MAX_NAME) {
      setError(true);
      return;
    }
    if (await onSubmit(value)) onCancel();
  }

  return (
    // Niente <form>: il campo può stare dentro il modulo delle regole.
    <div className="sd-row sd-maps__name">
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
          aria-invalid={error}
          disabled={busy}
          autoFocus
          onChange={(event) => {
            setName(event.target.value);
            setError(false);
          }}
          onKeyDown={(event) => {
            if (event.key === 'Escape') onCancel();
            if (event.key === 'Enter') {
              event.preventDefault();
              void submit();
            }
          }}
        />
      </Tooltip>
      <Tooltip text={t('maps.name.confirm.tip')}>
        <button
          type="button"
          className="sd-btn sd-btn--small sd-btn--primary"
          disabled={busy}
          onClick={() => void submit()}
        >
          {t('maps.name.confirm')}
        </button>
      </Tooltip>
      <Tooltip text={t('maps.name.cancel.tip')}>
        <button type="button" className="sd-btn sd-btn--small" disabled={busy} onClick={onCancel}>
          {t('maps.name.cancel')}
        </button>
      </Tooltip>
      {error && (
        <span className="sd-field-error" role="alert">
          {t('maps.err.name')}
        </span>
      )}
    </div>
  );
}

type Editing = { kind: 'create' } | { kind: 'rename' | 'duplicate'; id: string };
type Pending = { kind: 'delete'; id: string; name: string } | { kind: 'select'; id: string };

/**
 * Selettore della mappa attiva (personali e preset in gruppi distinti), «Nuova» e l'elenco
 * delle mappe personali con rinomina, duplica ed elimina (non quella attiva). I preset sono in
 * sola lettura: si clonano dal riquadro delle regole.
 */
export function MapSelector({ profiles, busy, dirty, onCommand }: MapSelectorProps) {
  const { t } = useI18n();
  const selectId = useId();
  const listId = useId();
  const [editing, setEditing] = useState<Editing | null>(null);
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);
  const active = profiles.profiles.find((p) => p.id === profiles.activeId) ?? profiles.profiles[0];
  if (!active) return null;
  const { personal } = groupProfiles(profiles);

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
    else await onCommand({ op: 'delete', id: p.id });
  }

  function nameForm(target: Editing) {
    const source = 'id' in target ? profiles.profiles.find((p) => p.id === target.id) : null;
    const initial =
      target.kind === 'rename'
        ? (source?.name ?? '')
        : target.kind === 'duplicate' && source
          ? t('maps.copyName', { name: source.name }).slice(0, MAX_NAME)
          : '';
    return (
      <MapNameForm
        key={`${target.kind}-${'id' in target ? target.id : ''}`}
        initial={initial}
        busy={busy}
        onCancel={() => setEditing(null)}
        onSubmit={(name) =>
          onCommand(
            target.kind === 'create'
              ? { op: 'create', name }
              : target.kind === 'rename'
                ? { op: 'rename', id: target.id, name }
                : { op: 'duplicate', id: target.id, name },
          )
        }
      />
    );
  }

  const dialog =
    pending?.kind === 'delete'
      ? {
          title: t('maps.confirmDelete', { name: pending.name }),
          message: t('maps.confirmDelete.message'),
          tone: 'danger' as const,
          label: t('maps.delete'),
        }
      : {
          title: t('maps.confirmSwitch'),
          message: t('maps.confirmSwitch.message'),
          tone: 'default' as const,
          label: t('maps.confirmSwitch.cta'),
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
            <MapOptions profiles={profiles} />
          </select>
        </Tooltip>
      </div>
      <div className="sd-row sd-maps__actions">
        <Tooltip text={t('maps.new.tip')}>
          <button
            type="button"
            className="sd-btn sd-btn--small"
            disabled={busy}
            onClick={() => setEditing({ kind: 'create' })}
          >
            {t('maps.new')}
          </button>
        </Tooltip>
        <Tooltip text={t('maps.manage.tip')}>
          <button
            type="button"
            className="sd-btn sd-btn--small"
            aria-expanded={open}
            aria-controls={listId}
            onClick={() => setOpen((o) => !o)}
          >
            {`${t('maps.manage')} (${personal.length}) ${open ? '▴' : '▾'}`}
          </button>
        </Tooltip>
      </div>
      {editing?.kind === 'create' && nameForm(editing)}
      {open && (
        <div id={listId}>
          {personal.length === 0 ? (
            <p className="sd-hint">{t('maps.none')}</p>
          ) : (
            <ul className="sd-maps__list" aria-label={t('maps.group.personal')}>
              {personal.map((p) => {
                const isActive = p.id === active.id;
                return (
                  <li key={p.id} className="sd-maps__item">
                    <div className="sd-maps__row">
                      <span className="sd-maps__label">
                        {p.name}
                        {isActive && <span className="sd-hint"> · {t('maps.activeTag')}</span>}
                      </span>
                      <span className="sd-row sd-maps__actions">
                        <Tooltip text={t('maps.rename.tip')}>
                          <button
                            type="button"
                            className="sd-btn sd-btn--small"
                            aria-label={t('maps.rename.label', { name: p.name })}
                            disabled={busy}
                            onClick={() => setEditing({ kind: 'rename', id: p.id })}
                          >
                            {t('maps.rename')}
                          </button>
                        </Tooltip>
                        <Tooltip text={t('maps.duplicate.tip')}>
                          <button
                            type="button"
                            className="sd-btn sd-btn--small"
                            aria-label={t('maps.duplicate.label', { name: p.name })}
                            disabled={busy}
                            onClick={() => setEditing({ kind: 'duplicate', id: p.id })}
                          >
                            {t('maps.duplicate')}
                          </button>
                        </Tooltip>
                        <Tooltip text={isActive ? t('maps.delete.active') : t('maps.delete.tip')}>
                          <button
                            type="button"
                            className="sd-btn sd-btn--small"
                            aria-label={t('maps.delete.label', { name: p.name })}
                            disabled={busy || isActive}
                            onClick={() => setPending({ kind: 'delete', id: p.id, name: p.name })}
                          >
                            {t('maps.delete')}
                          </button>
                        </Tooltip>
                      </span>
                    </div>
                    {editing && 'id' in editing && editing.id === p.id && nameForm(editing)}
                  </li>
                );
              })}
            </ul>
          )}
        </div>
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
