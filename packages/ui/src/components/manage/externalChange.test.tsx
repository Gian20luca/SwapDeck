import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { actions } from '../../backend/actions.js';
import { FakeBackend } from '../../dev/FakeBackend.js';
import { renderWith } from '../../dev/testUtils.js';
import { FoldersPage } from './FoldersPage.js';
import { RulesPage } from './RulesPage.js';
import { SettingsPage } from './SettingsPage.js';

const NOTICE = /Modificato in un’altra finestra/;

describe('schede aperte e modifiche fatte in un’altra finestra', () => {
  it('Impostazioni: senza modifiche il form si aggiorna da solo', async () => {
    const backend = new FakeBackend();
    renderWith(<SettingsPage />, backend);
    const interval = await screen.findByLabelText('Intervallo di controllo (secondi)');
    expect(interval).toHaveValue(60);
    act(() => backend.changeElsewhere({ preferences: { pollIntervalSeconds: 300 } }));
    await waitFor(() => expect(interval).toHaveValue(300));
    expect(screen.queryByText(NOTICE)).not.toBeInTheDocument();
  });

  it('Impostazioni: con modifiche in corso resta il form e compare «Ricarica»', async () => {
    const backend = new FakeBackend();
    const user = userEvent.setup();
    renderWith(<SettingsPage />, backend);
    const interval = await screen.findByLabelText('Intervallo di controllo (secondi)');
    await user.clear(interval);
    await user.type(interval, '120');
    act(() => backend.changeElsewhere({ preferences: { pollIntervalSeconds: 300 } }));
    expect(await screen.findByText(NOTICE)).toBeInTheDocument();
    expect(interval).toHaveValue(120);
    await user.click(screen.getByRole('button', { name: 'Ricarica' }));
    expect(interval).toHaveValue(300);
    expect(screen.queryByText(NOTICE)).not.toBeInTheDocument();
  });

  it('Impostazioni: un salvataggio da qui non conta come modifica esterna', async () => {
    const backend = new FakeBackend();
    const user = userEvent.setup();
    renderWith(<SettingsPage />, backend);
    const interval = await screen.findByLabelText('Intervallo di controllo (secondi)');
    await user.clear(interval);
    await user.type(interval, '180');
    const form = interval.closest('form')!;
    await user.click(within(form).getByRole('button', { name: 'Salva' }));
    await waitFor(() => expect(within(form).getByRole('button', { name: 'Salva' })).toBeDisabled());
    act(() => backend.changeElsewhere({}));
    await waitFor(() => expect(interval).toHaveValue(180));
    expect(screen.queryByText(NOTICE)).not.toBeInTheDocument();
  });

  it('configurazione di cswap: valore cambiato altrove, riga ferma o in modifica', async () => {
    const backend = new FakeBackend();
    const user = userEvent.setup();
    renderWith(<SettingsPage />, backend);
    await user.click(await screen.findByRole('button', { name: 'Avanzate' }));
    await screen.findByRole('list', { name: 'Impostazioni di cswap auto' });
    const row = () =>
      screen
        .getByRole('button', { name: 'Salva autoswitch.cooldownSeconds' })
        .closest('li') as HTMLElement;
    const field = () => within(row()).getByRole('spinbutton');
    await act(() => backend.runAction(actions.configSet('autoswitch.cooldownSeconds', '900')));
    act(() => backend.changeElsewhere({}));
    await waitFor(() => expect(field()).toHaveValue(900));
    await user.clear(field());
    await user.type(field(), '1200');
    await act(() => backend.runAction(actions.configSet('autoswitch.cooldownSeconds', '600')));
    act(() => backend.changeElsewhere({}));
    expect(await within(row()).findByText(NOTICE)).toBeInTheDocument();
    expect(field()).toHaveValue(1200);
    await user.click(within(row()).getByRole('button', { name: 'Ricarica' }));
    expect(field()).toHaveValue(600);
  });

  it('Notifiche: si aggiorna da sola; con modifiche avvisa', async () => {
    const backend = new FakeBackend();
    const user = userEvent.setup();
    renderWith(<SettingsPage />, backend);
    await user.click(await screen.findByRole('button', { name: 'Notifiche…' }));
    const reset = await screen.findByLabelText('Reset imminente');
    expect(reset).not.toBeChecked();
    act(() => backend.changeElsewhere({ notifications: { resetSoon: { enabled: true } } }));
    await waitFor(() => expect(reset).toBeChecked());
    expect(screen.queryByText(NOTICE)).not.toBeInTheDocument();

    await user.click(screen.getByLabelText('Reset avvenuto'));
    act(() => backend.changeElsewhere({ notifications: { resetSoon: { enabled: false } } }));
    expect(await screen.findByText(NOTICE)).toBeInTheDocument();
    expect(reset).toBeChecked();
    expect(screen.getByLabelText('Reset avvenuto')).toBeChecked();
    await user.click(screen.getByRole('button', { name: 'Ricarica' }));
    expect(reset).not.toBeChecked();
    expect(screen.getByLabelText('Reset avvenuto')).not.toBeChecked();
  });

  it('Cartelle: l’elenco si rilegge', async () => {
    const backend = new FakeBackend({ mappings: [] });
    renderWith(<FoldersPage />, backend);
    expect(await screen.findByText('Nessuna cartella mappata.')).toBeInTheDocument();
    await act(() => backend.runAction(actions.map(1, '~/nuovo')));
    act(() => backend.changeElsewhere({}));
    const list = await screen.findByRole('list', { name: 'Mappature' });
    expect(within(list).getByText('~/nuovo')).toBeInTheDocument();
  });

  it('Regole: con la bozza modificata avvisa invece di sovrascriverla', async () => {
    const backend = new FakeBackend({ rules: { primary: 6, reserves: [1, 3] } });
    const user = userEvent.setup();
    renderWith(<RulesPage />, backend);
    const five = () => screen.getByLabelText('Soglia sulle 5 ore (%)');
    await screen.findByLabelText('Soglia sulle 5 ore (%)');
    act(() => backend.changeElsewhere({ rules: { fiveHourThreshold: 90 } }));
    await waitFor(() => expect(five()).toHaveValue(90));
    expect(screen.queryByText(NOTICE)).not.toBeInTheDocument();

    await user.clear(five());
    await user.type(five(), '85');
    act(() => backend.changeElsewhere({ rules: { fiveHourThreshold: 80 } }));
    expect(await screen.findByText(NOTICE)).toBeInTheDocument();
    expect(five()).toHaveValue(85);
    await user.click(screen.getByRole('button', { name: 'Ricarica' }));
    expect(five()).toHaveValue(80);
  });
});
