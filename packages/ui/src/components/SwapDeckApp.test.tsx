import { screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend } from '../dev/FakeBackend.js';
import { renderWith } from '../dev/testUtils.js';
import { SwapDeckApp } from './SwapDeckApp.js';

vi.mock('uplot', () => ({
  default: class {
    setSize() {}
    destroy() {}
  },
}));

function setup(initialTab?: 'accounts' | 'tools') {
  const backend = new FakeBackend({ mappings: [{ path: 'C:\\x', account: 1, email: '' }] });
  renderWith(
    <SwapDeckApp {...(initialTab ? { initialTab } : {})} header={<p>intestazione</p>} />,
    backend,
  );
  return userEvent.setup();
}

const TABS = ['Account', 'Cartelle', 'Regole', 'Storico', 'Strumenti', 'Impostazioni'];

describe('SwapDeckApp', () => {
  it('mostra le sei schede e parte da Account', async () => {
    setup();
    const tabs = screen.getAllByRole('tab');
    expect(tabs.map((t) => t.textContent)).toEqual(TABS);
    expect(screen.getByRole('tab', { name: 'Account' })).toHaveAttribute('aria-selected', 'true');
    expect(await screen.findByRole('heading', { level: 1, name: 'Account' })).toBeInTheDocument();
    expect(screen.getByText('intestazione')).toBeInTheDocument();
    expect(screen.getByRole('tabpanel')).toHaveAttribute(
      'aria-labelledby',
      screen.getByRole('tab', { name: 'Account' }).id,
    );
  });

  it('naviga fra le schede con il clic', async () => {
    const user = setup();
    await user.click(screen.getByRole('tab', { name: 'Cartelle' }));
    expect(await screen.findByRole('heading', { name: 'Cartelle mappate' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Regole' }));
    expect(await screen.findByRole('heading', { name: 'Regole del motore' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Storico' }));
    expect(await screen.findByRole('heading', { name: 'Storico' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Strumenti' }));
    expect(await screen.findByRole('heading', { name: 'Strumenti' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Impostazioni' }));
    expect(await screen.findByRole('heading', { name: 'Impostazioni' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Account' }));
    expect(await screen.findByRole('heading', { level: 1, name: 'Account' })).toBeInTheDocument();
  });

  it('frecce, Home e Fine spostano la scheda attiva e il focus', async () => {
    const user = setup();
    screen.getByRole('tab', { name: 'Account' }).focus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Cartelle' })).toHaveFocus();
    expect(screen.getByRole('tab', { name: 'Cartelle' })).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{ArrowLeft}{ArrowLeft}');
    expect(screen.getByRole('tab', { name: 'Impostazioni' })).toHaveFocus();
    await user.keyboard('{Home}');
    expect(screen.getByRole('tab', { name: 'Account' })).toHaveFocus();
    await user.keyboard('{End}');
    expect(screen.getByRole('tab', { name: 'Impostazioni' })).toHaveFocus();
    // Solo la scheda attiva è raggiungibile con Tab.
    expect(screen.getByRole('tab', { name: 'Account' })).toHaveAttribute('tabindex', '-1');
  });

  it('può partire da un’altra scheda', async () => {
    setup('tools');
    expect(await screen.findByRole('heading', { name: 'Strumenti' })).toBeInTheDocument();
  });
});
