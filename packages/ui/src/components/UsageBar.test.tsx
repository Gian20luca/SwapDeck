import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
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
    expect(screen.getByText('Reset tra 1h 57m')).toBeInTheDocument();
  });

  it('applica il livello di soglia', () => {
    const { container, rerender } = render(<UsageBar label="x" accountName="a" pct={50} now={now} />);
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
