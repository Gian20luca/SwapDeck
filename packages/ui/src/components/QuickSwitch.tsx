import { useI18n } from '../i18n/index.js';
import { Tooltip } from './Tooltip.js';

export type QuickStrategy = 'best' | 'next-available';

export interface QuickSwitchProps {
  onSwitch: (strategy: QuickStrategy) => void;
  disabled?: boolean;
  /** Perché il cambio rapido non si può usare (es. meno di due account): disattiva i pulsanti e
   * prende il posto del suggerimento. */
  unavailable?: string;
}

/** Cambio rapido: `best` (più margine) e `next-available` (prossimo non al limite). */
export function QuickSwitch({ onSwitch, disabled = false, unavailable }: QuickSwitchProps) {
  const { t } = useI18n();
  const off = disabled || unavailable !== undefined;
  return (
    <div className="sd-toolbar__group" role="group" aria-label={t('quick.group')}>
      <Tooltip text={unavailable ?? t('quick.best.hint')}>
        <button
          type="button"
          className="sd-btn sd-btn--small"
          disabled={off}
          onClick={() => onSwitch('best')}
        >
          {t('quick.best')}
        </button>
      </Tooltip>
      <Tooltip text={unavailable ?? t('quick.next.hint')}>
        <button
          type="button"
          className="sd-btn sd-btn--small"
          disabled={off}
          onClick={() => onSwitch('next-available')}
        >
          {t('quick.next')}
        </button>
      </Tooltip>
    </div>
  );
}
