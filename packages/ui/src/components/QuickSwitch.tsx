import { useI18n } from '../i18n/index.js';

export type QuickStrategy = 'best' | 'next-available';

export interface QuickSwitchProps {
  onSwitch: (strategy: QuickStrategy) => void;
  disabled?: boolean;
}

/** Cambio rapido: `best` (più margine) e `next-available` (prossimo non al limite). */
export function QuickSwitch({ onSwitch, disabled = false }: QuickSwitchProps) {
  const { t } = useI18n();
  return (
    <div className="sd-toolbar__group" role="group" aria-label={t('quick.group')}>
      <button
        type="button"
        className="sd-btn sd-btn--small"
        disabled={disabled}
        title={t('quick.best.hint')}
        onClick={() => onSwitch('best')}
      >
        {t('quick.best')}
      </button>
      <button
        type="button"
        className="sd-btn sd-btn--small"
        disabled={disabled}
        title={t('quick.next.hint')}
        onClick={() => onSwitch('next-available')}
      >
        {t('quick.next')}
      </button>
    </div>
  );
}
