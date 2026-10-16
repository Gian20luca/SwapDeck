import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { BackendProvider } from '../backend/context.js';
import { FakeBackend } from '../dev/FakeBackend.js';
import { ThemeProvider, useTheme, type ThemeMode } from '../theme/index.js';
import { WindowTheme } from './WindowTheme.js';

function Switch({ to }: { to: ThemeMode }) {
  const { setMode } = useTheme();
  return <button onClick={() => setMode(to)}>{to}</button>;
}

describe('WindowTheme', () => {
  it('porta il tema alla cornice nativa: dark, light e null per auto', async () => {
    const setWindowTheme = vi.fn().mockResolvedValue(undefined);
    const backend = Object.assign(new FakeBackend(), { setWindowTheme });
    render(
      <ThemeProvider initial="dark">
        <BackendProvider backend={backend}>
          <WindowTheme />
          <Switch to="auto" />
          <Switch to="light" />
        </BackendProvider>
      </ThemeProvider>,
    );
    // All'avvio, subito.
    expect(setWindowTheme).toHaveBeenLastCalledWith('dark');
    const [toAuto, toLight] = screen.getAllByRole('button');
    await userEvent.click(toAuto!);
    expect(setWindowTheme).toHaveBeenLastCalledWith(null);
    await userEvent.click(toLight!);
    expect(setWindowTheme).toHaveBeenLastCalledWith('light');
    expect(setWindowTheme).toHaveBeenCalledTimes(3);
  });

  it('senza setWindowTheme (VS Code) non fa niente', () => {
    expect(() =>
      render(
        <ThemeProvider initial="dark">
          <BackendProvider backend={new FakeBackend()}>
            <WindowTheme />
          </BackendProvider>
        </ThemeProvider>,
      ),
    ).not.toThrow();
  });
});
