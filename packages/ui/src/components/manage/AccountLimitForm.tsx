import type { AccountLimit } from '@swapdeck/types';
import { useState } from 'react';
import { useI18n } from '../../i18n/index.js';
import { fromLimitDraft, toLimitDraft, type LimitDraft } from '../../lib/engineRules.js';
import { Tooltip } from '../Tooltip.js';
import { TextField } from './common.js';

export interface AccountLimitFormProps {
  /** Nome dell'utenza (alias o email breve), per l'etichetta del riquadro. */
  name: string;
  initial: AccountLimit | undefined;
  busy: boolean;
  /** Salva il massimo (`null` = tolto); `true` se è andato a buon fine (il riquadro si chiude). */
  onSave: (limit: AccountLimit | null) => Promise<boolean>;
  onCancel: () => void;
}

/**
 * Riquadro «Massimo per questa utenza» (F22): soglie sulle 5 ore e sui 7 giorni proprie
 * dell'utenza, comuni a tutte le mappe; campi vuoti = vale la mappa. Niente `<form>`: sta dentro
 * il modulo delle regole.
 */
export function AccountLimitForm({ name, initial, busy, onSave, onCancel }: AccountLimitFormProps) {
  const { t } = useI18n();
  const [draft, setDraft] = useState<LimitDraft>(() => toLimitDraft(initial));
  const { limit, errors } = fromLimitDraft(draft);
  const invalid = Object.keys(errors).length > 0;

  async function save() {
    if (invalid) return;
    if (await onSave(limit)) onCancel();
  }

  return (
    <div
      className="sd-flow__limit"
      role="group"
      aria-label={t('flow.limit', { name })}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.stopPropagation();
          onCancel();
        } else if (event.key === 'Enter') {
          // Il riquadro sta dentro il modulo delle regole: Invio salva il massimo, non la mappa.
          event.preventDefault();
          event.stopPropagation();
          void save();
        }
      }}
    >
      <TextField
        label={t('rules.fiveHour')}
        tip={t('flow.limit.fiveHour.tip')}
        type="number"
        min={1}
        max={100}
        step="any"
        placeholder={t('flow.limit.placeholder')}
        value={draft.fiveHour}
        onChange={(fiveHour) => setDraft((d) => ({ ...d, fiveHour }))}
        error={errors.fiveHour ? t(errors.fiveHour) : null}
        disabled={busy}
      />
      <TextField
        label={t('rules.sevenDay')}
        tip={t('flow.limit.sevenDay.tip')}
        type="number"
        min={1}
        max={100}
        step="any"
        placeholder={t('flow.limit.placeholder')}
        value={draft.sevenDay}
        onChange={(sevenDay) => setDraft((d) => ({ ...d, sevenDay }))}
        error={errors.sevenDay ? t(errors.sevenDay) : null}
        disabled={busy}
      />
      <p className="sd-hint">{t('flow.limit.hint')}</p>
      <div className="sd-row">
        <Tooltip text={t('flow.limit.save.tip')}>
          <button
            type="button"
            className="sd-btn sd-btn--small sd-btn--primary"
            disabled={busy || invalid}
            onClick={() => void save()}
          >
            {t('common.save')}
          </button>
        </Tooltip>
        <Tooltip text={t('flow.limit.cancel.tip')}>
          <button type="button" className="sd-btn sd-btn--small" disabled={busy} onClick={onCancel}>
            {t('common.cancel')}
          </button>
        </Tooltip>
      </div>
    </div>
  );
}
