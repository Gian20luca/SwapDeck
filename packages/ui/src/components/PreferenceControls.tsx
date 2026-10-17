import { useId } from 'react';
import { LOCALES, useI18n, type LocalePreference } from '../i18n/index.js';
import { THEME_MODES, useTheme, type ThemeMode } from '../theme/index.js';
import { WithInfo } from './InfoTip.js';
import { Tooltip } from './Tooltip.js';

const LOCALE_OPTIONS: readonly LocalePreference[] = ['auto', ...LOCALES];

/** Selettore della lingua (automatica, italiano, inglese). */
export function LanguageSelect({
  onChange,
  info,
}: { onChange?: (value: LocalePreference) => void; info?: string } = {}) {
  const { t, preference, setPreference } = useI18n();
  const id = useId();
  return (
    <span className="sd-field">
      <WithInfo info={info} name={t('settings.language')}>
        <label htmlFor={id}>{t('settings.language')}</label>
      </WithInfo>
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
export function ThemeSelect({
  onChange,
  info,
}: { onChange?: (value: ThemeMode) => void; info?: string } = {}) {
  const { t } = useI18n();
  const { mode, setMode } = useTheme();
  const id = useId();
  return (
    <span className="sd-field">
      <WithInfo info={info} name={t('settings.theme')}>
        <label htmlFor={id}>{t('settings.theme')}</label>
      </WithInfo>
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
