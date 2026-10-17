import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend } from '../dev/FakeBackend.js';
import { renderWith } from '../dev/testUtils.js';
import { en } from '../i18n/en.js';
import { it as itDict } from '../i18n/it.js';
import { CONFIG_KEYS } from '../lib/cswapConfig.js';
import { Dashboard } from './Dashboard.js';
import { FoldersPage } from './manage/FoldersPage.js';
import { HistoryPage } from './manage/HistoryPage.js';
import { NotificationsPage } from './manage/NotificationsPage.js';
import { RulesPage } from './manage/RulesPage.js';
import { SettingsPage } from './manage/SettingsPage.js';
import { ToolsPage } from './manage/ToolsPage.js';
import { UsagePage } from './manage/UsagePage.js';

vi.mock('uplot', () => ({
  default: class {
    setSize() {}
    destroy() {}
  },
}));

const infoButtons = (root: HTMLElement | Document = document) =>
  within(root as HTMLElement).queryAllByRole('button', { name: /^Informazioni su / });

describe('testi delle «i»', () => {
  it('ogni chiave info.* esiste in italiano e in inglese, breve e senza segnaposto', () => {
    const keys = Object.keys(itDict).filter((k) => k.startsWith('info.'));
    expect(keys.length).toBeGreaterThan(30);
    for (const key of keys as (keyof typeof itDict)[]) {
      expect(en[key], key).toBeTruthy();
      if (key !== 'info.label') expect(itDict[key].length, key).toBeLessThan(320);
    }
  });
  it('ogni chiave di configurazione di cswap ha la sua spiegazione, una sola', () => {
    for (const [name, info] of Object.entries(CONFIG_KEYS)) {
      const key = info.info ?? info.hint;
      expect(key && itDict[key], name).toBeTruthy();
    }
    // Il tema ha la spiegazione visibile sotto il campo: niente «i» con lo stesso testo.
    expect(CONFIG_KEYS['ui.theme']?.info).toBeUndefined();
    expect(CONFIG_KEYS['ui.theme']?.hint).toBe('config.key.theme.hint');
  });
});

describe('«i» nelle pagine', () => {
  it('Account: barre 5h e 7d, ritmo e titolo', async () => {
    renderWith(<Dashboard />, new FakeBackend());
    await screen.findByRole('list', { name: 'Account' });
    const names = infoButtons().map((b) => b.getAttribute('aria-label'));
    expect(names).toContain('Informazioni su Account');
    expect(names).toContain('Informazioni su 5 ore');
    expect(names).toContain('Informazioni su 7 giorni');
    expect(names).toContain('Informazioni su Ritmo');
  });

  it('Impostazioni: lingua, tema, intervallo, avvio e ogni chiave di cswap; testo al clic', async () => {
    const backend = new FakeBackend();
    (backend as { getNotificationSettings?: unknown }).getNotificationSettings = undefined;
    renderWith(<SettingsPage />, backend);
    await screen.findByRole('list', { name: 'Configurazione di cswap' });
    const names = infoButtons().map((b) => b.getAttribute('aria-label'));
    for (const name of [
      'Lingua',
      'Tema',
      'Intervallo di controllo (secondi)',
      'Soglie delle notifiche (%)',
      'Strategia',
      'Isteresi (%)',
      'Pausa tra i cambi (secondi)',
      'Configurazione di cswap',
    ])
      expect(names, name).toContain(`Informazioni su ${name}`);
    const user = userEvent.setup();
    const list = screen.getByRole('list', { name: 'Configurazione di cswap' });
    await user.click(within(list).getByRole('button', { name: 'Informazioni su Isteresi (%)' }));
    expect(screen.getByText(/evita i cambi avanti e indietro/)).toBeVisible();
  });

  it('Notifiche: gruppi e avvisi principali', async () => {
    renderWith(<NotificationsPage onBack={() => undefined} />, new FakeBackend());
    await screen.findByText('Cambi di utenza');
    const names = infoButtons().map((b) => b.getAttribute('aria-label'));
    for (const name of ['Cambi di utenza', 'Consumi e reset', 'Non disturbare', 'Riepilogo'])
      expect(names, name).toContain(`Informazioni su ${name}`);
    expect(names).toContain('Informazioni su Nessuna riserva');
  });

  it('Cartelle, Storico e Strumenti', async () => {
    renderWith(<FoldersPage />, new FakeBackend());
    await screen.findByText('Mappature');
    expect(infoButtons().map((b) => b.getAttribute('aria-label'))).toContain(
      'Informazioni su Mappature',
    );
  });

  it('Storico', async () => {
    renderWith(<HistoryPage />, new FakeBackend());
    await screen.findByText('Grafici per account');
    const names = infoButtons().map((b) => b.getAttribute('aria-label'));
    expect(names).toContain('Informazioni su Intervallo');
    expect(names).toContain('Informazioni su Grafici per account');
    expect(names).toContain('Informazioni su Cambi e decisioni');
  });

  it('Strumenti', async () => {
    renderWith(<ToolsPage />, new FakeBackend());
    await screen.findByText('Esporta');
    const names = infoButtons().map((b) => b.getAttribute('aria-label'));
    for (const name of ['Esporta', 'Importa', 'Credenziali parcheggiate'])
      expect(names, name).toContain(`Informazioni su ${name}`);
  });

  it('Regole: stato, mappa, riquadro, lista, soglie, criterio, pesi, adesso andrebbe a, altri motori', async () => {
    const backend = new FakeBackend({
      rules: { enabled: true, primary: 6, reserves: [1, 3], reserveCriterion: 'balanced-score' },
      conflicts: [{ id: 'process:tui', kind: 'process', label: 'cswap tui', certain: false }],
    });
    renderWith(<RulesPage />, backend);
    await screen.findByTestId('flow-next');
    const names = infoButtons().map((b) => b.getAttribute('aria-label'));
    for (const name of [
      'Stato del motore',
      'Mappa attiva',
      'Regole della mappa «Personale»',
      'Lista delle utenze',
      'Soglie e criteri',
      'Criterio di scelta della riserva',
      'Pesi del punteggio',
      'Adesso andrebbe a',
      'Altri motori rilevati',
    ])
      expect(names, name).toContain(`Informazioni su ${name}`);
    // Il gruppo dei pesi ha il nome pulito, senza il testo della «i».
    expect(screen.getByRole('group', { name: 'Pesi del punteggio' })).toBeInTheDocument();
    // Niente «i» sui pulsanti: hanno il tooltip.
    for (const b of infoButtons()) expect(b.closest('.sd-btn')).toBeNull();
  });

  it('Regole: preset in sola lettura e modello', async () => {
    const backend = new FakeBackend({
      rules: {
        primary: 6,
        reserves: [1, 3],
        reserveCriterion: 'model-headroom',
        headroomModel: 'Fable',
      },
    });
    renderWith(<RulesPage />, backend);
    await screen.findByLabelText('Modello');
    expect(screen.getByRole('button', { name: 'Informazioni su Modello' })).toBeInTheDocument();
    await userEvent.setup().selectOptions(screen.getByLabelText('Mappa attiva'), 'sport');
    await screen.findByTestId('map-readonly');
    expect(screen.getByRole('button', { name: 'Informazioni su preset' })).toBeInTheDocument();
  });

  it('Chi consuma: cosa misura, altri dispositivi, privacy', async () => {
    renderWith(<UsagePage />, new FakeBackend());
    await screen.findByRole('list', { name: 'Classifica dei consumi' });
    const names = infoButtons().map((b) => b.getAttribute('aria-label'));
    for (const name of ['Chi consuma', 'Altri dispositivi (stima)', 'Privacy'])
      expect(names, name).toContain(`Informazioni su ${name}`);
  });
});
