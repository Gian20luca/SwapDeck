import { useId } from 'react';
import { LOCALES, useI18n, type LocalePreference } from '../i18n/index.js';
import { THEME_MODES, useTheme, type ThemeMode } from '../theme/index.js';

const LOCALE_OPTIONS: readonly LocalePreference[] = ['auto', ...LOCALES];

/** Selettore della lingua (automatica, italiano, inglese). */
export function LanguageSelect() {
  const { t, preference, setPreference } = useI18n();
  const id = useId();
  return (
    <span className="sd-field">
      <label htmlFor={id}>{t('settings.language')}</label>
      <select
        id={id}
        className="sd-input sd-input--inline"
        value={preference}
        onChange={(event) => setPreference(event.target.value as LocalePreference)}
      >
        {LOCALE_OPTIONS.map((value) => (
          <option key={value} value={value}>
            {t(`settings.language.${value}`)}
          </option>
        ))}
      </select>
    </span>
  );
}

/** Selettore del tema (automatico, chiaro, scuro). */
export function ThemeSelect() {
  const { t } = useI18n();
  const { mode, setMode } = useTheme();
  const id = useId();
  return (
    <span className="sd-field">
      <label htmlFor={id}>{t('settings.theme')}</label>
      <select
        id={id}
        className="sd-input sd-input--inline"
        value={mode}
        onChange={(event) => setMode(event.target.value as ThemeMode)}
      >
        {THEME_MODES.map((value) => (
          <option key={value} value={value}>
            {t(`settings.theme.${value}`)}
          </option>
        ))}
      </select>
    </span>
  );
}
