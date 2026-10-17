import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend, type FakeBackendOptions } from '../../dev/FakeBackend.js';
import { renderWith } from '../../dev/testUtils.js';
import { ToolsPage } from './ToolsPage.js';

async function setup(options: FakeBackendOptions = {}) {
  const backend = new FakeBackend({ unclaimed: ['legacy-1f3a', 'legacy-9c2b'], ...options });
  const run = vi.spyOn(backend, 'runAction');
  renderWith(<ToolsPage />, backend);
  await screen.findByRole('list', { name: 'Credenziali parcheggiate' });
  return { backend, run, user: userEvent.setup() };
}

describe('ToolsPage: export', () => {
  it('avvisa che il file contiene le credenziali e chiede conferma prima di esportare', async () => {
    const { run, user } = await setup({ picks: { saveFile: 'C:\\Temp\\copia.json' } });
    expect(screen.getByRole('note')).toHaveTextContent('credenziali in chiaro');
    expect(screen.getByRole('button', { name: 'Esporta…' })).toBeDisabled();
    await user.click(screen.getAllByRole('button', { name: 'Sfoglia…' })[0]!);
    await waitFor(() =>
      expect(screen.getAllByLabelText('File')[0]).toHaveValue('C:\\Temp\\copia.json'),
    );
    await user.click(screen.getByRole('button', { name: 'Esporta…' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('credenziali in chiaro');
    expect(run).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole('button', { name: 'Esporta…' }));
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({
        kind: 'export',
        path: 'C:\\Temp\\copia.json',
        full: true,
      }),
    );
    expect(
      await screen.findByText('Esportazione salvata in C:\\Temp\\copia.json.'),
    ).toBeInTheDocument();
  });

  it('export di un solo account', async () => {
    const { run, user } = await setup();
    await user.selectOptions(screen.getByLabelText('Cosa esportare'), '3');
    await user.type(screen.getAllByLabelText('File')[0]!, 'out.json');
    await user.click(screen.getByRole('button', { name: 'Esporta…' }));
    await user.click(
      within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Esporta…' }),
    );
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({ kind: 'export', path: 'out.json', account: 3 }),
    );
  });

  it('annullando la conferma non esporta nulla', async () => {
    const { run, user } = await setup();
    await user.type(screen.getAllByLabelText('File')[0]!, 'out.json');
    await user.click(screen.getByRole('button', { name: 'Esporta…' }));
    await user.click(
      within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Annulla' }),
    );
    expect(run).not.toHaveBeenCalled();
  });
});

describe('ToolsPage: import', () => {
  it('import chiede conferma e passa --force solo se scelto', async () => {
    const { run, user } = await setup({ picks: { openFile: 'C:\\Temp\\in.json' } });
    await user.click(screen.getAllByRole('button', { name: 'Sfoglia…' })[1]!);
    await waitFor(() =>
      expect(screen.getAllByLabelText('File')[1]).toHaveValue('C:\\Temp\\in.json'),
    );
    await user.click(screen.getByRole('button', { name: 'Importa…' }));
    let dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('quelli già presenti restano');
    await user.click(within(dialog).getByRole('button', { name: 'Importa…' }));
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({ kind: 'import', path: 'C:\\Temp\\in.json', force: false }),
    );

    await user.click(screen.getByLabelText('Sovrascrivi gli account già presenti'));
    await user.click(screen.getByRole('button', { name: 'Importa…' }));
    dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('sovrascritti');
    await user.click(within(dialog).getByRole('button', { name: 'Importa…' }));
    await waitFor(() =>
      expect(run).toHaveBeenLastCalledWith({
        kind: 'import',
        path: 'C:\\Temp\\in.json',
        force: true,
      }),
    );
  });
});

describe('ToolsPage: unclaimed e purge', () => {
  it('elenca le credenziali parcheggiate; unclaimed --purge chiede conferma', async () => {
    const { run, user } = await setup();
    const list = screen.getByRole('list', { name: 'Credenziali parcheggiate' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(2);
    await user.click(screen.getByRole('button', { name: 'Elimina la credenziale legacy-1f3a' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(run).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole('button', { name: 'Elimina' }));
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({ kind: 'unclaimedPurge', id: 'legacy-1f3a' }),
    );
    await waitFor(() => expect(within(list).getAllByRole('listitem')).toHaveLength(1));
  });

  it('stato vuoto senza credenziali parcheggiate', async () => {
    const backend = new FakeBackend();
    renderWith(<ToolsPage />, backend);
    expect(await screen.findByText('Nessuna credenziale parcheggiata.')).toBeInTheDocument();
  });

  it('purge: doppia conferma con la parola da digitare', async () => {
    const { run, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Elimina tutti i dati…' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('Passo 1 di 2');
    await user.click(within(dialog).getByRole('button', { name: 'Continua' }));
    expect(run).not.toHaveBeenCalled();
    expect(dialog).toHaveTextContent('Passo 2 di 2');
    const confirm = within(dialog).getByRole('button', { name: 'Elimina tutti i dati…' });
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByLabelText('Parola di conferma'), 'purg');
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByLabelText('Parola di conferma'), 'e');
    await user.click(confirm);
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'purge' }));
    expect(await screen.findByText('Dati di cswap eliminati.')).toBeInTheDocument();
  });

  it('purge annullato al secondo passo: non parte nulla', async () => {
    const { run, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Elimina tutti i dati…' }));
    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: 'Continua' }));
    await user.click(within(dialog).getByRole('button', { name: 'Annulla' }));
    expect(run).not.toHaveBeenCalled();
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('con hostConfirms purge parte senza il dialogo della UI', async () => {
    const { run, user } = await setup({ hostConfirms: true });
    await user.click(screen.getByRole('button', { name: 'Elimina tutti i dati…' }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'purge' }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });
});

describe('ToolsPage: diagnostica', () => {
  it('copia la diagnostica negli appunti e apre la cartella dei log', async () => {
    const { backend, user } = await setup();
    const section = screen.getByRole('region', { name: 'Diagnostica' });
    await user.click(within(section).getByRole('button', { name: 'Copia diagnostica' }));
    expect(await within(section).findByText('Diagnostica copiata negli appunti.')).toBeVisible();
    expect(await navigator.clipboard.readText()).toContain('Diagnostica di SwapDeck');
    await user.click(within(section).getByRole('button', { name: 'Apri cartella dei log' }));
    await waitFor(() => expect(backend.logsFolderOpened).toBe(1));
  });

  it('usa gli appunti dell’host se ci sono; se falliscono mostra il testo da copiare a mano', async () => {
    const backend = new FakeBackend();
    const copyText = vi.fn().mockRejectedValue(new Error('no clipboard'));
    Object.assign(backend, { copyText });
    renderWith(<ToolsPage />, backend);
    const user = userEvent.setup();
    const section = await screen.findByRole('region', { name: 'Diagnostica' });
    await user.click(within(section).getByRole('button', { name: 'Copia diagnostica' }));
    const text = await within(section).findByRole('textbox', { name: 'Testo della diagnostica' });
    expect((text as HTMLTextAreaElement).value).toContain('Diagnostica di SwapDeck');
    expect(copyText).toHaveBeenCalledWith(expect.stringContaining('Motore:'));
    expect(within(section).getByRole('alert')).toHaveTextContent('Appunti non disponibili');
  });

  it('senza metodi di diagnostica nell’host la sezione non c’è', async () => {
    const backend = new FakeBackend();
    Object.assign(backend, { getDiagnostics: undefined, openLogsFolder: undefined });
    renderWith(<ToolsPage />, backend);
    await screen.findByRole('heading', { name: 'Strumenti' });
    expect(screen.queryByRole('region', { name: 'Diagnostica' })).toBeNull();
  });
});
