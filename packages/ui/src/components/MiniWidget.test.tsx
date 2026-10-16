import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { BackendProvider } from '../backend/context.js';
import { BackendError, type Backend } from '../backend/types.js';
import { FakeBackend } from '../dev/FakeBackend.js';
import { I18nProvider } from '../i18n/index.js';
import { MiniWidget } from './MiniWidget.js';

function renderMini(backend: Backend) {
  return render(
    <I18nProvider initial="it">
      <BackendProvider backend={backend}>
        <MiniWidget />
      </BackendProvider>
    </I18nProvider>,
  );
}

describe('MiniWidget', () => {
  it('mostra l’account attivo con le barre', async () => {
    renderMini(new FakeBackend());
    expect(await screen.findByText('alias6')).toBeInTheDocument();
    expect(screen.getByText('Attivo')).toBeInTheDocument();
    expect(screen.getByRole('progressbar', { name: '5 ore di alias6: 66%' })).toBeInTheDocument();
    expect(
      screen.getByRole('progressbar', { name: '7 giorni di alias6: 77%' }),
    ).toBeInTheDocument();
    // 5 ore, 7 giorni e il limite per modello (Fable).
    expect(screen.getAllByRole('progressbar')).toHaveLength(3);
    expect(screen.getByRole('region', { name: 'Mini-widget account attivo' })).toBeInTheDocument();
  });

  it('segue il cambio di account', async () => {
    const backend = new FakeBackend();
    renderMini(backend);
    await screen.findByText('alias6');
    await backend.runAction({ kind: 'switch', target: 3, force: false });
    expect(await screen.findByText('alias3')).toBeInTheDocument();
    expect(screen.queryByText('alias6')).toBeNull();
  });

  it('senza account attivo', async () => {
    renderMini(new FakeBackend({ scenario: 'noAccounts' }));
    expect(await screen.findByText('Nessun account attivo')).toBeInTheDocument();
  });

  it('con errore permette di riprovare', async () => {
    const backend = new FakeBackend();
    vi.spyOn(backend, 'listAccounts').mockRejectedValueOnce(
      new BackendError({ kind: 'timeout', message: 'tempo scaduto' }),
    );
    renderMini(backend);
    expect(await screen.findByRole('alert')).toHaveTextContent('tempo scaduto');
    await userEvent.click(screen.getByRole('button', { name: 'Riprova' }));
    expect(await screen.findByText('alias6')).toBeInTheDocument();
  });

  it('l’account attivo con consumi non disponibili mostra l’ultimo dato noto', async () => {
    const backend = new FakeBackend({ scenario: 'stale' });
    renderMini(backend);
    const list = await backend.listAccounts();
    const active = list.accounts.find((a) => a.active);
    expect(active).toBeDefined();
    expect(
      await screen.findByRole('region', { name: 'Mini-widget account attivo' }),
    ).toBeInTheDocument();
  });
});
