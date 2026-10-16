import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend, type FakeBackendOptions } from '../../dev/FakeBackend.js';
import { renderWith } from '../../dev/testUtils.js';
import { Dashboard } from '../Dashboard.js';

vi.mock('uplot', () => ({
  default: class {
    setSize() {}
    destroy() {}
  },
}));

async function setup(options: FakeBackendOptions = {}) {
  const backend = new FakeBackend(options);
  const run = vi.spyOn(backend, 'runAction');
  const user = userEvent.setup();
  renderWith(<Dashboard manage />, backend);
  await screen.findByRole('list', { name: 'Account' });
  return { backend, run, user };
}

async function openManage(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(screen.getByRole('button', { name: `Gestisci l’account ${name}` }));
  return screen.getByRole('group', { name: `Gestione di ${name}` });
}

describe('Aggiunta di un account', () => {
  it('«account corrente» mostra la guida a /login e lancia add con slot e alias', async () => {
    const { run, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Aggiungi account' }));
    expect(screen.getByText(/scrivi \/login/)).toBeInTheDocument();
    await user.type(screen.getByLabelText('Alias (facoltativo)'), 'nuovo');
    await user.type(screen.getByLabelText('Slot (facoltativo)'), '7');
    await user.click(screen.getByRole('button', { name: 'Aggiungi' }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'add', slot: 7, alias: 'nuovo' }));
    expect(await screen.findByText('Account aggiunto.')).toBeInTheDocument();
  });

  it('add-token: campo mascherato, token mai mostrato né tenuto nello stato dopo l’invio', async () => {
    const { backend, run, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Aggiungi account' }));
    await user.click(screen.getByRole('radio', { name: 'Con token' }));
    const field = screen.getByLabelText('Token');
    expect(field).toHaveAttribute('type', 'password');
    expect(field).toHaveAttribute('autocomplete', 'off');
    await user.type(field, 'sk-ant-oat01-SEGRETO');
    await user.click(screen.getByRole('button', { name: 'Aggiungi' }));
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({ kind: 'addToken', token: 'sk-ant-oat01-SEGRETO' }),
    );
    expect(await screen.findByText('Account con token aggiunto.')).toBeInTheDocument();
    expect(screen.getByLabelText('Token')).toHaveValue('');
    expect(document.body.innerHTML).not.toContain('SEGRETO');
    expect(JSON.stringify(backend.actionLog)).not.toContain('SEGRETO');
  });

  it('add-token fallito: l’errore non riporta il token e il campo resta vuoto', async () => {
    const { run, user } = await setup();
    run.mockRejectedValueOnce(new Error('Slot 1 already occupied'));
    await user.click(screen.getByRole('button', { name: 'Aggiungi account' }));
    await user.click(screen.getByRole('radio', { name: 'Con token' }));
    await user.type(screen.getByLabelText('Token'), 'sk-ant-api03-ALTRO');
    await user.type(screen.getByLabelText('Slot (facoltativo)'), '1');
    await user.click(screen.getByRole('button', { name: 'Aggiungi' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Slot 1 already occupied');
    expect(document.body.innerHTML).not.toContain('ALTRO');
    expect(screen.getByLabelText('Token')).toHaveValue('');
  });

  it('senza token il pulsante resta disabilitato e uno slot non valido è segnalato', async () => {
    const { user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Aggiungi account' }));
    await user.click(screen.getByRole('radio', { name: 'Con token' }));
    expect(screen.getByRole('button', { name: 'Aggiungi' })).toBeDisabled();
    await user.type(screen.getByLabelText('Token'), 'x');
    await user.type(screen.getByLabelText('Slot (facoltativo)'), '0');
    expect(screen.getByText('Lo slot è un numero intero da 1.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Aggiungi' })).toBeDisabled();
  });
});

describe('Azioni sull’account', () => {
  it('alias: imposta e toglie', async () => {
    const { run, user } = await setup();
    const panel = await openManage(user, 'alias1');
    const input = within(panel).getByLabelText('Alias');
    await user.clear(input);
    await user.type(input, 'lavoro');
    await user.click(within(panel).getByRole('button', { name: 'Imposta alias' }));
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({ kind: 'setAlias', account: 1, alias: 'lavoro' }),
    );
    await user.click(await screen.findByRole('button', { name: 'Togli alias' }));
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({ kind: 'setAlias', account: 1, alias: null }),
    );
  });

  it('alias non valido (solo cifre) blocca l’invio', async () => {
    const { user } = await setup();
    const panel = await openManage(user, 'alias1');
    const input = within(panel).getByLabelText('Alias');
    await user.clear(input);
    await user.type(input, '123');
    expect(within(panel).getByText('Alias non valido.')).toBeInTheDocument();
    expect(within(panel).getByRole('button', { name: 'Imposta alias' })).toBeDisabled();
  });

  it('disabilitare un account non attivo non chiede conferma; riabilitare nemmeno', async () => {
    const { run, user } = await setup();
    const panel = await openManage(user, 'alias1');
    await user.click(within(panel).getByRole('button', { name: 'Disabilita' }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'disable', account: 1 }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    await user.click(await within(panel).findByRole('button', { name: 'Abilita' }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'enable', account: 1 }));
  });

  it('disabilitare l’account attivo chiede conferma; annullando non parte nulla', async () => {
    const { run, user } = await setup();
    const panel = await openManage(user, 'alias6');
    await user.click(within(panel).getByRole('button', { name: 'Disabilita' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('Disabilitare alias6?');
    await user.click(within(dialog).getByRole('button', { name: 'Annulla' }));
    expect(run).not.toHaveBeenCalled();
    await user.click(within(panel).getByRole('button', { name: 'Disabilita' }));
    const again = await screen.findByRole('alertdialog');
    await user.click(within(again).getByRole('button', { name: 'Disabilita' }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'disable', account: 6 }));
  });

  it('rimuovi: conferma obbligatoria prima dell’azione', async () => {
    const { run, user } = await setup();
    const panel = await openManage(user, 'alias1');
    await user.click(within(panel).getByRole('button', { name: 'Rimuovi' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(run).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole('button', { name: 'Rimuovi' }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'remove', account: 1 }));
  });

  it('con hostConfirms la UI non mostra il suo dialogo e l’azione parte subito', async () => {
    const { backend, run, user } = await setup({ hostConfirms: true });
    const confirmation = vi.spyOn(backend, 'confirmationFor');
    const panel = await openManage(user, 'alias1');
    await user.click(within(panel).getByRole('button', { name: 'Rimuovi' }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'remove', account: 1 }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(confirmation).not.toHaveBeenCalled();
  });

  it('sposta: chiede conferma e poi lancia move', async () => {
    const { run, user } = await setup();
    const panel = await openManage(user, 'alias1');
    await user.type(within(panel).getByLabelText('Sposta nello slot'), '9');
    await user.click(within(panel).getByRole('button', { name: 'Sposta' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(dialog).toHaveTextContent('slot 9');
    await user.click(within(dialog).getByRole('button', { name: 'Sposta' }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'move', account: 1, slot: 9 }));
  });

  it('sposta in uno slot occupato avvisa dello scambio', async () => {
    const { user } = await setup();
    const panel = await openManage(user, 'alias1');
    await user.type(within(panel).getByLabelText('Sposta nello slot'), '3');
    expect(
      within(panel).getByText('Slot occupato: i due account si scambiano.'),
    ).toBeInTheDocument();
    await user.click(within(panel).getByRole('button', { name: 'Sposta' }));
    expect(await screen.findByRole('alertdialog')).toHaveTextContent('si scambiano i numeri');
  });

  it('scambia: conferma e poi swap con l’altro account', async () => {
    const { run, user } = await setup();
    const panel = await openManage(user, 'alias1');
    await user.selectOptions(within(panel).getByLabelText('Scambia con'), '3');
    await user.click(within(panel).getByRole('button', { name: 'Scambia' }));
    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: 'Scambia' }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'swap', a: 1, b: 3 }));
  });

  it('apri terminale: run con lo slot dell’account, senza conferma', async () => {
    const { run, user } = await setup();
    const panel = await openManage(user, 'alias3');
    await user.click(
      within(panel).getByRole('button', { name: 'Apri terminale come questo account' }),
    );
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'run', options: { account: 3 } }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('un errore dell’azione è mostrato come avviso', async () => {
    const { run, user } = await setup();
    run.mockRejectedValueOnce(new Error('boom'));
    const panel = await openManage(user, 'alias1');
    await user.click(
      within(panel).getByRole('button', { name: 'Apri terminale come questo account' }),
    );
    expect(await screen.findByRole('alert')).toHaveTextContent('Operazione non riuscita: boom');
  });
});
