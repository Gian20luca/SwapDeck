import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { BackendProvider } from '../backend/context.js';
import { hasSetup } from '../backend/types.js';
import { FakeBackend, type FakeBackendOptions } from '../dev/FakeBackend.js';
import { I18nProvider, type LocalePreference } from '../i18n/index.js';
import { CswapUpdateNotice } from './CswapUpdateNotice.js';
import { SetupWizard } from './SetupWizard.js';

function wrap(backend: FakeBackend, node: ReactNode, locale: LocalePreference = 'it') {
  return render(
    <I18nProvider initial={locale}>
      <BackendProvider backend={backend}>{node}</BackendProvider>
    </I18nProvider>,
  );
}

const fake = (options: FakeBackendOptions = {}) => new FakeBackend(options);

describe('SetupWizard', () => {
  it('con cswap pronto e account chiama subito onReady', async () => {
    const onReady = vi.fn();
    wrap(fake(), <SetupWizard onReady={onReady} />);
    await waitFor(() => expect(onReady).toHaveBeenCalledTimes(1));
    expect(onReady.mock.calls[0]![0].status).toBe('ready');
  });

  it('cswap mancante: spiegazione, conferma, avanzamento e guida al primo account', async () => {
    const backend = fake({ scenario: 'noAccounts', setup: { state: 'missing', uv: false } });
    const onReady = vi.fn();
    wrap(backend, <SetupWizard onReady={onReady} />);
    expect(await screen.findByRole('heading', { name: 'cswap non è installato' })).toBeVisible();
    expect(screen.getByText(/uv non è presente/)).toBeVisible();

    await userEvent.click(screen.getByRole('button', { name: 'Installa' }));
    const dialog = screen.getByRole('alertdialog', { name: 'Installare cswap?' });
    expect(dialog).toHaveTextContent('Installo uv e claude-swap nel tuo profilo');
    expect(backend.setup.calls).not.toContain('install');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Installa' }));

    expect(await screen.findByRole('heading', { name: 'cswap 0.27.2 è pronto' })).toBeVisible();
    expect(backend.setup.calls).toContain('install');
    expect(screen.getByRole('heading', { name: 'Nessun account in cswap' })).toBeVisible();
    expect(screen.getByText(/esegui \/login/)).toBeVisible();
    expect(onReady).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole('button', { name: 'Aggiungi account corrente' }));
    await waitFor(() => expect(backend.actionLog).toContainEqual({ kind: 'add' }));
    await userEvent.click(screen.getByRole('button', { name: 'Continua' }));
    expect(onReady).toHaveBeenCalled();
  });

  it('con uv presente lo dice e la conferma parla solo di claude-swap', async () => {
    wrap(fake({ setup: { state: 'missing' } }), <SetupWizard onReady={() => {}} />);
    expect(await screen.findByText(/uv è già presente/)).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Installa' }));
    expect(screen.getByRole('alertdialog')).toHaveTextContent('Installo claude-swap con uv');
  });

  it('installazione non riuscita: passo fallito, log aperto, testo per «busy» e Riprova', async () => {
    const backend = fake({
      setup: {
        state: 'missing',
        failure: { kind: 'busy', message: 'os error 32', exitCode: 2 },
      },
    });
    wrap(backend, <SetupWizard onReady={() => {}} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Installa' }));
    await userEvent.click(
      within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Installa' }),
    );
    expect(
      await screen.findByRole('heading', { name: 'Installazione non riuscita' }),
    ).toBeVisible();
    expect(screen.getByRole('alert')).toHaveTextContent('Chiudi i processi cswap');
    expect(screen.getByRole('log')).toHaveTextContent('Installed 2 executables');
    expect(screen.getByText('Installo claude-swap').closest('li')).toHaveAttribute(
      'data-state',
      'failed',
    );
    expect(screen.getByRole('button', { name: 'Riprova' })).toBeEnabled();
  });

  it('versione vecchia: propone l’aggiornamento con conferma', async () => {
    const backend = fake({
      setup: { state: 'unsupportedVersion', installed: '0.25.0', latest: '0.27.2' },
    });
    const onReady = vi.fn();
    wrap(backend, <SetupWizard onReady={onReady} />);
    expect(
      await screen.findByText('È installata la 0.25.0; serve almeno la 0.26.0.'),
    ).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Aggiorna' }));
    await userEvent.click(
      within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Aggiorna' }),
    );
    await waitFor(() => expect(onReady).toHaveBeenCalled());
    expect(backend.setup.calls).toContain('upgrade');
  });

  it('schema non supportato e testi in inglese', async () => {
    wrap(fake({ setup: { state: 'unsupportedSchema' } }), <SetupWizard onReady={() => {}} />, 'en');
    expect(await screen.findByRole('heading', { name: 'Unsupported cswap format' })).toBeVisible();
    expect(screen.getByText(/schemaVersion 2/)).toBeVisible();
  });

  it('diagnosi non riuscita: errore e Riprova', async () => {
    const backend = fake();
    vi.spyOn(backend, 'diagnoseSetup').mockRejectedValueOnce(new Error('boom'));
    wrap(backend, <SetupWizard onReady={() => {}} />);
    expect(await screen.findByRole('alert')).toHaveTextContent('boom');
    await userEvent.click(screen.getByRole('button', { name: 'Riprova' }));
    expect(await screen.findByRole('heading', { name: 'cswap 0.27.2 è pronto' })).toBeVisible();
  });
});

describe('CswapUpdateNotice', () => {
  it('niente avviso senza versione nuova o con il controllo non riuscito', async () => {
    const backend = fake();
    const { container } = wrap(backend, <CswapUpdateNotice />);
    await waitFor(() => expect(backend.setup.calls).toContain('checkUpdate'));
    expect(container).toBeEmptyDOMElement();

    const offline = fake({ setup: { state: 'missing' } });
    const r = wrap(offline, <CswapUpdateNotice />);
    await waitFor(() => expect(offline.setup.calls).toContain('checkUpdate'));
    expect(r.container).toBeEmptyDOMElement();
  });

  it('chiuso: ricorda la scelta per quella versione', async () => {
    const backend = fake({ setup: { latest: '0.28.0' } });
    const first = wrap(backend, <CswapUpdateNotice />);
    expect(
      await screen.findByText('È disponibile cswap 0.28.0 (installata 0.27.2).'),
    ).toBeVisible();
    await userEvent.click(screen.getByRole('button', { name: 'Chiudi l’avviso per la 0.28.0' }));
    expect(screen.queryByText(/È disponibile/)).toBeNull();
    await waitFor(() => expect(backend.setup.calls).toContain('dismiss:0.28.0'));
    first.unmount();

    const again = wrap(backend, <CswapUpdateNotice />);
    await waitFor(() =>
      expect(backend.setup.calls.filter((c) => c === 'checkUpdate')).toHaveLength(2),
    );
    expect(again.container).toBeEmptyDOMElement();
  });

  it('«Aggiorna» chiede conferma, mostra l’avanzamento e l’esito', async () => {
    const backend = fake({ setup: { latest: '0.28.0' } });
    const onUpgraded = vi.fn();
    wrap(backend, <CswapUpdateNotice onUpgraded={onUpgraded} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Aggiorna' }));
    const dialog = screen.getByRole('alertdialog', { name: 'Aggiornare cswap alla 0.28.0?' });
    expect(dialog).toHaveTextContent('uv tool upgrade claude-swap');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Aggiorna' }));
    expect(await screen.findByText('cswap aggiornato alla 0.28.0.')).toBeVisible();
    expect(onUpgraded).toHaveBeenCalledTimes(1);
    expect(backend.setup.calls).not.toContain('dismiss:0.28.0');
  });

  it('aggiornamento non riuscito per schema incompatibile', async () => {
    const backend = fake({
      setup: {
        latest: '0.28.0',
        failure: { kind: 'unsupportedSchema', message: 'schemaVersion 2 non supportato' },
      },
    });
    wrap(backend, <CswapUpdateNotice />);
    await userEvent.click(await screen.findByRole('button', { name: 'Aggiorna' }));
    await userEvent.click(
      within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Aggiorna' }),
    );
    expect(await screen.findByText('Aggiornamento di cswap non riuscito.')).toBeVisible();
    expect(screen.getByRole('alert')).toHaveTextContent('aggiorna SwapDeck');
    expect(screen.getByRole('button', { name: 'Riprova' })).toBeVisible();
    // Il ripristino della versione precedente compare come passo in coda.
    expect(screen.getByText('Ripristino la versione precedente').closest('li')).toHaveAttribute(
      'data-state',
      'done',
    );
    // Chiudere dopo un errore non ignora la versione: l'avviso deve poter tornare.
    await userEvent.click(screen.getByRole('button', { name: 'Chiudi l’avviso per la 0.28.0' }));
    expect(screen.queryByText('Aggiornamento di cswap non riuscito.')).toBeNull();
    expect(backend.setup.calls).not.toContain('dismiss:0.28.0');
  });
});

describe('host senza SetupBackend', () => {
  it('hasSetup lo riconosce e i componenti non mostrano nulla', async () => {
    const base = fake({ setup: { state: 'missing', latest: '0.28.0' } });
    expect(hasSetup(base)).toBe(true);
    // Stesso backend, ma senza un metodo di setup: come un host che non li implementa.
    const plain = Object.assign(Object.create(base) as FakeBackend, { upgradeCswap: undefined });
    expect(hasSetup(plain)).toBe(false);
    const { container } = wrap(
      plain,
      <>
        <SetupWizard onReady={vi.fn()} />
        <CswapUpdateNotice />
      </>,
    );
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(container).toBeEmptyDOMElement();
    expect(base.setup.calls).toEqual([]);
  });
});
