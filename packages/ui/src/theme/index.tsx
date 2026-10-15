import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

export type ThemeMode = 'light' | 'dark' | 'auto';
export type ResolvedTheme = 'light' | 'dark';

export const THEME_MODES: readonly ThemeMode[] = ['auto', 'light', 'dark'];

const DARK_QUERY = '(prefers-color-scheme: dark)';

function systemPrefersDark(): boolean {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
    ? window.matchMedia(DARK_QUERY).matches
    : false;
}

export function resolveTheme(mode: ThemeMode, systemDark: boolean): ResolvedTheme {
  if (mode === 'auto') return systemDark ? 'dark' : 'light';
  return mode;
}

interface ThemeValue {
  mode: ThemeMode;
  resolved: ResolvedTheme;
  setMode: (mode: ThemeMode) => void;
}

const ThemeContext = createContext<ThemeValue | null>(null);

export interface ThemeProviderProps {
  initial?: ThemeMode;
  /** Chiamata a ogni cambio di modo (per salvarlo nella config). */
  onChange?: (mode: ThemeMode) => void;
  /** Elemento che riceve `data-theme`; di default `document.documentElement`. */
  target?: HTMLElement;
  children: ReactNode;
}

/** Tema chiaro/scuro/auto: imposta `data-theme` sull'elemento radice; i colori sono variabili CSS. */
export function ThemeProvider({
  initial = 'auto',
  onChange,
  target,
  children,
}: ThemeProviderProps) {
  const [mode, setModeState] = useState<ThemeMode>(initial);
  const [systemDark, setSystemDark] = useState<boolean>(systemPrefersDark);

  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return;
    const query = window.matchMedia(DARK_QUERY);
    const listener = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    query.addEventListener('change', listener);
    return () => query.removeEventListener('change', listener);
  }, []);

  const resolved = resolveTheme(mode, systemDark);

  useEffect(() => {
    const el = target ?? document.documentElement;
    el.setAttribute('data-theme', resolved);
    el.style.colorScheme = resolved;
  }, [resolved, target]);

  const setMode = useCallback(
    (next: ThemeMode) => {
      setModeState(next);
      onChange?.(next);
    },
    [onChange],
  );

  const value = useMemo<ThemeValue>(() => ({ mode, resolved, setMode }), [mode, resolved, setMode]);
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

const fallback: ThemeValue = { mode: 'auto', resolved: 'light', setMode: () => undefined };

export function useTheme(): ThemeValue {
  return useContext(ThemeContext) ?? fallback;
}
