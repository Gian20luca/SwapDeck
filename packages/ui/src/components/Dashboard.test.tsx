import type { ListResponse } from '@swapdeck/types';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { BackendProvider } from '../backend/context.js';
import { BackendError, type Backend } from '../backend/types.js';
import { FakeBackend, type FakeBackendOptions } from '../dev/FakeBackend.js';
import { I18nProvider, type LocalePreference } from '../i18n/index.js';
import { Dashboard } from './Dashboard.js';

vi.mock('uplot', () => ({
  default: class {
    setSize() {}
    destroy() {}
  },
}));

function renderDashboard(backend: Backend, locale: LocalePreference = 'it') {
  return render(
    <I18nProvider initial={locale}>
      <BackendProvider backend={backend}>
        <Dashboard />
      </BackendProvider>
    </I18nProvider>,
  );
}

function fake(options: FakeBackendOptions = {}) {
  return new FakeBackend(options);
}

async function cards() {
  const list = await screen.findByRole('list', { name: 'Account' });
  return within(list).getAllByRole('listitem');
}

describe('Dashboard: stati', () => {
  it('mostra il caricamento e poi le schede, con l’attivo per primo', async () => {
    renderDashboard(fake());
    expect(screen.getByText('Caricamento degli account…')).toBeInTheDocument();
    const items = await cards();
    expect(items).toHaveLength(5);
    expect(within(items[0]!).getByText('alias6')).toBeInTheDocument();
    expect(within(items[0]!).getByText('Attivo')).toBeInTheDocument();
    expect(within(items[1]!).getByText('alias1')).toBeInTheDocument();
  });

  it('stato vuoto senza account', async () => {
    renderDashboard(fake({ scenario: 'noAccounts' }));
    expect(await screen.findByText('Nessun account gestito')).toBeInTheDocument();
    expect(screen.queryByRole('list', { name: 'Account' })).toBeNull();
  });

  it('stato di errore con «Riprova»', async () => {
    const backend = fake();
    const spy = vi
      .spyOn(backend, 'listAccounts')
      .mockRejectedValueOnce(
        new BackendError({ kind: 'notInstalled', message: 'cswap non trovato' }),
      );
    renderDashboard(backend);
    expect(await screen.findByRole('alert')).toHaveTextContent('cswap non trovato');
    await userEvent.click(screen.getByRole('button', { name: 'Riprova' }));
    expect(await cards()).toHaveLength(5);
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('mostra gli avvisi della lista', async () => {
    const backend = fake();
    const original = backend.listAccounts.bind(backend);
    vi.spyOn(backend, 'listAccounts').mockImplementation(async (options) => {
      const data: ListResponse = await original(options);
      return { ...data, duplicateAccountWarnings: ['Account 1 e 3 hanno lo stesso login'] };
    });
    renderDashboard(backend);
    expect(await screen.findByText('Account 1 e 3 hanno lo stesso login')).toBeInTheDocument();
  });

  it('segnala il login attivo non gestito', async () => {
    const backend = fake();
    const original = backend.listAccounts.bind(backend);
    vi.spyOn(backend, 'listAccounts').mockImplementation(async (options) => ({
      ...(await original(options)),
      activeAccountNumber: null,
    }));
    renderDashboard(backend);
    expect(await screen.findByText('Il login attivo non è gestito da cswap.')).toBeInTheDocument();
  });

  it('si può usare in inglese', async () => {
    renderDashboard(fake(), 'en');
    expect(await screen.findByRole('list', { name: 'Accounts' })).toBeInTheDocument();
    expect(screen.getAllByText('Active').length).toBeGreaterThan(0);
    expect(screen.getByRole('button', { name: 'Refresh' })).toBeInTheDocument();
  });
});

describe('Dashboard: contenuto delle schede', () => {
  it('barre 5h e 7d, limiti per modello e ritmo sull’account attivo', async () => {
    renderDashboard(fake());
    const [active] = await cards();
    const bars = within(active!).getAllByRole('progressbar');
    // 5 ore, 7 giorni, Fable
    expect(bars).toHaveLength(3);
    expect(
      within(active!).getByRole('progressbar', { name: '5 ore di alias6: 66%' }),
    ).toBeInTheDocument();
    expect(
      within(active!).getByRole('progressbar', { name: '7 giorni di alias6: 77%' }),
    ).toBeInTheDocument();
    expect(
      within(active!).getByRole('progressbar', { name: 'Fable di alias6: 0%' }),
    ).toBeInTheDocument();
    expect(within(active!).getByText('Limiti per modello')).toBeInTheDocument();
    expect(within(active!).getByText('In anticipo sul ritmo')).toBeInTheDocument();
    expect(within(active!).getByText(/Si esaurisce prima del reset/)).toBeInTheDocument();
    expect(within(active!).getByText(/Dato di/)).toBeInTheDocument();
  });

  it('account con login richiesto: badge, ultimo dato noto e cambio bloccato', async () => {
    renderDashboard(fake());
    const items = await cards();
    const relogin = items.find((li) => within(li).queryByText('alias2'))!;
    expect(within(relogin).getByText('Login richiesto')).toBeInTheDocument();
    expect(within(relogin).getByText(/Ultimo dato noto/)).toBeInTheDocument();
    expect(
      within(relogin).getByRole('progressbar', { name: '7 giorni di alias2: 87%' }),
    ).toBeInTheDocument();
    const button = within(relogin).getByRole('button', { name: /Passa a questo account/ });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute('title', 'Serve un nuovo login in Claude Code');
  });

  it('il pulsante dell’account attivo è disabilitato', async () => {
    renderDashboard(fake());
    const [active] = await cards();
    expect(within(active!).getByRole('button', { name: /Passa a questo account/ })).toBeDisabled();
  });

  it('account disabilitato: badge e nessun cambio', async () => {
    renderDashboard(fake({ scenario: 'disabled' }));
    const items = await cards();
    const disabled = items.find((li) => within(li).queryByText('Disabilitato'));
    expect(disabled).toBeDefined();
    expect(
      within(disabled!).getByRole('button', { name: /Passa a questo account/ }),
    ).toBeDisabled();
  });

  it('dato vecchio: segnalato come tale', async () => {
    renderDashboard(fake({ scenario: 'stale' }));
    await cards();
    expect(screen.getAllByText(/Dato vecchio/).length).toBeGreaterThan(0);
  });
});

describe('Dashboard: cambio account', () => {
  it('«Passa a questo account» cambia l’attivo e avvisa', async () => {
    const backend = fake();
    renderDashboard(backend);
    const items = await cards();
    const target = items.find((li) => within(li).queryByText('alias1'))!;
    await userEvent.click(within(target).getByRole('button', { name: /Passa a questo account/ }));
    expect(await screen.findByText('Passato all’account alias1.')).toBeInTheDocument();
    expect(backend.actionLog.at(-1)).toMatchObject({ kind: 'switch', target: 1 });
    await waitFor(async () => {
      const [first] = await cards();
      expect(within(first!).getByText('alias1')).toBeInTheDocument();
      expect(within(first!).getByText('Attivo')).toBeInTheDocument();
    });
  });

  it('cambio rapido «Migliore»', async () => {
    const backend = fake();
    renderDashboard(backend);
    await cards();
    await userEvent.click(screen.getByRole('button', { name: 'Migliore' }));
    expect(await screen.findByText('Passato all’account alias3.')).toBeInTheDocument();
    expect(backend.actionLog.at(-1)).toMatchObject({ kind: 'switch', strategy: 'best' });
  });

  it('cambio rapido «Prossimo disponibile»', async () => {
    const backend = fake();
    renderDashboard(backend);
    await cards();
    await userEvent.click(screen.getByRole('button', { name: 'Prossimo disponibile' }));
    expect(await screen.findByText('Passato all’account alias1.')).toBeInTheDocument();
    expect(backend.actionLog.at(-1)).toMatchObject({ kind: 'switch', strategy: 'next-available' });
  });

  it('se cswap non cambia, mostra il messaggio senza errore', async () => {
    const backend = fake({ scenario: 'tokenExpired' });
    renderDashboard(backend);
    await cards();
    await userEvent.click(screen.getByRole('button', { name: 'Migliore' }));
    expect(await screen.findByText(/Nessun cambio:/)).toBeInTheDocument();
  });

  it('un errore del comando compare come avviso e i pulsanti si riabilitano', async () => {
    const backend = fake();
    vi.spyOn(backend, 'runAction').mockRejectedValueOnce(
      new BackendError({ kind: 'busy', message: 'cswap occupato' }),
    );
    renderDashboard(backend);
    await cards();
    await userEvent.click(screen.getByRole('button', { name: 'Migliore' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Cambio non riuscito: cswap occupato',
    );
    expect(screen.getByRole('button', { name: 'Migliore' })).toBeEnabled();
  });

  it('i pulsanti sono disabilitati durante il cambio', async () => {
    const backend = fake();
    let release!: () => void;
    const original = backend.runAction.bind(backend);
    vi.spyOn(backend, 'runAction').mockImplementationOnce(async (action) => {
      await new Promise<void>((resolve) => (release = resolve));
      return original(action);
    });
    renderDashboard(backend);
    await cards();
    await userEvent.click(screen.getByRole('button', { name: 'Migliore' }));
    expect(screen.getByRole('button', { name: 'Prossimo disponibile' })).toBeDisabled();
    expect(
      screen
        .getAllByRole('button', { name: /Passa a questo account/ })
        .every((b) => (b as HTMLButtonElement).disabled),
    ).toBe(true);
    release();
    await screen.findByText(/Passato all’account/);
    expect(screen.getByRole('button', { name: 'Prossimo disponibile' })).toBeEnabled();
  });
});

describe('Dashboard: aggiornamento e storico', () => {
  it('«Aggiorna» rilegge con refresh', async () => {
    const backend = fake();
    const spy = vi.spyOn(backend, 'listAccounts');
    renderDashboard(backend);
    await cards();
    await userEvent.click(screen.getByRole('button', { name: 'Aggiorna' }));
    await waitFor(() => expect(spy).toHaveBeenLastCalledWith({ refresh: true }));
  });

  it('reagisce agli eventi di aggiornamento del backend', async () => {
    const backend = fake();
    renderDashboard(backend);
    await cards();
    backend.tick();
    expect(
      await screen.findByRole('progressbar', { name: '5 ore di alias6: 68%' }),
    ).toBeInTheDocument();
  });

  it('senza le schede il pulsante Storico non c’è (si apre dalla scheda Storico)', async () => {
    renderDashboard(fake());
    await cards();
    expect(screen.queryByRole('button', { name: /storico/i })).toBeNull();
  });
});
