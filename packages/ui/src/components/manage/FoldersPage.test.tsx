import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend, type FakeBackendOptions } from '../../dev/FakeBackend.js';
import { bareBackend, renderWith } from '../../dev/testUtils.js';
import { FoldersPage } from './FoldersPage.js';

const MAPPINGS = [
  { path: 'C:\\Progetti\\cliente-a', account: 1, email: 'user1@example.com' },
  { path: 'C:\\Progetti\\cliente-b', account: 3, email: 'user3@example.com' },
];

function setup(options: FakeBackendOptions = {}) {
  const backend = new FakeBackend({ mappings: MAPPINGS, ...options });
  const run = vi.spyOn(backend, 'runAction');
  renderWith(<FoldersPage />, backend);
  return { backend, run, user: userEvent.setup() };
}

describe('FoldersPage', () => {
  it('elenca le cartelle mappate con il nome dell’account', async () => {
    setup();
    const list = await screen.findByRole('list', { name: 'Mappature' });
    const items = within(list).getAllByRole('listitem');
    expect(items).toHaveLength(2);
    expect(items[0]).toHaveTextContent('C:\\Progetti\\cliente-a');
    expect(items[0]).toHaveTextContent('alias1');
  });

  it('account rimosso: mostra l’email e permette di togliere la mappatura', async () => {
    setup({ mappings: [{ path: '~/vecchio', account: null, email: 'old@example.com' }] });
    const list = await screen.findByRole('list', { name: 'Mappature' });
    expect(list).toHaveTextContent('old@example.com (account rimosso)');
    expect(screen.getByRole('button', { name: 'Togli la mappatura di ~/vecchio' })).toBeEnabled();
  });

  it('stato vuoto senza mappature', async () => {
    setup({ mappings: [] });
    expect(await screen.findByText('Nessuna cartella mappata.')).toBeInTheDocument();
  });

  it('mappa una cartella scelta con il selettore e aggiorna l’elenco', async () => {
    const { backend, run, user } = setup({ picks: { folder: 'C:\\Progetti\\nuova' } });
    const pick = vi.spyOn(backend, 'pickFolder');
    await screen.findByRole('list', { name: 'Mappature' });
    await user.selectOptions(screen.getByLabelText('Account'), '5');
    await user.click(screen.getByRole('button', { name: 'Sfoglia…' }));
    expect(pick).toHaveBeenCalled();
    await waitFor(() =>
      expect(screen.getByLabelText('Cartella')).toHaveValue('C:\\Progetti\\nuova'),
    );
    await user.click(screen.getByRole('button', { name: 'Mappa' }));
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({ kind: 'map', account: 5, path: 'C:\\Progetti\\nuova' }),
    );
    expect(
      await screen.findByText('C:\\Progetti\\nuova', { selector: '.sd-path' }),
    ).toBeInTheDocument();
    expect(screen.getByLabelText('Cartella')).toHaveValue('');
  });

  it('il percorso si può anche scrivere a mano e senza percorso «Mappa» è disabilitato', async () => {
    const { run, user } = setup();
    await screen.findByRole('list', { name: 'Mappature' });
    expect(screen.getByRole('button', { name: 'Mappa' })).toBeDisabled();
    await user.selectOptions(screen.getByLabelText('Account'), '6');
    await user.type(screen.getByLabelText('Cartella'), 'D:\\Lavoro');
    await user.click(screen.getByRole('button', { name: 'Mappa' }));
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({ kind: 'map', account: 6, path: 'D:\\Lavoro' }),
    );
  });

  it('unmap chiede conferma; annullando la mappatura resta', async () => {
    const { run, user } = setup();
    await screen.findByRole('list', { name: 'Mappature' });
    await user.click(
      screen.getByRole('button', { name: 'Togli la mappatura di C:\\Progetti\\cliente-a' }),
    );
    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: 'Annulla' }));
    expect(run).not.toHaveBeenCalled();
    await user.click(
      screen.getByRole('button', { name: 'Togli la mappatura di C:\\Progetti\\cliente-a' }),
    );
    await user.click(
      within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Togli' }),
    );
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({ kind: 'unmap', path: 'C:\\Progetti\\cliente-a' }),
    );
    await waitFor(() =>
      expect(screen.queryByText('C:\\Progetti\\cliente-a', { selector: '.sd-path' })).toBeNull(),
    );
  });

  it('senza il selettore dell’host non c’è «Sfoglia…»', async () => {
    const backend = new FakeBackend({ mappings: MAPPINGS });
    renderWith(<FoldersPage />, {
      ...bareBackend(backend),
      listMappings: () => backend.listMappings(),
    });
    await screen.findByRole('list', { name: 'Mappature' });
    expect(screen.queryByRole('button', { name: 'Sfoglia…' })).toBeNull();
  });

  it('host senza elenco delle mappature: funzione non disponibile', async () => {
    renderWith(<FoldersPage />, bareBackend(new FakeBackend()));
    expect(await screen.findByText(/non è disponibile in questo ambiente/)).toBeInTheDocument();
  });

  it('errore di lettura con «Riprova»', async () => {
    const backend = new FakeBackend();
    const list = vi
      .spyOn(backend, 'listMappings')
      .mockRejectedValueOnce(new Error('illeggibile'))
      .mockResolvedValue([]);
    renderWith(<FoldersPage />, backend);
    expect(await screen.findByRole('alert')).toHaveTextContent('illeggibile');
    await userEvent.click(screen.getByRole('button', { name: 'Riprova' }));
    expect(await screen.findByText('Nessuna cartella mappata.')).toBeInTheDocument();
    expect(list).toHaveBeenCalledTimes(2);
  });
});
