import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { FakeBackend } from '../../dev/FakeBackend.js';
import { bareBackend, renderWith } from '../../dev/testUtils.js';
import { translate, type Translate } from '../../i18n/index.js';
import { decisionText, HistoryPage } from './HistoryPage.js';

vi.mock('uplot', () => ({
  default: class {
    setSize() {}
    destroy() {}
  },
}));

async function setup() {
  const backend = new FakeBackend();
  const history = vi.spyOn(backend, 'getHistory');
  const events = vi.spyOn(backend, 'getHistoryEvents');
  renderWith(<HistoryPage />, backend);
  await screen.findByRole('list', { name: 'Grafici per account' });
  return { backend, history, events, user: userEvent.setup() };
}

describe('HistoryPage', () => {
  it('mostra un grafico per account, aperto solo sull’attivo', async () => {
    const { history } = await setup();
    const list = screen.getByRole('list', { name: 'Grafici per account' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(5);
    await waitFor(() => expect(history).toHaveBeenCalled());
    expect(history.mock.calls[0]![0]).toMatchObject({ account: 6 });
    expect(screen.getByRole('button', { name: 'Nascondi lo storico di alias6' })).toHaveAttribute(
      'aria-expanded',
      'true',
    );
    expect(screen.getByRole('button', { name: 'Mostra lo storico di alias1' })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
  });

  it('apre e chiude i grafici di un altro account', async () => {
    const { history, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Mostra lo storico di alias1' }));
    await waitFor(() =>
      expect(history.mock.calls.some(([q]) => (q as { account?: number }).account === 1)).toBe(
        true,
      ),
    );
    await user.click(screen.getByRole('button', { name: 'Nascondi lo storico di alias1' }));
    expect(screen.getByRole('button', { name: 'Mostra lo storico di alias1' })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
  });

  it('la scelta dell’intervallo rilegge storico ed eventi', async () => {
    const { history, events, user } = await setup();
    expect(screen.getByRole('button', { name: '7 giorni' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    history.mockClear();
    events.mockClear();
    await user.click(screen.getByRole('button', { name: '24 ore' }));
    await waitFor(() => expect(history).toHaveBeenCalled());
    await waitFor(() => expect(events).toHaveBeenCalled());
    const since = (history.mock.calls.at(-1)![0] as { sinceSeconds: number }).sinceSeconds;
    expect(Math.abs(Date.now() / 1000 - 24 * 3600 - since)).toBeLessThan(10);
    expect(screen.getByRole('button', { name: '24 ore' })).toHaveAttribute('aria-pressed', 'true');
    history.mockClear();
    await user.click(screen.getByRole('button', { name: '30 giorni' }));
    await waitFor(() => expect(history).toHaveBeenCalled());
    const since30 = (history.mock.calls.at(-1)![0] as { sinceSeconds: number }).sinceSeconds;
    expect(Math.abs(Date.now() / 1000 - 720 * 3600 - since30)).toBeLessThan(10);
  });

  it('elenca cambi di account e decisioni del motore', async () => {
    await setup();
    const list = await screen.findByRole('list', { name: 'Cambi e decisioni' });
    const items = within(list).getAllByRole('listitem');
    expect(items.length).toBeGreaterThan(3);
    expect(within(list).getAllByText('Cambio').length).toBeGreaterThan(0);
    expect(within(list).getAllByText('Decisione').length).toBeGreaterThan(0);
    expect(list).toHaveTextContent('principale al limite');
    expect(list).toHaveTextContent('riuscito');
  });

  it('«Mostra altri» allunga l’elenco', async () => {
    const { user } = await setup();
    await user.click(screen.getByRole('button', { name: '30 giorni' }));
    const more = await screen.findByRole('button', { name: /^Mostra altri \d+$/ });
    const list = screen.getByRole('list', { name: 'Cambi e decisioni' });
    const before = within(list).getAllByRole('listitem').length;
    await user.click(more);
    expect(within(list).getAllByRole('listitem').length).toBeGreaterThan(before);
  });

  it('senza eventi: stato vuoto', async () => {
    const backend = new FakeBackend();
    vi.spyOn(backend, 'getHistoryEvents').mockResolvedValue({ switches: [], decisions: [] });
    renderWith(<HistoryPage />, backend);
    expect(
      await screen.findByText('Nessun cambio né decisione nell’intervallo.'),
    ).toBeInTheDocument();
  });

  it('errore di lettura degli eventi con «Riprova»', async () => {
    const backend = new FakeBackend();
    vi.spyOn(backend, 'getHistoryEvents').mockRejectedValueOnce(new Error('storico illeggibile'));
    renderWith(<HistoryPage />, backend);
    expect(await screen.findByText('storico illeggibile')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Riprova' }));
    expect(await screen.findByRole('list', { name: 'Cambi e decisioni' })).toBeInTheDocument();
  });

  it('host senza elenco degli eventi: restano i soli grafici', async () => {
    renderWith(<HistoryPage />, bareBackend(new FakeBackend()));
    await screen.findByRole('list', { name: 'Grafici per account' });
    expect(screen.queryByText('Cambi e decisioni')).toBeNull();
  });
});

describe('decisionText', () => {
  const tr: Translate = (key, params) => translate('it', key, params);
  it('descrive le quattro decisioni', () => {
    expect(decisionText({ kind: 'noAction', reason: 'cooldown' }, tr)).toBe(
      'Nessuna azione: pausa tra i cambi',
    );
    expect(decisionText({ kind: 'switch', from: 6, to: 1, cause: 'primaryAtLimit' }, tr)).toBe(
      'Cambio da 6 a 1: principale al limite',
    );
    expect(
      decisionText({ kind: 'returnToPrimary', from: 1, to: 6, cause: 'primaryReset' }, tr),
    ).toBe('Ritorno da 1 a 6: il principale si è azzerato');
    expect(decisionText({ kind: 'suspended', reason: 'conflict', detail: ['task:x'] }, tr)).toBe(
      'Motore sospeso: altro motore attivo',
    );
  });
});

describe('HistoryPage: intestazione compatta', () => {
  it('nome, badge e «5h · 7d» con i reset nel tooltip, senza la scheda completa', async () => {
    renderWith(<HistoryPage />, new FakeBackend());
    const summary = await screen.findByTestId('summary-6');
    expect(within(summary).getByText('alias6')).toBeInTheDocument();
    expect(within(summary).getByText('Attivo')).toBeInTheDocument();
    const five = within(summary).getByText(/^5h [0-9]+%$/);
    expect(five).toHaveAccessibleDescription(/Reset tra/);
    expect(within(summary).getByText(/^7d [0-9]+%$/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Passa a questo account/ })).toBeNull();
  });
});
