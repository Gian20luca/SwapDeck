import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { FakeBackend } from '../dev/FakeBackend.js';
import { renderWith } from '../dev/testUtils.js';
import type { EngineMark } from '../lib/engineMarks.js';
import { formatWhen } from '../lib/format.js';
import { UsageBar } from './UsageBar.js';

const now = Date.parse('2026-10-15T20:00:00Z');

describe('UsageBar', () => {
  it('mostra percentuale, countdown e progressbar accessibile', () => {
    render(
      <UsageBar
        label="5 ore"
        accountName="alias6"
        pct={47.4}
        resetsAt="2026-10-15T21:57:59+00:00"
        now={now}
      />,
    );
    const bar = screen.getByRole('progressbar', { name: '5 ore di alias6: 47%' });
    expect(bar).toHaveAttribute('aria-valuenow', '47');
    expect(screen.getByText('47%')).toBeInTheDocument();
    expect(screen.getByText(/^Reset tra 1h 57m · /)).toBeInTheDocument();
  });

  it('applica il livello di soglia', () => {
    const { container, rerender } = render(
      <UsageBar label="x" accountName="a" pct={50} now={now} />,
    );
    expect(container.querySelector('.sd-bar')).toHaveAttribute('data-level', 'ok');
    rerender(<UsageBar label="x" accountName="a" pct={85} now={now} />);
    expect(container.querySelector('.sd-bar')).toHaveAttribute('data-level', 'warn');
    rerender(<UsageBar label="x" accountName="a" pct={97} now={now} />);
    expect(container.querySelector('.sd-bar')).toHaveAttribute('data-level', 'danger');
  });

  it('gestisce il dato assente senza aria-valuenow', () => {
    render(<UsageBar label="7 giorni" accountName="a" pct={null} now={now} />);
    const bar = screen.getByRole('progressbar');
    expect(bar).not.toHaveAttribute('aria-valuenow');
    expect(screen.getByText('n.d.')).toBeInTheDocument();
    expect(screen.getByText('Nessun reset in corso')).toBeInTheDocument();
  });

  it('segna il ritmo atteso con un tacco', () => {
    render(<UsageBar label="x" accountName="a" pct={10} expectedPct={44.3} now={now} />);
    expect(screen.getByTestId('pace-mark')).toHaveStyle({ left: '44.3%' });
  });

  it('in modalità compatta nasconde il piede se non c’è reset', () => {
    const { container } = render(<UsageBar label="x" accountName="a" pct={10} now={now} compact />);
    expect(container.querySelector('.sd-bar__foot')).toBeNull();
  });
});

describe('UsageBar: trattini del motore e del ritmo', () => {
  const mark = (over: Partial<EngineMark> = {}): EngineMark => ({
    pct: 95,
    mode: 'active',
    criterion: 'lowest-weekly',
    reason: null,
    ...over,
  });
  const engine = () => screen.getByRole('note', { name: /Soglia del motore/ });

  it('motore acceso: trattino rosso alla soglia, con tooltip e criterio', () => {
    render(<UsageBar label="5 ore" accountName="a" pct={50} now={now} engineMark={mark()} />);
    expect(engine()).toHaveAttribute('data-tone', 'active');
    expect(engine()).toHaveStyle({ left: '95%' });
    expect(engine()).toHaveAccessibleName('Soglia del motore: 95%');
    expect(engine()).toHaveAccessibleDescription(
      'A 95% il motore passa a un’altra utenza (riserva con la settimanale più bassa)',
    );
    expect(engine()).toHaveAttribute('tabindex', '0');
  });

  it.each([
    ['best', 'più margine'],
    ['fixed-order', 'ordine fisso'],
  ] as const)('criterio %s nel tooltip', (criterion, text) => {
    render(
      <UsageBar label="x" accountName="a" pct={1} now={now} engineMark={mark({ criterion })} />,
    );
    expect(engine()).toHaveAccessibleDescription(
      `A 95% il motore passa a un’altra utenza (${text})`,
    );
  });

  it('motore spento: grigio e tooltip dedicato', () => {
    render(
      <UsageBar label="x" accountName="a" pct={1} now={now} engineMark={mark({ mode: 'off' })} />,
    );
    expect(engine()).toHaveAttribute('data-tone', 'muted');
    expect(engine()).toHaveAccessibleDescription(
      'Soglia 95%: nessun cambio automatico, motore spento',
    );
  });

  it('sola osservazione: grigio, registra soltanto', () => {
    render(
      <UsageBar
        label="x"
        accountName="a"
        pct={1}
        now={now}
        engineMark={mark({ mode: 'observe' })}
      />,
    );
    expect(engine()).toHaveAttribute('data-tone', 'muted');
    expect(engine()).toHaveAccessibleDescription(
      'Soglia 95%: nessun cambio automatico, in sola osservazione: registra soltanto',
    );
  });

  it('sospeso: grigio, con il motivo', () => {
    render(
      <UsageBar
        label="x"
        accountName="a"
        pct={1}
        now={now}
        engineMark={mark({ mode: 'suspended', reason: 'noViableTarget', pct: 98 })}
      />,
    );
    expect(engine()).toHaveAttribute('data-tone', 'muted');
    expect(engine()).toHaveStyle({ left: '98%' });
    expect(engine()).toHaveAccessibleDescription(
      'Soglia 98%: nessun cambio automatico, sospeso: nessun account utilizzabile',
    );
  });

  it('in inglese', () => {
    renderWith(
      <UsageBar label="x" accountName="a" pct={1} now={now} engineMark={mark({ mode: 'off' })} />,
      new FakeBackend(),
      'en',
    );
    expect(screen.getByRole('note', { name: 'Engine threshold: 95%' })).toHaveAccessibleDescription(
      'Threshold 95%: no automatic switch, engine off',
    );
  });

  it('il ritmo atteso ha un tooltip suo e non si confonde con la soglia', () => {
    render(
      <UsageBar
        label="x"
        accountName="a"
        pct={10}
        expectedPct={44.3}
        now={now}
        engineMark={mark()}
      />,
    );
    const pace = screen.getByTestId('pace-mark');
    expect(pace).toHaveAttribute('data-kind', 'pace');
    expect(pace).toHaveAccessibleDescription('Consumo atteso a quest’ora per arrivare al reset');
    expect(engine()).toHaveAttribute('data-kind', 'engine');
    expect(screen.getAllByRole('note')).toHaveLength(2);
  });

  it('senza soglia non c’è il trattino', () => {
    render(<UsageBar label="x" accountName="a" pct={10} now={now} />);
    expect(screen.queryByRole('note')).toBeNull();
  });
});

describe('UsageBar: data e ora del reset', () => {
  it('«Reset tra …» seguito da data e ora', () => {
    render(
      <UsageBar
        label="5 ore"
        accountName="a"
        pct={10}
        resetsAt="2026-10-19T16:59:59+00:00"
        now={now}
      />,
    );
    const expected = formatWhen('2026-10-19T16:59:59+00:00', now, 'it', {
      today: 'oggi',
      tomorrow: 'domani',
    });
    expect(screen.getByText(`Reset tra 3d 20h · ${expected}`)).toBeInTheDocument();
  });

  it('la riga «Al ritmo attuale finisce» c’è solo se cade prima del reset', () => {
    const { rerender } = render(
      <UsageBar
        label="7 giorni"
        accountName="a"
        pct={80}
        resetsAt="2026-10-19T16:59:59+00:00"
        exhaustionAt="2026-10-18T13:40:00Z"
        now={now}
      />,
    );
    expect(screen.getByText(/^Al ritmo attuale finisce: /)).toBeInTheDocument();
    rerender(
      <UsageBar
        label="7 giorni"
        accountName="a"
        pct={80}
        resetsAt="2026-10-19T16:59:59+00:00"
        exhaustionAt="2026-10-21T13:40:00Z"
        now={now}
      />,
    );
    expect(screen.queryByText(/Al ritmo attuale finisce/)).toBeNull();
  });
});

describe('UsageBar: reset già avvenuto', () => {
  // Ore locali, come le mostra la barra: 14:13 del 16, reset alle 00:20 dello stesso giorno.
  const at1413 = new Date(2026, 9, 16, 14, 13).getTime();
  const today0020 = new Date(2026, 9, 16, 0, 20).toISOString();
  const yesterday2350 = new Date(2026, 9, 15, 23, 50).toISOString();

  it('di oggi: «Reset avvenuto alle …» e barra in stile dato vecchio, valore conservato', () => {
    const { container } = render(
      <UsageBar label="5 ore" accountName="a" pct={88} resetsAt={today0020} now={at1413} />,
    );
    expect(screen.getByText('Reset avvenuto alle 00:20 · dati da aggiornare')).toBeInTheDocument();
    expect(screen.queryByText(/Reset tra/)).toBeNull();
    expect(container.querySelector('.sd-bar')).toHaveAttribute('data-expired', 'true');
    expect(screen.getByText('88%')).toBeInTheDocument();
  });

  it('di ieri, anche compatta e in inglese; niente riga di esaurimento', () => {
    const { container } = renderWith(
      <UsageBar
        label="7 days"
        accountName="a"
        pct={40}
        resetsAt={yesterday2350}
        exhaustionAt={new Date(2026, 9, 15, 20, 0).toISOString()}
        now={at1413}
        compact
      />,
      new FakeBackend(),
      'en',
    );
    expect(screen.getByText('Reset yesterday 23:50 · data needs a refresh')).toBeInTheDocument();
    expect(container.querySelector('.sd-bar__exhaust')).toBeNull();
  });

  it('reset futuro: conto alla rovescia, senza stile di dato vecchio', () => {
    const { container } = render(
      <UsageBar
        label="5 ore"
        accountName="a"
        pct={10}
        resetsAt={new Date(2026, 9, 16, 15, 0).toISOString()}
        now={at1413}
      />,
    );
    expect(screen.getByText('Reset tra 47m · oggi 15:00')).toBeInTheDocument();
    expect(container.querySelector('.sd-bar')).not.toHaveAttribute('data-expired');
  });
});
