import type { OtherDevicesEstimate, UsageBreakdown } from '@swapdeck/types';
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { BackendError } from '../../backend/types.js';
import { FakeBackend } from '../../dev/FakeBackend.js';
import { bareBackend, renderWith } from '../../dev/testUtils.js';
import { UsagePage } from './UsagePage.js';

async function setup(backend = new FakeBackend(), target?: { account: number; key: number }) {
  const spy = vi.spyOn(backend, 'getUsageBreakdown');
  renderWith(<UsagePage {...(target ? { target } : {})} />, backend);
  await screen.findByRole('list', { name: 'Classifica dei consumi' });
  return { backend, spy, user: userEvent.setup() };
}

const ranking = () => screen.getByRole('list', { name: 'Classifica dei consumi' });
const names = () =>
  within(ranking())
    .getAllByRole('listitem')
    .map((li) => li.querySelector('.sd-usage__name')?.textContent);

/** Risposta del Fake con le stime sostituite. */
async function withEstimates(
  backend: FakeBackend,
  estimates: Partial<OtherDevicesEstimate>[],
  patch: Partial<UsageBreakdown> = {},
) {
  const real = backend.getUsageBreakdown.bind(backend);
  vi.spyOn(backend, 'getUsageBreakdown').mockImplementation(async (query) => {
    const base = await real(query);
    return {
      ...base,
      otherDevices: base.otherDevices.map((e, i) => ({ ...e, ...(estimates[i] ?? {}) })),
      ...patch,
    };
  });
}

describe('UsagePage', () => {
  it('classifica per progetto: barre, percentuale, token e ultimo uso, dal più alto', async () => {
    const { spy } = await setup();
    expect(spy).toHaveBeenCalledWith({ window: '5h', groupBy: 'project' });
    expect(names()[0]).toBe('SwapDeck');
    const first = within(ranking()).getAllByRole('listitem')[0]!;
    expect(within(first).getByRole('img', { name: /% del consumo/ })).toBeInTheDocument();
    expect(first.textContent).toMatch(/token eq\. · \d+ messaggi · ultimo uso/);
    expect(first.querySelector('.sd-bar__pct')?.textContent).toMatch(/^\d+(,\d)?%$/);
    expect(screen.getByText(/Su questo PC: .* token equivalenti/)).toBeInTheDocument();
  });

  it('cambio di raggruppamento: sessione, agente (conversazione principale e sotto-agenti), modello', async () => {
    const { spy, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Sessione' }));
    await waitFor(() => expect(spy).toHaveBeenLastCalledWith({ window: '5h', groupBy: 'session' }));
    await screen.findByRole('list', { name: 'Classifica dei consumi' });
    expect(names()).toContain('sess-a1');
    expect(screen.getByRole('button', { name: 'Sessione' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );

    await user.click(screen.getByRole('button', { name: 'Agente' }));
    await waitFor(() => expect(spy).toHaveBeenLastCalledWith({ window: '5h', groupBy: 'agent' }));
    await screen.findByRole('list', { name: 'Classifica dei consumi' });
    expect(names()).toEqual(expect.arrayContaining(['Conversazione principale', 'Explore']));
    expect(screen.getByText(/Cerca nel codice/)).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Modello' }));
    await screen.findByText('claude-opus-4-8');
    expect(names()).toContain('claude-haiku-4-5');
  });

  it('cambio di finestra e di utenza: la richiesta li porta al backend', async () => {
    const { spy, user } = await setup();
    await user.click(screen.getByRole('button', { name: '7 giorni' }));
    await waitFor(() => expect(spy).toHaveBeenLastCalledWith({ window: '7d', groupBy: 'project' }));
    await screen.findByRole('list', { name: 'Classifica dei consumi' });
    await user.selectOptions(screen.getByLabelText('Utenza'), '2');
    await waitFor(() =>
      expect(spy).toHaveBeenLastCalledWith({ window: '7d', groupBy: 'project', account: 2 }),
    );
  });

  it('arriva da «Chi consuma questa utenza»: utenza già scelta', async () => {
    const { spy } = await setup(new FakeBackend(), { account: 1, key: 1 });
    expect(screen.getByLabelText('Utenza')).toHaveValue('1');
    expect(spy).toHaveBeenLastCalledWith({ window: '5h', groupBy: 'project', account: 1 });
  });

  it('altri dispositivi: fiducia media e bassa con il tooltip del metodo, 7 giorni non calibrata', async () => {
    const { user } = await setup();
    const block = screen.getByRole('list', { name: 'Altri dispositivi (stima)' });
    const [first, second] = within(block).getAllByRole('listitem');
    expect(within(first!).getByText('Fiducia media')).toBeInTheDocument();
    expect(first!.textContent).toMatch(/≈ 11,5% della quota/);
    expect(first!.textContent).toMatch(/Server 42% · questo PC 30,5%/);
    expect(within(second!).getByText('Fiducia bassa')).toBeInTheDocument();
    expect(within(second!).getByText(/Storico incompleto/)).toBeInTheDocument();
    const badge = within(first!).getByText('Fiducia media');
    const tip = document.getElementById(badge.getAttribute('aria-describedby')!.split(' ').pop()!);
    expect(tip).toHaveAttribute('role', 'tooltip');
    expect(tip?.textContent).toMatch(/Metodo:/);

    await user.click(screen.getByRole('button', { name: '7 giorni' }));
    const week = await screen.findByRole('list', { name: 'Altri dispositivi (stima)' });
    expect(within(week).getByText('Stima non ancora disponibile')).toBeInTheDocument();
    expect(within(week).getByText(/nessuna lettura del server/)).toBeInTheDocument();
    expect(within(week).queryByText(/Fiducia/)).toBeNull();
  });

  it('con l’utenza scelta e la stima disponibile: barra degli altri dispositivi e percentuali sul totale', async () => {
    const backend = new FakeBackend();
    await withEstimates(backend, [{ confidence: 'high', samples: 9 }]);
    const { user } = await setup(backend);
    await user.selectOptions(screen.getByLabelText('Utenza'), '1');
    const block = await screen.findByRole('list', { name: 'Altri dispositivi (stima)' });
    expect(within(block).getByText('Fiducia alta')).toBeInTheDocument();
    expect(within(block).getByRole('img', { name: /Altri dispositivi: \d/ })).toBeInTheDocument();
  });

  it('utenza sconosciuta: blocco a parte quando l’attribuzione manca', async () => {
    const { user } = await setup();
    expect(screen.queryByText('Utenza sconosciuta')).toBeNull();
    await user.click(screen.getByRole('button', { name: '7 giorni' }));
    expect(await screen.findByText('Utenza sconosciuta')).toBeInTheDocument();
    expect(screen.getByText(/non attribuiti a nessuna utenza/)).toBeInTheDocument();
  });

  it('prima analisi: caricamento, poi avanzamento indeterminato; la pagina resta usabile', async () => {
    const backend = new FakeBackend();
    const real = backend.getUsageBreakdown.bind(backend);
    let release: () => void = () => undefined;
    vi.spyOn(backend, 'getUsageBreakdown').mockImplementation(
      (query) => new Promise((resolve) => (release = () => resolve(real(query)))),
    );
    renderWith(<UsagePage />, backend);
    expect(screen.getByRole('status')).toHaveTextContent('Caricamento…');
    expect(await screen.findByText('Prima analisi delle sessioni in corso…')).toBeInTheDocument();
    const progress = screen.getByRole('progressbar');
    expect(progress).not.toHaveAttribute('value');
    // I controlli e la riga sulla privacy ci sono già durante la lettura.
    expect(screen.getByRole('button', { name: '7 giorni' })).toBeEnabled();
    expect(screen.getByRole('note')).toHaveTextContent(/nulla esce dal PC/);
    release();
    expect(await screen.findByRole('list', { name: 'Classifica dei consumi' })).toBeInTheDocument();
    expect(screen.queryByRole('progressbar')).toBeNull();
  });

  it('una lettura lenta superata da una più recente non sovrascrive il risultato', async () => {
    const backend = new FakeBackend();
    const real = backend.getUsageBreakdown.bind(backend);
    const slow: Array<() => void> = [];
    vi.spyOn(backend, 'getUsageBreakdown').mockImplementation((query) =>
      query.groupBy === 'project'
        ? new Promise((resolve) => slow.push(() => resolve(real(query))))
        : real(query),
    );
    renderWith(<UsagePage />, backend);
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Modello' }));
    await screen.findByText('claude-opus-4-8');
    slow.forEach((release) => release());
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(names()).toContain('claude-opus-4-8');
  });

  it('lettura a budget: richiama finché c’è partial e mostra l’avanzamento vero', async () => {
    const backend = new FakeBackend({ usageScanCalls: 2 });
    const real = backend.getUsageBreakdown.bind(backend);
    const pending: Array<() => void> = [];
    const spy = vi
      .spyOn(backend, 'getUsageBreakdown')
      .mockImplementation(
        (query) => new Promise((resolve) => pending.push(() => resolve(real(query)))),
      );
    renderWith(<UsagePage />, backend);
    const next = async (calls: number) => {
      await waitFor(() => expect(pending).toHaveLength(calls));
      pending[calls - 1]!();
    };
    await next(1);
    expect(await screen.findByText('Letto il 33% delle sessioni')).toBeInTheDocument();
    const progress = screen.getByRole('progressbar');
    expect(Number(progress.getAttribute('value'))).toBeCloseTo(33.33, 1);
    expect(progress).toHaveAttribute('max', '100');
    await next(2);
    expect(await screen.findByText('Letto il 66% delle sessioni')).toBeInTheDocument();
    await next(3);
    await screen.findByRole('list', { name: 'Classifica dei consumi' });
    expect(spy).toHaveBeenCalledTimes(3);
    expect(spy).toHaveBeenLastCalledWith({ window: '5h', groupBy: 'project' });
    expect(screen.queryByText(/Lettura delle sessioni non completata/)).toBeNull();
  });

  it('lettura a budget senza avanzamento: si ferma e mostra quanto letto con l’avviso', async () => {
    const backend = new FakeBackend();
    const real = backend.getUsageBreakdown.bind(backend);
    const spy = vi.spyOn(backend, 'getUsageBreakdown').mockImplementation(async (query) => ({
      ...(await real(query)),
      partial: { scannedBytes: 10, totalBytes: 100 },
    }));
    await setup(backend);
    expect(spy).toHaveBeenCalledTimes(2);
    expect(screen.getByText(/Lettura delle sessioni non completata/)).toBeInTheDocument();
  });

  it('una lettura a budget superata da una più recente smette di richiamare', async () => {
    const backend = new FakeBackend();
    const real = backend.getUsageBreakdown.bind(backend);
    let scanned = 0;
    const slow: Array<() => void> = [];
    const spy = vi.spyOn(backend, 'getUsageBreakdown').mockImplementation((query) =>
      query.groupBy === 'project'
        ? new Promise((resolve) =>
            slow.push(() =>
              resolve(
                real(query).then((r) => ({
                  ...r,
                  partial: { scannedBytes: (scanned += 10), totalBytes: 1000 },
                })),
              ),
            ),
          )
        : real(query),
    );
    renderWith(<UsagePage />, backend);
    await waitFor(() => expect(slow).toHaveLength(1));
    slow[0]!();
    await screen.findByText('Letto il 1% delle sessioni');
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Modello' }));
    await screen.findByText('claude-opus-4-8');
    slow.forEach((release) => release());
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(names()).toContain('claude-opus-4-8');
    const projectCalls = spy.mock.calls.filter(([q]) => q.groupBy === 'project').length;
    expect(projectCalls).toBe(2);
  });

  it('privacy: riga fissa in fondo alla pagina', async () => {
    await setup();
    const note = screen.getByText(
      'Si leggono solo i conteggi dei token e le descrizioni degli agenti delle sessioni di Claude Code su questo PC; il testo dei messaggi non viene letto e nulla esce dal PC.',
    );
    expect(note.parentElement?.lastElementChild).toBe(note);
  });

  it('Claude Code non trovato, nessun consumo, righe tagliate e avvisi della lettura', async () => {
    const backend = new FakeBackend();
    await withEstimates(backend, [], { sourceAvailable: false });
    renderWith(<UsagePage />, backend);
    expect(
      await screen.findByText('Non trovo le sessioni di Claude Code su questo PC.'),
    ).toBeVisible();
  });

  it('nessun consumo nella finestra', async () => {
    const backend = new FakeBackend();
    await withEstimates(backend, [], { rows: [], otherDevices: [] });
    renderWith(<UsagePage />, backend);
    expect(await screen.findByText('Nessun consumo registrato in questa finestra.')).toBeVisible();
    expect(screen.queryByRole('list', { name: 'Altri dispositivi (stima)' })).toBeNull();
  });

  it('righe tagliate e avvisi discreti della lettura', async () => {
    const backend = new FakeBackend();
    const real = backend.getUsageBreakdown.bind(backend);
    vi.spyOn(backend, 'getUsageBreakdown').mockImplementation(async (q) => {
      const base = await real(q);
      return {
        ...base,
        truncatedRows: 12,
        scan: { ...base.scan, indexReset: true, saveFailed: true, incomplete: true },
      };
    });
    renderWith(<UsagePage />, backend);
    expect(await screen.findByText('Altre 12 righe non sono mostrate.')).toBeVisible();
    expect(screen.getByText('L’indice delle letture è stato rifatto da capo.')).toBeVisible();
    expect(screen.getByText(/Indice non salvato/)).toBeVisible();
    expect(screen.getByText(/non sono leggibili/)).toBeVisible();
  });

  it('errore del backend con «Riprova»', async () => {
    const backend = new FakeBackend();
    const spy = vi
      .spyOn(backend, 'getUsageBreakdown')
      .mockRejectedValueOnce(new BackendError({ kind: 'failed', message: 'cartella illeggibile' }));
    renderWith(<UsagePage />, backend);
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('cartella illeggibile');
    await userEvent.setup().click(within(alert).getByRole('button', { name: 'Riprova' }));
    expect(await screen.findByRole('list', { name: 'Classifica dei consumi' })).toBeInTheDocument();
    expect(spy).toHaveBeenCalledTimes(2);
  });

  it('host senza lettura dei consumi: funzione non disponibile', () => {
    renderWith(<UsagePage />, bareBackend(new FakeBackend()));
    expect(screen.getByText('Questa funzione non è disponibile in questo ambiente.')).toBeVisible();
    expect(screen.queryByRole('group', { name: 'Finestra' })).toBeNull();
  });

  it('in inglese', async () => {
    renderWith(<UsagePage />, new FakeBackend(), 'en');
    expect(await screen.findByRole('list', { name: 'Usage ranking' })).toBeInTheDocument();
    expect(screen.getByText('Other devices (estimate)')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Session' })).toBeInTheDocument();
  });
});
