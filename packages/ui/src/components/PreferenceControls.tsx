import { useId } from 'react';
import { LOCALES, useI18n, type LocalePreference } from '../i18n/index.js';
import { THEME_MODES, useTheme, type ThemeMode } from '../theme/index.js';
import { Tooltip } from './Tooltip.js';

const LOCALE_OPTIONS: readonly LocalePreference[] = ['auto', ...LOCALES];

/** Selettore della lingua (automatica, italiano, inglese). */
export function LanguageSelect({
  onChange,
}: { onChange?: (value: LocalePreference) => void } = {}) {
  const { t, preference, setPreference } = useI18n();
  const id = useId();
  return (
    <span className="sd-field">
      <label htmlFor={id}>{t('settings.language')}</label>
      <Tooltip text={t('settings.language.tip')}>
        <select
          id={id}
          className="sd-input sd-input--inline"
          value={preference}
          onChange={(event) => {
            const value = event.target.value as LocalePreference;
            setPreference(value);
            onChange?.(value);
          }}
        >
          {LOCALE_OPTIONS.map((value) => (
            <option key={value} value={value}>
              {t(`settings.language.${value}`)}
            </option>
          ))}
        </select>
      </Tooltip>
    </span>
  );
}

/** Selettore del tema (automatico, chiaro, scuro). */
export function ThemeSelect({ onChange }: { onChange?: (value: ThemeMode) => void } = {}) {
  const { t } = useI18n();
  const { mode, setMode } = useTheme();
  const id = useId();
  return (
    <span className="sd-field">
      <label htmlFor={id}>{t('settings.theme')}</label>
      <Tooltip text={t('settings.theme.tip')}>
        <select
          id={id}
          className="sd-input sd-input--inline"
          value={mode}
          onChange={(event) => {
            const value = event.target.value as ThemeMode;
            setMode(value);
            onChange?.(value);
          }}
        >
          {THEME_MODES.map((value) => (
            <option key={value} value={value}>
              {t(`settings.theme.${value}`)}
            </option>
          ))}
        </select>
      </Tooltip>
    </span>
  );
}
