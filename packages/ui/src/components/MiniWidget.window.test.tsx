import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { BackendProvider } from '../backend/context.js';
import type { Backend } from '../backend/types.js';
import { FakeBackend } from '../dev/FakeBackend.js';
import { bareBackend } from '../dev/testUtils.js';
import { I18nProvider } from '../i18n/index.js';
import { formatClock } from '../lib/format.js';
import { MiniWidget } from './MiniWidget.js';

function renderMini(backend: Backend, locale: 'it' | 'en' = 'it') {
  return render(
    <I18nProvider initial={locale}>
      <BackendProvider backend={backend}>
        <MiniWidget />
      </BackendProvider>
    </I18nProvider>,
  );
}

const hide = () => screen.queryByRole('button', { name: /^Nascondi il mini-widget/ });
const compact = () => screen.queryByRole('button', { name: 'Comprimi in una striscia' });

describe('MiniWidget: pulsanti della finestra', () => {
  it('nascondi e comprimi ci sono solo se l’host ha miniWindow', async () => {
    const { unmount } = renderMini(new FakeBackend({ miniWindow: false }));
    await screen.findByText('alias6');
    expect(hide()).toBeNull();
    expect(compact()).toBeNull();
    expect(screen.getByRole('button', { name: /^Aggiorna ora/ })).toBeInTheDocument();
    unmount();
    renderMini(new FakeBackend());
    await screen.findByText('alias6');
    expect(hide()).toBeInTheDocument();
    expect(compact()).toBeInTheDocument();
  });

  it('i pulsanti hanno etichetta e tooltip, anche in inglese', async () => {
    renderMini(new FakeBackend(), 'en');
    await screen.findByText('alias6');
    for (const name of [
      'Hide the mini-widget (stays in the notification area)',
      'Collapse to a strip',
    ]) {
      expect(screen.getByRole('button', { name })).toHaveAccessibleDescription(name);
    }
  });

  it('nascondi chiama l’host', async () => {
    const backend = new FakeBackend();
    renderMini(backend);
    await screen.findByText('alias6');
    await userEvent.click(hide()!);
    expect(backend.miniLog).toEqual(['hide']);
  });

  it('comprimi: striscia con alias e % 5h nel colore della soglia, poi riespandi', async () => {
    const backend = new FakeBackend();
    renderMini(backend);
    await screen.findByText('alias6');
    await userEvent.click(compact()!);
    expect(backend.miniLog).toEqual(['compact']);
    const strip = await screen.findByRole('region', { name: 'Mini-widget account attivo' });
    expect(strip).toHaveClass('sd-mini--compact');
    expect(within(strip).getByText('alias6')).toBeInTheDocument();
    expect(within(strip).getByText('5 ore 66%')).toHaveAttribute('data-level', 'ok');
    expect(screen.queryAllByRole('progressbar')).toHaveLength(0);
    expect(screen.getByRole('button', { name: 'Espandi il mini-widget' })).toBeInTheDocument();
    expect(hide()).toBeInTheDocument();
    // Un clic sulla striscia la riespande.
    await userEvent.click(strip);
    expect(await screen.findAllByRole('progressbar')).toHaveLength(3);
    expect(backend.miniLog).toEqual(['compact', 'expand']);
  });

  it('il pulsante Espandi riespande, una volta sola', async () => {
    const backend = new FakeBackend();
    renderMini(backend);
    await screen.findByText('alias6');
    await userEvent.click(compact()!);
    await userEvent.click(await screen.findByRole('button', { name: 'Espandi il mini-widget' }));
    expect(await screen.findAllByRole('progressbar')).toHaveLength(3);
    expect(backend.miniLog).toEqual(['compact', 'expand']);
  });

  it('riparte compresso se l’host lo aveva salvato così', async () => {
    const backend = new FakeBackend();
    await backend.miniWindow!.setCompact(true);
    renderMini(backend);
    const strip = await screen.findByRole('region', { name: 'Mini-widget account attivo' });
    expect(await within(strip).findByText('alias6')).toBeInTheDocument();
    expect(strip).toHaveClass('sd-mini--compact');
  });

  it('il livello della striscia segue le soglie', async () => {
    const backend = new FakeBackend();
    const list = await backend.listAccounts();
    list.accounts.find((a) => a.active)!.usage!.fiveHour!.pct = 96;
    vi.spyOn(backend, 'listAccounts').mockResolvedValue(list);
    await backend.miniWindow!.setCompact(true);
    renderMini(backend);
    expect(await screen.findByText('5 ore 96%')).toHaveAttribute('data-level', 'danger');
  });

  it('adatta l’altezza della finestra al contenuto', async () => {
    const spy = vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockReturnValue(221.4);
    const backend = new FakeBackend();
    renderMini(backend);
    await screen.findByText('alias6');
    await waitFor(() => expect(backend.miniHeights).toContain(222));
    spy.mockRestore();
  });
});

describe('MiniWidget: aggiornamento, limiti per modello, ora', () => {
  it('Aggiorna chiede la lettura al backend e mostra «in aggiornamento»', async () => {
    const backend = new FakeBackend();
    const spy = vi.spyOn(backend, 'listAccounts');
    renderMini(backend);
    await screen.findByText('alias6');
    const button = screen.getByRole('button', { name: /^Aggiorna ora/ });
    expect(button).toHaveAccessibleName(
      'Aggiorna ora (cswap rilegge i consumi al massimo ogni 3 minuti)',
    );
    expect(button).not.toHaveAttribute('aria-busy');
    await userEvent.click(button);
    expect(spy).toHaveBeenLastCalledWith({ refresh: true });
    const busy = await screen.findByRole('button', { name: 'Aggiornamento in corso…' });
    expect(busy).toHaveAttribute('aria-busy', 'true');
    await waitFor(
      () => expect(screen.getByRole('button', { name: /^Aggiorna ora/ })).toBeVisible(),
      {
        timeout: 2000,
      },
    );
  });

  it('mostra i limiti per modello con barra e percentuale', async () => {
    renderMini(new FakeBackend());
    expect(
      await screen.findByRole('progressbar', { name: 'Fable di alias6: 0%' }),
    ).toBeInTheDocument();
    expect(screen.getByText('Fable')).toBeInTheDocument();
  });

  it('«Aggiornato alle» con ore, minuti e secondi di usageFetchedAt', async () => {
    const backend = new FakeBackend();
    const active = (await backend.listAccounts()).accounts.find((a) => a.active)!;
    renderMini(backend);
    const time = formatClock(active.usageFetchedAt)!;
    expect(time).toMatch(/^\d\d:\d\d:\d\d$/);
    const line = await screen.findByText(`Aggiornato alle ${time}`);
    expect(line).toHaveAttribute('data-stale', 'false');
  });

  it('dato vecchio: lo dice come la dashboard', async () => {
    const backend = new FakeBackend();
    const list = await backend.listAccounts();
    list.accounts.find((a) => a.active)!.usageAgeSeconds = 1200;
    vi.spyOn(backend, 'listAccounts').mockResolvedValue(list);
    renderMini(backend);
    const line = await screen.findByText(/^Dato vecchio, aggiornato alle \d\d:\d\d:\d\d$/);
    expect(line).toHaveAttribute('data-stale', 'true');
  });

  it('la data del reset sta accanto al countdown', async () => {
    renderMini(new FakeBackend());
    await screen.findByText('alias6');
    expect(screen.getAllByText(/^Reset tra .+ · .+/).length).toBeGreaterThanOrEqual(2);
  });
});

describe('MiniWidget: soglie del motore', () => {
  const marks = () => screen.findAllByRole('note', { name: /^Soglia del motore/ });

  it('motore acceso: due trattini rossi (5 ore e 7 giorni), niente sul limite per modello', async () => {
    renderMini(new FakeBackend({ rules: { enabled: true, primary: 6 } }));
    const found = await marks();
    expect(found.map((m) => m.getAttribute('aria-label'))).toEqual([
      'Soglia del motore: 95%',
      'Soglia del motore: 98%',
    ]);
    for (const m of found) expect(m).toHaveAttribute('data-tone', 'active');
    expect(found[0]).toHaveAccessibleDescription(/^A 95% il motore passa a un’altra utenza/);
    const fable = screen.getByRole('progressbar', { name: /^Fable/ }).closest('.sd-bar')!;
    expect(within(fable as HTMLElement).queryByRole('note', { name: /^Soglia/ })).toBeNull();
  });

  it('motore spento (default): grigi', async () => {
    renderMini(new FakeBackend());
    const [five] = await marks();
    expect(five).toHaveAttribute('data-tone', 'muted');
    expect(five).toHaveAccessibleDescription('Soglia 95%: nessun cambio automatico, motore spento');
  });

  it('solo osservazione', async () => {
    renderMini(new FakeBackend({ rules: { enabled: true, primary: 6, dryRun: true } }));
    const [five] = await marks();
    expect(five).toHaveAccessibleDescription(/in sola osservazione: registra soltanto$/);
  });

  it('sospeso per conflitto', async () => {
    renderMini(
      new FakeBackend({
        rules: { enabled: true, primary: 6 },
        conflicts: [{ id: 'task:x', kind: 'scheduledTask', label: 'x', certain: true }],
      }),
    );
    const [five] = await marks();
    expect(five).toHaveAttribute('data-tone', 'muted');
    expect(five).toHaveAccessibleDescription(/sospeso: altro motore attivo$/);
  });

  it('senza soglia sui 7 giorni resta il solo trattino delle 5 ore', async () => {
    renderMini(new FakeBackend({ rules: { enabled: true, primary: 6, sevenDayThreshold: null } }));
    expect(await marks()).toHaveLength(1);
  });

  it('un host senza stato del motore non mostra trattini', async () => {
    renderMini(bareBackend(new FakeBackend()));
    await screen.findByText('alias6');
    expect(screen.queryByRole('note', { name: /^Soglia/ })).toBeNull();
  });
});
