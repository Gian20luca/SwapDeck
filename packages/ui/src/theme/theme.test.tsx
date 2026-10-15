import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { resolveTheme, ThemeProvider, useTheme } from './index.js';

function Probe() {
  const { mode, resolved, setMode } = useTheme();
  return (
    <div>
      <span data-testid="state">{`${mode}/${resolved}`}</span>
      <button onClick={() => setMode('dark')}>scuro</button>
      <button onClick={() => setMode('auto')}>auto</button>
    </div>
  );
}

function mockMatchMedia(dark: boolean) {
  let listener: ((e: MediaQueryListEvent) => void) | undefined;
  const mql = {
    matches: dark,
    addEventListener: (_: string, l: (e: MediaQueryListEvent) => void) => (listener = l),
    removeEventListener: () => undefined,
  };
  vi.stubGlobal('matchMedia', () => mql);
  window.matchMedia = (() => mql) as unknown as typeof window.matchMedia;
  return (next: boolean) => listener?.({ matches: next } as MediaQueryListEvent);
}

afterEach(() => {
  document.documentElement.removeAttribute('data-theme');
  vi.unstubAllGlobals();
});

describe('resolveTheme', () => {
  it('auto segue il sistema, gli altri sono espliciti', () => {
    expect(resolveTheme('auto', true)).toBe('dark');
    expect(resolveTheme('auto', false)).toBe('light');
    expect(resolveTheme('light', true)).toBe('light');
    expect(resolveTheme('dark', false)).toBe('dark');
  });
});

describe('ThemeProvider', () => {
  it('imposta data-theme e reagisce al cambio di modo', async () => {
    mockMatchMedia(false);
    const seen: string[] = [];
    render(
      <ThemeProvider onChange={(m) => seen.push(m)}>
        <Probe />
      </ThemeProvider>,
    );
    expect(document.documentElement).toHaveAttribute('data-theme', 'light');
    await userEvent.click(screen.getByText('scuro'));
    expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
    expect(screen.getByTestId('state')).toHaveTextContent('dark/dark');
    expect(seen).toEqual(['dark']);
  });

  it('in modo auto segue le variazioni del sistema', () => {
    const emit = mockMatchMedia(false);
    render(
      <ThemeProvider initial="auto">
        <Probe />
      </ThemeProvider>,
    );
    expect(screen.getByTestId('state')).toHaveTextContent('auto/light');
    act(() => emit(true));
    expect(screen.getByTestId('state')).toHaveTextContent('auto/dark');
    expect(document.documentElement).toHaveAttribute('data-theme', 'dark');
  });
});
