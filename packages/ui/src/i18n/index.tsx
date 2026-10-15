import { createContext, useCallback, useContext, useMemo, useState, type ReactNode } from 'react';
import { en } from './en.js';
import { it, type MessageKey } from './it.js';

export type { MessageKey } from './it.js';

export type Locale = 'it' | 'en';
/** Preferenza dell'utente: `auto` segue la lingua del sistema. */
export type LocalePreference = Locale | 'auto';

export const LOCALES: readonly Locale[] = ['it', 'en'];

const dictionaries: Record<Locale, Record<MessageKey, string>> = { it, en };

export type Params = Record<string, string | number>;
export type Translate = (key: MessageKey, params?: Params) => string;

/** Risolve `auto`: inglese se il sistema è in inglese, altrimenti italiano (default del progetto). */
export function resolveLocale(pref: LocalePreference, systemLanguage?: string): Locale {
  if (pref !== 'auto') return pref;
  const lang =
    (systemLanguage ?? (typeof navigator !== 'undefined' ? navigator.language : '')) || '';
  return lang.toLowerCase().startsWith('en') ? 'en' : 'it';
}

/** Sostituisce i segnaposto `{nome}`; un segnaposto senza valore resta com'è. */
export function interpolate(template: string, params?: Params): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}

export function translate(locale: Locale, key: MessageKey, params?: Params): string {
  return interpolate(dictionaries[locale][key] ?? it[key], params);
}

interface I18nValue {
  locale: Locale;
  preference: LocalePreference;
  setPreference: (pref: LocalePreference) => void;
  t: Translate;
}

const I18nContext = createContext<I18nValue | null>(null);

export interface I18nProviderProps {
  initial?: LocalePreference;
  /** Chiamata a ogni cambio di preferenza (per salvarla nella config). */
  onChange?: (pref: LocalePreference) => void;
  children: ReactNode;
}

export function I18nProvider({ initial = 'auto', onChange, children }: I18nProviderProps) {
  const [preference, setPref] = useState<LocalePreference>(initial);
  const locale = resolveLocale(preference);
  const setPreference = useCallback(
    (pref: LocalePreference) => {
      setPref(pref);
      onChange?.(pref);
    },
    [onChange],
  );
  const value = useMemo<I18nValue>(
    () => ({
      locale,
      preference,
      setPreference,
      t: (key, params) => translate(locale, key, params),
    }),
    [locale, preference, setPreference],
  );
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

/** Senza provider i componenti usano l'italiano, così i test e l'uso isolato funzionano. */
const fallback: I18nValue = {
  locale: 'it',
  preference: 'it',
  setPreference: () => undefined,
  t: (key, params) => translate('it', key, params),
};

export function useI18n(): I18nValue {
  return useContext(I18nContext) ?? fallback;
}
