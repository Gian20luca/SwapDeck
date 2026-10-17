import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { BackendError } from '../backend/types.js';
import { FakeBackend } from '../dev/FakeBackend.js';
import { bareBackend, renderWith } from '../dev/testUtils.js';
import { SwapDeckApp } from './SwapDeckApp.js';

vi.mock('uplot', () => ({
  default: class {
    setSize() {}
    destroy() {}
  },
}));

function setup(initialTab?: 'accounts' | 'tools') {
  const backend = new FakeBackend({ mappings: [{ path: 'C:\\x', account: 1, email: '' }] });
  renderWith(
    <SwapDeckApp {...(initialTab ? { initialTab } : {})} header={<p>intestazione</p>} />,
    backend,
  );
  return userEvent.setup();
}

const TABS = [
  'Account',
  'Cartelle',
  'Regole',
  'Storico',
  'Chi consuma',
  'Strumenti',
  'Impostazioni',
];

describe('SwapDeckApp', () => {
  it('mostra le sette schede e parte da Account', async () => {
    setup();
    const tabs = screen.getAllByRole('tab');
    expect(tabs.map((t) => t.textContent)).toEqual(TABS);
    expect(screen.getByRole('tab', { name: 'Account' })).toHaveAttribute('aria-selected', 'true');
    expect(await screen.findByRole('heading', { level: 1, name: 'Account' })).toBeInTheDocument();
    expect(screen.getByText('intestazione')).toBeInTheDocument();
    expect(screen.getByRole('tabpanel')).toHaveAttribute(
      'aria-labelledby',
      screen.getByRole('tab', { name: 'Account' }).id,
    );
  });

  it('segnala la scheda aperta (stato ricordato dal webview di VS Code)', async () => {
    const onTabChange = vi.fn();
    renderWith(
      <SwapDeckApp initialTab="tools" onTabChange={onTabChange} />,
      new FakeBackend({ mappings: [] }),
    );
    expect(onTabChange).toHaveBeenLastCalledWith('tools');
    await userEvent.setup().click(screen.getByRole('tab', { name: 'Cartelle' }));
    expect(onTabChange).toHaveBeenLastCalledWith('folders');
  });

  it('naviga fra le schede con il clic', async () => {
    const user = setup();
    await user.click(screen.getByRole('tab', { name: 'Cartelle' }));
    expect(await screen.findByRole('heading', { name: 'Cartelle mappate' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Regole' }));
    expect(await screen.findByRole('heading', { name: 'Regole del motore' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Storico' }));
    expect(await screen.findByRole('heading', { name: 'Storico' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Strumenti' }));
    expect(await screen.findByRole('heading', { name: 'Strumenti' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Impostazioni' }));
    expect(await screen.findByRole('heading', { name: 'Impostazioni' })).toBeInTheDocument();
    await user.click(screen.getByRole('tab', { name: 'Account' }));
    expect(await screen.findByRole('heading', { level: 1, name: 'Account' })).toBeInTheDocument();
  });

  it('frecce, Home e Fine spostano la scheda attiva e il focus', async () => {
    const user = setup();
    screen.getByRole('tab', { name: 'Account' }).focus();
    await user.keyboard('{ArrowRight}');
    expect(screen.getByRole('tab', { name: 'Cartelle' })).toHaveFocus();
    expect(screen.getByRole('tab', { name: 'Cartelle' })).toHaveAttribute('aria-selected', 'true');
    await user.keyboard('{ArrowLeft}{ArrowLeft}');
    expect(screen.getByRole('tab', { name: 'Impostazioni' })).toHaveFocus();
    await user.keyboard('{Home}');
    expect(screen.getByRole('tab', { name: 'Account' })).toHaveFocus();
    await user.keyboard('{End}');
    expect(screen.getByRole('tab', { name: 'Impostazioni' })).toHaveFocus();
    // Solo la scheda attiva è raggiungibile con Tab.
    expect(screen.getByRole('tab', { name: 'Account' })).toHaveAttribute('tabindex', '-1');
  });

  it('può partire da un’altra scheda', async () => {
    setup('tools');
    expect(await screen.findByRole('heading', { name: 'Strumenti' })).toBeInTheDocument();
  });
});

describe('SwapDeckApp: Storico dalla scheda di un account', () => {
  async function openHistoryOf(name: string, backend = new FakeBackend()) {
    renderWith(<SwapDeckApp />, backend);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: `Apri lo storico di ${name}` }));
    return user;
  }

  it('il clic su «Storico» passa alla scheda Storico con quell’account aperto e col focus', async () => {
    await openHistoryOf('alias3');
    expect(screen.getByRole('tab', { name: 'Storico' })).toHaveAttribute('aria-selected', 'true');
    expect(await screen.findByRole('heading', { name: 'Storico' })).toBeInTheDocument();
    const toggle = await screen.findByRole('button', { name: 'Nascondi lo storico di alias3' });
    expect(toggle).toHaveAttribute('aria-expanded', 'true');
    await waitFor(() => expect(toggle).toHaveFocus());
    // Gli altri account restano chiusi, anche l'attivo.
    expect(screen.getByRole('button', { name: 'Mostra lo storico di alias6' })).toHaveAttribute(
      'aria-expanded',
      'false',
    );
  });

  it('nella scheda Account non c’è più lo storico in linea', async () => {
    renderWith(<SwapDeckApp />, new FakeBackend());
    await screen.findByRole('button', { name: 'Apri lo storico di alias6' });
    expect(screen.queryByRole('button', { name: /^Mostra lo storico/ })).toBeNull();
    expect(screen.queryByRole('button', { name: /^Nascondi lo storico/ })).toBeNull();
  });

  it('storico: errore dal backend', async () => {
    const backend = new FakeBackend();
    vi.spyOn(backend, 'getHistory').mockRejectedValue(
      new BackendError({ kind: 'failed', message: 'file illeggibile' }),
    );
    await openHistoryOf('alias6', backend);
    expect(await screen.findByText(/Impossibile leggere lo storico/)).toBeInTheDocument();
  });

  it('storico: vuoto', async () => {
    const backend = new FakeBackend();
    vi.spyOn(backend, 'getHistory').mockResolvedValue([]);
    await openHistoryOf('alias6', backend);
    expect((await screen.findAllByText('Nessun dato nello storico.')).length).toBeGreaterThan(0);
  });
});

describe('SwapDeckApp: Chi consuma questa utenza', () => {
  it('dalla scheda di un account apre «Chi consuma» già filtrata su quell’utenza', async () => {
    const backend = new FakeBackend();
    const spy = vi.spyOn(backend, 'getUsageBreakdown');
    renderWith(<SwapDeckApp />, backend);
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole('button', { name: 'Chi consuma questa utenza: alias3' }),
    );
    expect(screen.getByRole('tab', { name: 'Chi consuma' })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    expect(await screen.findByRole('heading', { name: 'Chi consuma' })).toBeInTheDocument();
    expect(await screen.findByText('Nessun consumo registrato in questa finestra.')).toBeVisible();
    expect(screen.getByLabelText('Utenza')).toHaveValue('3');
    expect(spy).toHaveBeenLastCalledWith({ window: '5h', groupBy: 'project', account: 3 });
  });

  it('dallo Storico: il link dentro il pannello dell’utenza apre la scheda filtrata', async () => {
    renderWith(<SwapDeckApp initialTab="history" />, new FakeBackend());
    const user = userEvent.setup();
    await screen.findByRole('list', { name: 'Grafici per account' });
    await user.click(
      await screen.findByRole('button', { name: 'Chi consuma questa utenza: alias6' }),
    );
    expect(await screen.findByText('Nessun consumo registrato in questa finestra.')).toBeVisible();
    expect(screen.getByLabelText('Utenza')).toHaveValue('6');
  });

  it('un host che non sa leggere i consumi non mostra scheda né link', async () => {
    renderWith(<SwapDeckApp />, bareBackend(new FakeBackend()));
    await screen.findByRole('list', { name: 'Account' });
    expect(screen.queryByRole('tab', { name: 'Chi consuma' })).toBeNull();
    expect(screen.queryByRole('button', { name: /Chi consuma/ })).toBeNull();
    expect(screen.getAllByRole('tab')).toHaveLength(6);
  });
});

describe('SwapDeckApp: primo avvio e aggiornamento di cswap', () => {
  it('cswap mancante: la procedura prende il posto delle schede, dopo l’installazione le schede', async () => {
    const user = userEvent.setup();
    const backend = new FakeBackend({ setup: { state: 'missing' } });
    renderWith(<SwapDeckApp header={<p>intestazione</p>} />, backend);
    expect(await screen.findByText('cswap non è installato')).toBeVisible();
    expect(screen.queryByRole('tablist')).toBeNull();
    expect(screen.getByText('intestazione')).toBeVisible();
    await user.click(screen.getByRole('button', { name: 'Installa' }));
    await user.click(
      within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Installa' }),
    );
    expect(await screen.findByRole('tablist')).toBeVisible();
  });

  it('cswap pronto con account: schede subito e avviso di nuova versione sopra', async () => {
    const backend = new FakeBackend({ setup: { latest: '0.28.0' } });
    renderWith(<SwapDeckApp />, backend);
    expect(screen.getByRole('tablist')).toBeVisible();
    expect(
      await screen.findByText('È disponibile cswap 0.28.0 (installata 0.27.2).'),
    ).toBeVisible();
    expect(screen.queryByText('cswap non è installato')).toBeNull();
  });

  it('host senza SetupBackend: solo le schede', () => {
    renderWith(<SwapDeckApp />, bareBackend(new FakeBackend()));
    expect(screen.getByRole('tablist')).toBeVisible();
  });
});

describe('SwapDeckApp: pagina di dettaglio dell’utenza', () => {
  async function openDetail(name: string, backend = new FakeBackend()) {
    renderWith(<SwapDeckApp />, backend);
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: `Gestisci l’account ${name}` }));
    return { user, backend, page: screen.getByRole('region', { name: `Gestione di ${name}` }) };
  }

  it('apre solo quell’utenza, col focus su «Indietro»; «Indietro» torna all’elenco snello', async () => {
    const { user, page } = await openDetail('alias3');
    expect(screen.getByRole('heading', { name: 'Modifica alias3' })).toBeInTheDocument();
    // Solo la modifica: niente scheda dell'account, barre o percentuali.
    expect(within(page).queryByRole('listitem')).toBeNull();
    expect(within(page).queryByRole('progressbar')).toBeNull();
    expect(within(page).queryByText(/5 ore|5h/)).toBeNull();
    expect(
      within(page).queryByRole('button', { name: /Passa a questo account|storico/i }),
    ).toBeNull();
    const back = screen.getByRole('button', { name: /Indietro/ });
    expect(back).toHaveFocus();
    await user.click(back);
    expect(screen.queryByRole('region', { name: 'Gestione di alias3' })).toBeNull();
    expect(screen.getByRole('list', { name: 'Account' })).toBeInTheDocument();
    // Nell'elenco nessun pannello di gestione aperto sotto le schede.
    expect(screen.queryByLabelText('Alias')).toBeNull();
  });

  it('rimuovi con conferma: si torna all’elenco con l’esito', async () => {
    const { user, backend } = await openDetail('alias3');
    const run = vi.spyOn(backend, 'runAction');
    await user.click(screen.getByRole('button', { name: 'Rimuovi' }));
    const dialog = await screen.findByRole('alertdialog');
    expect(run).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole('button', { name: 'Rimuovi' }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ kind: 'remove', account: 3 }));
    expect(await screen.findByRole('list', { name: 'Account' })).toBeInTheDocument();
    expect(screen.getByText(/alias3.*rimoss/i)).toBeInTheDocument();
  });

  it('sposta in un altro slot: la pagina segue l’utenza', async () => {
    const { user } = await openDetail('alias3');
    await user.type(screen.getByLabelText('Sposta nello slot'), '9');
    await user.click(screen.getByRole('button', { name: 'Sposta' }));
    await user.click(
      within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Sposta' }),
    );
    expect(await screen.findByText(/slot 9/)).toBeInTheDocument();
    expect(screen.getByRole('region', { name: 'Gestione di alias3' })).toBeInTheDocument();
  });
});
