import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend } from '../../dev/FakeBackend.js';
import { bareBackend, renderWith } from '../../dev/testUtils.js';
import { SettingsPage } from './SettingsPage.js';

async function setup(backend = new FakeBackend()) {
  const save = vi.spyOn(backend, 'setNotificationSettings');
  const user = userEvent.setup();
  renderWith(<SettingsPage />, backend);
  await user.click(await screen.findByRole('button', { name: 'Notifiche…' }));
  await screen.findByLabelText('Preavviso di cambio');
  return { backend, save, user };
}

describe('Pagina Notifiche', () => {
  it('si apre dalle impostazioni, mostra i default e «Indietro» torna', async () => {
    const { user } = await setup();
    expect(screen.getByLabelText('Preavviso di cambio')).toBeChecked();
    expect(screen.getByLabelText('Reset imminente')).not.toBeChecked();
    expect(screen.getByLabelText('Reset avvenuto')).not.toBeChecked();
    // Il tooltip spiega perché il reset è spento di default.
    const reset = screen.getByLabelText('Reset imminente');
    const tip = document.getElementById(reset.getAttribute('aria-describedby')!.split(' ').pop()!);
    expect(tip).toHaveTextContent(/ogni 5 ore/);
    expect(screen.getByText(/non si accumulano/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /Indietro/ }));
    expect(screen.getByRole('heading', { name: 'Impostazioni' })).toBeInTheDocument();
  });

  it('interruttori, parametri e ambito: si salva solo la patch delle sezioni cambiate', async () => {
    const { save, user } = await setup();
    await user.click(screen.getByLabelText('Reset imminente'));
    const group = screen.getByTestId('notif-resetSoon');
    await user.selectOptions(within(group).getByLabelText('Utenze'), 'all');
    const minutes = within(group).getByLabelText('Minuti prima del reset');
    await user.clear(minutes);
    await user.type(minutes, '30');
    const five = screen.getByLabelText('Soglie sulle 5 ore (%)');
    await user.clear(five);
    await user.type(five, '70, 90');
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    expect(save.mock.calls[0]![0]).toEqual({
      resetSoon: { enabled: true, scope: 'all', minutesBefore: 30 },
      percent: { enabled: true, scope: 'active', fiveHour: [70, 90], sevenDay: [80, 95] },
    });
    expect(await screen.findByText('Notifiche salvate.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Salva' })).toBeDisabled();
  });

  it('non disturbare con orari ed eccezione per i critici; riepilogo con ora', async () => {
    const { save, user } = await setup();
    await user.click(screen.getByLabelText('Non disturbare a orari'));
    expect(screen.getByLabelText('Lascia passare i critici')).toBeChecked();
    await user.click(screen.getByLabelText('Lascia passare i critici'));
    await user.click(screen.getByLabelText('Riepilogo giornaliero'));
    const time = screen.getByLabelText('Ora del riepilogo');
    await user.clear(time);
    await user.type(time, '18:30');
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]![0]).toMatchObject({
      quietHours: { enabled: true, start: '22:00', end: '08:00', allowCritical: false },
      dailySummary: { enabled: true, time: '18:30' },
    });
  });

  it('valori non validi bloccano il salvataggio', async () => {
    const { save, user } = await setup();
    const points = screen.getByLabelText('Punti prima della soglia');
    await user.clear(points);
    await user.type(points, '80');
    expect(screen.getByText('Valore fuori dall’intervallo 0–50.')).toBeInTheDocument();
    const seven = screen.getByLabelText('Soglie sui 7 giorni (%)');
    await user.clear(seven);
    await user.type(seven, '95, 80');
    expect(screen.getByText(/soglie crescenti/)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Salva' })).toBeDisabled();
    expect(save).not.toHaveBeenCalled();
  });

  it('errore del backend mostrato, bozza conservata; «Annulla modifiche» ripristina', async () => {
    const { save, user } = await setup();
    save.mockRejectedValueOnce(new Error('config bloccata'));
    await user.click(screen.getByLabelText('Aggiornamenti'));
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    expect(await screen.findByText('Operazione non riuscita: config bloccata')).toBeInTheDocument();
    expect(screen.getByLabelText('Aggiornamenti')).not.toBeChecked();
    await user.click(screen.getByRole('button', { name: 'Annulla modifiche' }));
    expect(screen.getByLabelText('Aggiornamenti')).toBeChecked();
  });

  it('con la pagina Notifiche le soglie classiche spariscono dalle impostazioni', async () => {
    renderWith(<SettingsPage />, new FakeBackend());
    await screen.findByLabelText('Intervallo di controllo (secondi)');
    expect(screen.queryByLabelText('Soglie delle notifiche (%)')).toBeNull();
  });

  it('host senza notifiche: nessun accesso alla pagina, restano le soglie classiche', async () => {
    const source = new FakeBackend();
    const backend = {
      ...bareBackend(source),
      getPreferences: () => source.getPreferences(),
      setPreferences: (p: Parameters<FakeBackend['setPreferences']>[0]) => source.setPreferences(p),
    };
    renderWith(<SettingsPage />, backend);
    expect(await screen.findByLabelText('Soglie delle notifiche (%)')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Notifiche…' })).toBeNull();
  });
});
