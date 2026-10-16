import type { Conflict } from '@swapdeck/types';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend, type FakeBackendOptions } from '../../dev/FakeBackend.js';
import { bareBackend, renderWith } from '../../dev/testUtils.js';
import { RulesPage } from './RulesPage.js';

const GUARD: Conflict = {
  id: 'task:cswap-guard',
  kind: 'scheduledTask',
  label: 'Attività pianificata cswap-guard',
  certain: true,
};
const TUI: Conflict = { id: 'process:tui', kind: 'process', label: 'cswap tui', certain: false };

async function setup(options: FakeBackendOptions = {}) {
  const backend = new FakeBackend(options);
  const save = vi.spyOn(backend, 'setEngineRules');
  renderWith(<RulesPage />, backend);
  await screen.findByRole('heading', { name: 'Regole del motore' });
  return { backend, save, user: userEvent.setup() };
}

describe('RulesPage: editor', () => {
  it('mostra le regole salvate e non permette di salvare finché non cambia nulla', async () => {
    await setup({ rules: { enabled: true, primary: 6, reserves: [1, 3] } });
    expect(screen.getByLabelText('Motore acceso')).toBeChecked();
    expect(screen.getByLabelText('Account principale')).toHaveValue('6');
    expect(screen.getByLabelText('Soglia sulle 5 ore (%)')).toHaveValue(95);
    const reserves = screen.getByRole('list', { name: 'Riserve, in ordine di preferenza' });
    expect(within(reserves).getAllByRole('listitem')).toHaveLength(2);
    expect(screen.getByRole('button', { name: 'Salva' })).toBeDisabled();
  });

  it('il preset «come cswap-guard» compila i campi e il salvataggio invia le regole', async () => {
    const { save, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Preset come cswap-guard' }));
    expect(screen.getByLabelText('Account principale')).toHaveValue('6');
    expect(screen.getByLabelText('Criterio di scelta della riserva')).toHaveValue('lowest-weekly');
    expect(screen.getByLabelText('Soglia sulle 5 ore (%)')).toHaveValue(95);
    expect(screen.getByRole('status')).toHaveTextContent('Preset applicato');
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    expect(save.mock.calls[0]![0]).toMatchObject({
      primary: 6,
      reserves: [1, 3],
      reserveCriterion: 'lowest-weekly',
      fiveHourThreshold: 95,
      returnToPrimary: true,
      enabled: false,
    });
    expect(await screen.findByText('Regole salvate.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Salva' })).toBeDisabled();
  });

  it('modifica di motore, osservazione, soglie, criterio, isteresi e pausa', async () => {
    const { save, user } = await setup({ rules: { primary: 6, reserves: [1, 3] } });
    await user.click(screen.getByLabelText('Motore acceso'));
    await user.click(screen.getByLabelText('Solo osservazione'));
    const five = screen.getByLabelText('Soglia sulle 5 ore (%)');
    await user.clear(five);
    await user.type(five, '90');
    const seven = screen.getByLabelText('Soglia sui 7 giorni (%)');
    await user.clear(seven);
    await user.selectOptions(
      screen.getByLabelText('Criterio di scelta della riserva'),
      'fixed-order',
    );
    const hyst = screen.getByLabelText('Isteresi del ritorno (punti %)');
    await user.clear(hyst);
    await user.type(hyst, '5');
    const gap = screen.getByLabelText('Pausa minima tra due cambi (secondi)');
    await user.clear(gap);
    await user.type(gap, '600');
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]![0]).toMatchObject({
      enabled: true,
      dryRun: true,
      fiveHourThreshold: 90,
      sevenDayThreshold: null,
      reserveCriterion: 'fixed-order',
      returnHysteresis: 5,
      minSwitchGapSecs: 600,
    });
  });

  it('riserve ordinabili: sposta giù, togli, aggiungi', async () => {
    const { save, user } = await setup({ rules: { primary: 6, reserves: [1, 3] } });
    await user.click(screen.getByRole('button', { name: 'Sposta giù alias1 (#1)' }));
    const items = within(
      screen.getByRole('list', { name: 'Riserve, in ordine di preferenza' }),
    ).getAllByRole('listitem');
    expect(items[0]).toHaveTextContent('alias3');
    await user.click(screen.getByRole('button', { name: 'Togli alias1 (#1) dalle riserve' }));
    await user.selectOptions(screen.getByLabelText('Riserva da aggiungere'), '5');
    await user.click(screen.getByRole('button', { name: 'Aggiungi riserva' }));
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]![0].reserves).toEqual([3, 5]);
  });

  it('escludere un account lo toglie dalle riserve', async () => {
    const { save, user } = await setup({ rules: { primary: 6, reserves: [1, 3] } });
    const excluded = screen.getByRole('group', { name: 'Account esclusi' });
    await user.click(within(excluded).getByLabelText('alias3 (#3)'));
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]![0]).toMatchObject({ reserves: [1], excluded: [3] });
  });

  it('valori non validi: errori accanto ai campi e salvataggio bloccato', async () => {
    const { save, user } = await setup({ rules: { enabled: true, primary: 6 } });
    const five = screen.getByLabelText('Soglia sulle 5 ore (%)');
    await user.clear(five);
    await user.type(five, '150');
    const hyst = screen.getByLabelText('Isteresi del ritorno (punti %)');
    await user.clear(hyst);
    await user.type(hyst, '80');
    expect(screen.getByText('La soglia va da 1 a 100.')).toBeInTheDocument();
    expect(screen.getByText('L’isteresi va da 0 a 50.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Salva' })).toBeDisabled();
    expect(save).not.toHaveBeenCalled();
  });

  it('con il motore acceso il principale è obbligatorio', async () => {
    const { user } = await setup();
    await user.click(screen.getByLabelText('Motore acceso'));
    expect(
      screen.getByText('Con il motore acceso serve l’account principale.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Salva' })).toBeDisabled();
  });

  it('«Annulla modifiche» ripristina le regole salvate', async () => {
    const { user } = await setup({ rules: { primary: 6 } });
    await user.click(screen.getByLabelText('Solo osservazione'));
    expect(screen.getByLabelText('Solo osservazione')).toBeChecked();
    await user.click(screen.getByRole('button', { name: 'Annulla modifiche' }));
    expect(screen.getByLabelText('Solo osservazione')).not.toBeChecked();
  });

  it('errore al salvataggio mostrato come avviso, bozza conservata', async () => {
    const { save, user } = await setup({ rules: { primary: 6 } });
    save.mockRejectedValueOnce(new Error('disco pieno'));
    await user.click(screen.getByLabelText('Solo osservazione'));
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    expect(await screen.findByText('Operazione non riuscita: disco pieno')).toBeInTheDocument();
    expect(screen.getByLabelText('Solo osservazione')).toBeChecked();
  });
});

describe('RulesPage: stato sospeso e conflitti', () => {
  it('motore spento: spiega che è spento', async () => {
    await setup();
    expect(screen.getByRole('alert', { name: 'Stato del motore' })).toHaveTextContent(
      'Il motore è spento.',
    );
  });

  it('sospeso per conflitto: elenca il conflitto e permette di ignorarlo', async () => {
    const { save, user } = await setup({
      rules: { enabled: true, primary: 6, reserves: [1, 3] },
      conflicts: [GUARD, TUI],
    });
    const status = screen.getByRole('alert', { name: 'Stato del motore' });
    expect(status).toHaveTextContent('Motore sospeso.');
    expect(status).toHaveTextContent('Attività pianificata cswap-guard');
    expect(within(status).queryByText('cswap tui')).toBeNull();
    await user.click(
      screen.getByRole('button', { name: 'Ignora il conflitto Attività pianificata cswap-guard' }),
    );
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]![0].ignoredConflicts).toEqual(['task:cswap-guard']);
    // Ignorato: il motore non è più sospeso.
    await waitFor(() =>
      expect(screen.getByRole('status', { name: 'Stato del motore' })).toHaveTextContent(
        'Motore acceso',
      ),
    );
    await user.click(
      screen.getByRole('button', {
        name: 'Smetti di ignorare il conflitto Attività pianificata cswap-guard',
      }),
    );
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2));
    expect(save.mock.calls[1]![0].ignoredConflicts).toEqual([]);
  });

  it('ignorare un conflitto non salva le altre modifiche in corso', async () => {
    const { save, user } = await setup({
      rules: { enabled: true, primary: 6, reserves: [1] },
      conflicts: [GUARD],
    });
    await user.click(screen.getByLabelText('Solo osservazione'));
    await user.click(
      screen.getByRole('button', { name: 'Ignora il conflitto Attività pianificata cswap-guard' }),
    );
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]![0]).toMatchObject({
      dryRun: false,
      ignoredConflicts: ['task:cswap-guard'],
    });
    expect(screen.getByLabelText('Solo osservazione')).toBeChecked();
  });

  it('regole non valide: mostra i problemi', async () => {
    const backend = new FakeBackend({ rules: { enabled: true, primary: 6 } });
    const state = await backend.getEngineState();
    vi.spyOn(backend, 'getEngineState').mockResolvedValue({
      ...state,
      view: {
        holder: true,
        enabled: true,
        dryRun: false,
        suspended: 'invalidRules',
        detail: ['soglia 5h fuori da 1–100'],
      },
    });
    renderWith(<RulesPage />, backend);
    const status = await screen.findByRole('alert', { name: 'Stato del motore' });
    expect(status).toHaveTextContent('soglia 5h fuori da 1–100');
  });

  it('host senza le regole del motore: funzione non disponibile', async () => {
    renderWith(<RulesPage />, bareBackend(new FakeBackend()));
    expect(await screen.findByText(/non è disponibile in questo ambiente/)).toBeInTheDocument();
  });
});
