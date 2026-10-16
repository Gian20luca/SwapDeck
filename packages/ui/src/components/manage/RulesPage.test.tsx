import type { Conflict } from '@swapdeck/types';
import { fireEvent, screen, waitFor, within } from '@testing-library/react';
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

const rowNames = () =>
  within(screen.getByRole('list', { name: 'Utenze in ordine di priorità' }))
    .getAllByRole('listitem')
    .map((li) => li.querySelector('.sd-flow__name')?.firstChild?.textContent);

describe('RulesPage: flusso a lista', () => {
  it('mostra la lista della mappa e non permette di salvare finché non cambia nulla', async () => {
    await setup({ rules: { enabled: true, primary: 6, reserves: [1, 3] } });
    expect(screen.getByLabelText('Motore acceso')).toBeChecked();
    expect(screen.getByLabelText('Soglia sulle 5 ore (%)')).toHaveValue(95);
    expect(rowNames()).toEqual(['alias6', 'alias1', 'alias3', 'alias2', 'alias5']);
    // Le utenze non citate dalla mappa restano escluse, come fa il motore.
    expect(screen.getByRole('switch', { name: 'Includi alias6' })).toBeChecked();
    expect(screen.getByRole('switch', { name: 'Includi alias2' })).not.toBeChecked();
    expect(screen.getByRole('button', { name: 'Salva' })).toBeDisabled();
  });

  it('tastiera: Spazio prende, frecce spostano, Spazio lascia; la lista va in order', async () => {
    const { save, user } = await setup({ rules: { primary: 6, reserves: [1, 3] } });
    screen.getByRole('button', { name: 'Sposta alias3, posizione 3 di 5' }).focus();
    await user.keyboard(' ');
    expect(screen.getByText('alias3 presa, posizione 3 di 5.')).toBeInTheDocument();
    await user.keyboard('{ArrowUp}{ArrowUp}');
    expect(rowNames()[0]).toBe('alias3');
    expect(document.activeElement).toHaveAccessibleName('Sposta alias3, posizione 1 di 5');
    await user.keyboard(' ');
    expect(screen.getByText('alias3 lasciata in posizione 1 di 5.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    const sent = save.mock.calls[0]![0];
    expect(sent.order).toEqual([3, 6, 1, 2, 5]);
    expect(sent.excluded).toEqual([2, 5]);
    expect(sent.primary).toBe(3);
    expect(sent.reserves).toEqual([6, 1, 2, 5]);
  });

  it('tastiera: Esc annulla lo spostamento', async () => {
    const { user } = await setup({ rules: { primary: 6, reserves: [1, 3] } });
    screen.getByRole('button', { name: 'Sposta alias6, posizione 1 di 5' }).focus();
    await user.keyboard(' {ArrowDown}{ArrowDown}');
    expect(rowNames()[2]).toBe('alias6');
    await user.keyboard('{Escape}');
    expect(rowNames()[0]).toBe('alias6');
    expect(screen.getByRole('button', { name: 'Salva' })).toBeDisabled();
  });

  it('mouse: trascinare la maniglia sopra un’altra riga cambia l’ordine', async () => {
    await setup({ rules: { primary: 6, reserves: [1, 3] } });
    const list = screen.getByRole('list', { name: 'Utenze in ordine di priorità' });
    within(list)
      .getAllByRole('listitem')
      .forEach((li, i) => {
        li.getBoundingClientRect = () =>
          ({
            top: i * 40,
            height: 40,
            bottom: i * 40 + 40,
            left: 0,
            right: 300,
            width: 300,
          }) as DOMRect;
      });
    const handle = screen.getByRole('button', { name: 'Sposta alias1, posizione 2 di 5' });
    fireEvent.mouseDown(handle, { button: 0, clientY: 60 });
    fireEvent.mouseMove(window, { clientY: 5 });
    fireEvent.mouseUp(window);
    expect(rowNames().slice(0, 2)).toEqual(['alias1', 'alias6']);
    expect(screen.getByRole('button', { name: 'Salva' })).toBeEnabled();
  });

  it('includere un’utenza; escludere la prima blocca il salvataggio', async () => {
    const { save, user } = await setup({ rules: { primary: 6, reserves: [1] } });
    await user.click(screen.getByRole('switch', { name: 'Includi alias3' }));
    await user.click(screen.getByRole('switch', { name: 'Includi alias6' }));
    expect(
      screen.getByText('La prima utenza della lista deve essere inclusa.'),
    ).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Salva' })).toBeDisabled();
    await user.click(screen.getByRole('switch', { name: 'Includi alias6' }));
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]![0]).toMatchObject({ order: [6, 1, 2, 3, 5], excluded: [2, 5] });
  });

  it('nessun «Usa ora» nelle righe: solo maniglia, nome, % e interruttore', async () => {
    await setup({ rules: { primary: 6, reserves: [1, 3] } });
    const row = screen.getByTestId('flow-row-1');
    expect(within(row).getAllByRole('button')).toHaveLength(1);
    expect(within(row).getByRole('switch')).toBeInTheDocument();
    expect(within(row).getByText(/5h .* · 7d /)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Usa ora/ })).toBeNull();
  });

  it('«Adesso andrebbe a» dal candidato del motore', async () => {
    await setup({ rules: { enabled: true, primary: 6, reserves: [1, 3] } });
    expect(await screen.findByTestId('flow-next')).toHaveTextContent(
      /Adesso andrebbe a: alias\d — /,
    );
  });

  it('utenze citate ma inesistenti: avviso e «Togli dalla lista»', async () => {
    const { save, user } = await setup({ rules: { order: [6, 9, 1] } });
    expect(screen.getByText(/non esistono in cswap: #9/)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Togli dalla lista' }));
    expect(screen.queryByText(/non esistono in cswap/)).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]![0].order).toEqual([6, 1]);
  });

  it('criteri: 9 valori con descrizione, pesi e modello quando servono', async () => {
    const { save, user } = await setup({ rules: { primary: 6, reserves: [1] } });
    const criterion = screen.getByLabelText('Criterio di scelta della riserva');
    expect(within(criterion).getAllByRole('option')).toHaveLength(9);
    expect(
      screen.getByText('Sceglie l’utenza con il consumo settimanale più basso.'),
    ).toBeInTheDocument();
    await user.selectOptions(criterion, 'balanced-score');
    const pace = screen.getByLabelText('Ritmo');
    await user.clear(pace);
    await user.type(pace, '0');
    await user.selectOptions(criterion, 'model-headroom');
    expect(screen.getByText('Indica il modello.')).toBeInTheDocument();
    await user.type(screen.getByLabelText('Modello'), 'Fable');
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0]![0]).toMatchObject({
      reserveCriterion: 'model-headroom',
      headroomModel: 'Fable',
      scoreWeights: { fiveHour: 0.4, sevenDay: 0.3, reset: 0.15, pace: 0 },
    });
  });

  it('modifica di motore, osservazione, soglie, isteresi e pausa', async () => {
    const { save, user } = await setup({ rules: { primary: 6, reserves: [1, 3] } });
    await user.click(screen.getByLabelText('Motore acceso'));
    await user.click(screen.getByLabelText('Solo osservazione'));
    const five = screen.getByLabelText('Soglia sulle 5 ore (%)');
    await user.clear(five);
    await user.type(five, '90');
    await user.clear(screen.getByLabelText('Soglia sui 7 giorni (%)'));
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
      returnHysteresis: 5,
      minSwitchGapSecs: 600,
    });
    expect(await screen.findByText('Regole salvate.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Salva' })).toBeDisabled();
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

describe('RulesPage: mappe', () => {
  it('cambia mappa: regole della nuova mappa e nome nell’esito', async () => {
    const { user } = await setup({ rules: { primary: 6, reserves: [1, 3] } });
    const select = screen.getByLabelText('Mappa attiva');
    expect(within(select).getAllByRole('option')).toHaveLength(5);
    await user.selectOptions(select, 'eco');
    expect(await screen.findByText('Mappa attiva: Eco.')).toBeInTheDocument();
    expect(screen.getByLabelText('Mappa attiva')).toHaveValue('eco');
    expect(screen.getByLabelText('Soglia sulle 5 ore (%)')).toHaveValue(90);
    expect(screen.getByLabelText('Criterio di scelta della riserva')).toHaveValue('reset-soonest');
  });

  it('con modifiche non salvate il cambio di mappa chiede conferma', async () => {
    const { user } = await setup({ rules: { primary: 6 } });
    await user.click(screen.getByLabelText('Solo osservazione'));
    await user.selectOptions(screen.getByLabelText('Mappa attiva'), 'sport');
    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: 'Mappa' }));
    expect(await screen.findByText('Mappa attiva: Sport.')).toBeInTheDocument();
  });

  it('crea, rinomina, duplica ed elimina con conferma', async () => {
    const { backend, user } = await setup({ rules: { primary: 6 } });
    await user.click(screen.getByRole('button', { name: 'Nuova' }));
    await user.type(screen.getByLabelText('Nome della mappa'), 'Notte{Enter}');
    expect(await screen.findByText('Mappa «Notte» creata.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Rinomina' }));
    const name = screen.getByLabelText('Nome della mappa');
    await user.clear(name);
    await user.type(name, 'Lavoro{Enter}');
    expect(await screen.findByText('Mappa rinominata in «Lavoro».')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Duplica' }));
    await user.click(screen.getByRole('button', { name: 'Conferma' }));
    expect(await screen.findByText('Copia creata.')).toBeInTheDocument();
    let profiles = await backend.getEngineProfiles();
    expect(profiles.profiles.map((p) => p.name)).toContain('Lavoro (copia)');
    expect(profiles.profiles).toHaveLength(7);
    await user.click(screen.getByRole('button', { name: 'Elimina' }));
    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: 'Elimina' }));
    expect(await screen.findByText('Mappa eliminata.')).toBeInTheDocument();
    profiles = await backend.getEngineProfiles();
    expect(profiles.profiles).toHaveLength(6);
  });

  it('nome vuoto: errore e nessuna chiamata', async () => {
    const { backend, user } = await setup();
    const apply = vi.spyOn(backend, 'applyEngineProfile');
    await user.click(screen.getByRole('button', { name: 'Nuova' }));
    await user.click(screen.getByRole('button', { name: 'Conferma' }));
    expect(screen.getByText('Scrivi un nome da 1 a 60 caratteri.')).toBeInTheDocument();
    expect(apply).not.toHaveBeenCalled();
  });

  it('ripristina il preset (solo per le mappe da preset), con conferma', async () => {
    const { backend, user } = await setup();
    expect(screen.queryByRole('button', { name: 'Ripristina preset' })).toBeNull();
    await user.selectOptions(screen.getByLabelText('Mappa attiva'), 'eco');
    await screen.findByText('Mappa attiva: Eco.');
    const five = screen.getByLabelText('Soglia sulle 5 ore (%)');
    await user.clear(five);
    await user.type(five, '50');
    await user.click(screen.getByRole('button', { name: 'Salva' }));
    await screen.findByText('Regole salvate.');
    await user.click(screen.getByRole('button', { name: 'Ripristina preset' }));
    const dialog = await screen.findByRole('alertdialog');
    await user.click(within(dialog).getByRole('button', { name: 'Ripristina preset' }));
    expect(await screen.findByText('Preset ripristinato.')).toBeInTheDocument();
    expect(screen.getByLabelText('Soglia sulle 5 ore (%)')).toHaveValue(90);
    expect((await backend.getEngineState()).rules.fiveHourThreshold).toBe(90);
  });

  it('l’ultima mappa non si elimina', async () => {
    const backend = new FakeBackend();
    const rules = (await backend.getEngineState()).rules;
    vi.spyOn(backend, 'getEngineProfiles').mockResolvedValue({
      activeId: 'personal',
      profiles: [{ id: 'personal', name: 'Personale', rules }],
    });
    renderWith(<RulesPage />, backend);
    expect(await screen.findByRole('button', { name: 'Elimina' })).toBeDisabled();
  });
});

describe('RulesPage: stato sospeso e conflitti', () => {
  it('motore spento: riga neutra (non rossa) con «Accendi»', async () => {
    await setup();
    expect(screen.queryByRole('alert')).toBeNull();
    const row = screen.getByRole('status', { name: 'Stato del motore' });
    expect(row).toHaveTextContent('Motore spento: nessun cambio automatico');
    expect(row).not.toHaveTextContent('Motore sospeso');
    expect(row).not.toHaveAttribute('data-tone');
    expect(within(row).getByRole('button', { name: 'Accendi' })).toBeEnabled();
  });

  it('«Accendi» salva le regole con il motore acceso', async () => {
    const { save, user } = await setup({ rules: { enabled: false, primary: 6, reserves: [1, 3] } });
    await user.click(screen.getByRole('button', { name: 'Accendi' }));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1));
    expect(save.mock.calls[0]![0].enabled).toBe(true);
    await waitFor(() =>
      expect(screen.getByRole('status', { name: 'Stato del motore' })).toHaveTextContent(
        'Motore acceso',
      ),
    );
    expect(screen.queryByRole('button', { name: 'Accendi' })).toBeNull();
    expect(screen.getByLabelText('Motore acceso')).toBeChecked();
    expect(screen.getByText('Motore acceso.', { exact: true })).toBeInTheDocument();
  });

  it('acceso: nessuna riga di osservazione né rossa', async () => {
    await setup({ rules: { enabled: true, primary: 6, reserves: [1, 3] } });
    expect(screen.getByRole('status', { name: 'Stato del motore' })).toHaveTextContent(
      'Motore acceso',
    );
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByTestId('engine-observe')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Accendi' })).toBeNull();
  });

  it('sola osservazione: riga informativa propria, senza rosso', async () => {
    await setup({ rules: { enabled: true, dryRun: true, primary: 6, reserves: [1, 3] } });
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.getByTestId('engine-observe')).toHaveTextContent(
      'In sola osservazione: registra le decisioni senza cambiare utenza',
    );
    expect(screen.getByRole('status', { name: 'Stato del motore' })).toHaveTextContent(
      'Motore acceso',
    );
  });

  it('sospeso per conflitto: elenca il conflitto e permette di ignorarlo', async () => {
    const { save, user } = await setup({
      rules: { enabled: true, primary: 6, reserves: [1, 3] },
      conflicts: [GUARD, TUI],
    });
    const status = screen.getByRole('alert', { name: 'Stato del motore' });
    expect(status).toHaveTextContent('Motore sospeso.');
    expect(screen.queryByRole('button', { name: 'Accendi' })).toBeNull();
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

describe('RulesPage: avvisi dalla lista di cswap', () => {
  it('modello sconosciuto: avviso nell’editor della mappa', async () => {
    const backend = new FakeBackend({ rules: { primary: 6, reserveCriterion: 'model-headroom' } });
    const state = await backend.getEngineState();
    vi.spyOn(backend, 'getEngineState').mockResolvedValue({
      ...state,
      rules: { ...state.rules, headroomModel: 'Ghost' },
      warnings: ['modello «Ghost» sconosciuto: nessuna utenza lo riporta'],
    });
    renderWith(<RulesPage />, backend);
    expect(await screen.findByText(/«Ghost» sconosciuto/)).toBeInTheDocument();
  });
});
