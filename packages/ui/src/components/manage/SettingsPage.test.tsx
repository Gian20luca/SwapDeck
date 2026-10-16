import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend } from '../../dev/FakeBackend.js';
import { bareBackend, renderWith } from '../../dev/testUtils.js';
import { parseThresholds, SettingsPage } from './SettingsPage.js';

/** Senza la pagina Notifiche (host della versione 1): restano le soglie classiche. */
async function setup() {
  const backend = new FakeBackend();
  (backend as { getNotificationSettings?: unknown }).getNotificationSettings = undefined;
  const run = vi.spyOn(backend, 'runAction');
  const setPrefs = vi.spyOn(backend, 'setPreferences');
  const setAutostart = vi.spyOn(backend, 'setAutostart');
  renderWith(<SettingsPage />, backend);
  await screen.findByLabelText('Intervallo di controllo (secondi)');
  await screen.findByRole('list', { name: 'Configurazione di cswap' });
  return { backend, run, setPrefs, setAutostart, user: userEvent.setup() };
}

describe('parseThresholds', () => {
  it('accetta valori crescenti da 1 a 100, al massimo quattro', () => {
    expect(parseThresholds('80, 95')).toEqual([80, 95]);
    expect(parseThresholds('50;70 90')).toEqual([50, 70, 90]);
    expect(parseThresholds('')).toBeNull();
    expect(parseThresholds('95, 80')).toBeNull();
    expect(parseThresholds('80, 80')).toBeNull();
    expect(parseThresholds('0, 50')).toBeNull();
    expect(parseThresholds('50, 101')).toBeNull();
    expect(parseThresholds('a, 50')).toBeNull();
    expect(parseThresholds('10, 20, 30, 40, 50')).toBeNull();
  });
});

describe('SettingsPage: impostazioni dell’app', () => {
  it('salva intervallo e soglie delle notifiche', async () => {
    const { setPrefs, user } = await setup();
    const interval = screen.getByLabelText('Intervallo di controllo (secondi)');
    expect(interval).toHaveValue(60);
    expect(screen.getByLabelText('Soglie delle notifiche (%)')).toHaveValue('80, 95');
    await user.clear(interval);
    await user.type(interval, '120');
    const thresholds = screen.getByLabelText('Soglie delle notifiche (%)');
    await user.clear(thresholds);
    await user.type(thresholds, '70, 90');
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() =>
      expect(setPrefs).toHaveBeenCalledWith({
        pollIntervalSeconds: 120,
        notificationThresholds: [70, 90],
      }),
    );
    expect(await screen.findByText('Impostazioni salvate.')).toBeInTheDocument();
  });

  it('intervallo e soglie non validi bloccano il salvataggio', async () => {
    const { setPrefs, user } = await setup();
    const interval = screen.getByLabelText('Intervallo di controllo (secondi)');
    await user.clear(interval);
    await user.type(interval, '30');
    const thresholds = screen.getByLabelText('Soglie delle notifiche (%)');
    await user.clear(thresholds);
    await user.type(thresholds, '95, 80');
    expect(screen.getByText('L’intervallo è un numero intero da 60 a 3600.')).toBeInTheDocument();
    expect(
      screen.getByText('Valori crescenti da 1 a 100, al massimo quattro.'),
    ).toBeInTheDocument();
    const form = interval.closest('form')!;
    expect(within(form).getByRole('button', { name: 'Salva' })).toBeDisabled();
    expect(setPrefs).not.toHaveBeenCalled();
  });

  it('lingua e tema vengono salvati nella config', async () => {
    const { setPrefs, user } = await setup();
    await user.selectOptions(screen.getByLabelText('Lingua'), 'en');
    await waitFor(() => expect(setPrefs).toHaveBeenCalledWith({ language: 'en' }));
    await user.selectOptions(screen.getByLabelText('Theme'), 'dark');
    await waitFor(() => expect(setPrefs).toHaveBeenCalledWith({ theme: 'dark' }));
  });

  it('avvio al login: casella che chiama setAutostart', async () => {
    const { setAutostart, user } = await setup();
    const box = await screen.findByLabelText('Avvia SwapDeck al login');
    expect(box).toBeChecked();
    await user.click(box);
    await waitFor(() => expect(setAutostart).toHaveBeenCalledWith(false));
    await waitFor(() => expect(screen.getByLabelText('Avvia SwapDeck al login')).not.toBeChecked());
    expect(screen.getByText('Versione 0.0.0-fake')).toBeInTheDocument();
  });

  it('senza il supporto dell’host (VS Code) niente avvio al login né salvataggio delle preferenze', async () => {
    renderWith(<SettingsPage />, bareBackend(new FakeBackend()));
    await screen.findByRole('list', { name: 'Configurazione di cswap' });
    expect(screen.queryByLabelText('Avvia SwapDeck al login')).toBeNull();
    expect(screen.queryByLabelText('Intervallo di controllo (secondi)')).toBeNull();
  });
});

describe('SettingsPage: configurazione di cswap', () => {
  it('elenca le chiavi del contratto con etichetta e stato', async () => {
    await setup();
    const list = screen.getByRole('list', { name: 'Configurazione di cswap' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(9);
    expect(within(list).getByText('autoswitch.threshold · default')).toBeInTheDocument();
  });

  it('config set: numero, scelta e booleano', async () => {
    const { run, user } = await setup();
    const threshold = screen.getByLabelText('Soglia di cambio (%)');
    await user.clear(threshold);
    await user.type(threshold, '85');
    await user.click(screen.getByRole('button', { name: 'Salva autoswitch.threshold' }));
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({
        kind: 'configSet',
        key: 'autoswitch.threshold',
        value: '85',
      }),
    );
    expect(await screen.findByText('autoswitch.threshold salvato.')).toBeInTheDocument();
    expect(
      await screen.findByRole('button', { name: 'Ripristina il default di autoswitch.threshold' }),
    ).toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText('Strategia'), 'consume-first');
    await userEvent.click(screen.getByRole('button', { name: 'Salva autoswitch.strategy' }));
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({
        kind: 'configSet',
        key: 'autoswitch.strategy',
        value: 'consume-first',
      }),
    );

    await userEvent.click(screen.getByLabelText('Includi gli account con chiave API'));
    await userEvent.click(
      screen.getByRole('button', { name: 'Salva autoswitch.includeApiKeyAccounts' }),
    );
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({
        kind: 'configSet',
        key: 'autoswitch.includeApiKeyAccounts',
        value: 'true',
      }),
    );
  });

  it('valore fuori intervallo: errore e «Salva» disabilitato', async () => {
    const { run, user } = await setup();
    const threshold = screen.getByLabelText('Soglia di cambio (%)');
    await user.clear(threshold);
    await user.type(threshold, '10');
    expect(screen.getByText('Valore fuori dall’intervallo ammesso.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Salva autoswitch.threshold' })).toBeDisabled();
    expect(run).not.toHaveBeenCalled();
  });

  it('config unset riporta la chiave al default', async () => {
    const { run, user } = await setup();
    const interval = screen.getByLabelText('Intervallo (secondi)');
    await user.clear(interval);
    await user.type(interval, '90');
    await user.click(screen.getByRole('button', { name: 'Salva autoswitch.intervalSeconds' }));
    const reset = await screen.findByRole('button', {
      name: 'Ripristina il default di autoswitch.intervalSeconds',
    });
    await user.click(reset);
    await waitFor(() =>
      expect(run).toHaveBeenCalledWith({ kind: 'configUnset', key: 'autoswitch.intervalSeconds' }),
    );
    expect(
      await screen.findByText('autoswitch.intervalSeconds riportato al default.'),
    ).toBeInTheDocument();
  });

  it('errore di cswap mostrato come avviso', async () => {
    const { run, user } = await setup();
    run.mockRejectedValueOnce(new Error('value out of range'));
    const cooldown = screen.getByLabelText('Pausa tra i cambi (secondi)');
    await user.clear(cooldown);
    await user.type(cooldown, '10');
    await user.click(screen.getByRole('button', { name: 'Salva autoswitch.cooldownSeconds' }));
    expect(await screen.findByText(/value out of range/)).toBeInTheDocument();
  });
});
