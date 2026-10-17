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
import { SettingsPage } from './manage/SettingsPage.js';
import { ToolsPage } from './manage/ToolsPage.js';

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
  it('ogni chiave di configurazione di cswap ha la sua spiegazione', () => {
    for (const [name, info] of Object.entries(CONFIG_KEYS)) {
      expect(itDict[info.info], name).toBeTruthy();
    }
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
});
