import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const created: { opts: any; data: any; destroyed: boolean }[] = [];

vi.mock('uplot', () => ({
  default: class FakeUPlot {
    private rec: { opts: any; data: any; destroyed: boolean };
    constructor(opts: unknown, data: unknown) {
      this.rec = { opts, data, destroyed: false };
      created.push(this.rec);
    }
    setSize() {}
    destroy() {
      this.rec.destroyed = true;
    }
  },
}));

import { UsageChart, lastValue } from './UsageChart.js';

beforeEach(() => {
  created.length = 0;
});

describe('UsageChart', () => {
  const x = [1000, 2000, 3000];
  const lines = [
    { label: 'Consumo', values: [10, 20, 30] as (number | null)[] },
    { label: 'Ritmo atteso', values: [5, 15, 25] as (number | null)[], kind: 'pace' as const },
  ];

  it('crea il grafico con scala 0-100 e una serie per linea, il ritmo tratteggiato', () => {
    render(<UsageChart title="5 ore" x={x} lines={lines} />);
    expect(created).toHaveLength(1);
    const { opts, data } = created[0]!;
    expect(opts.scales.y.range).toEqual([0, 100]);
    expect(opts.series).toHaveLength(3);
    expect(opts.series[1].dash).toBeUndefined();
    expect(opts.series[2].dash).toEqual([6, 4]);
    expect(data).toEqual([x, [10, 20, 30], [5, 15, 25]]);
  });

  it('espone un’etichetta accessibile con l’ultimo valore', () => {
    render(<UsageChart title="5 ore" x={x} lines={lines} />);
    expect(
      screen.getByRole('img', { name: 'Grafico: 5 ore. Ultimo valore 30%.' }),
    ).toBeInTheDocument();
  });

  it('distrugge il grafico allo smontaggio', () => {
    const { unmount } = render(<UsageChart title="t" x={x} lines={lines} />);
    unmount();
    expect(created[0]!.destroyed).toBe(true);
  });

  it('senza dati mostra lo stato vuoto e non crea il grafico', () => {
    render(<UsageChart title="t" x={[]} lines={[{ label: 'c', values: [] }]} />);
    expect(created).toHaveLength(0);
    expect(screen.getByText('Nessun dato nello storico.')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'Grafico: t. Nessun dato.' })).toBeInTheDocument();
  });

  it('lastValue salta i null finali e ignora la curva del ritmo', () => {
    expect(lastValue([{ label: 'a', values: [1, 2, null] }])).toBe(2);
    expect(lastValue([{ label: 'p', values: [9], kind: 'pace' }])).toBeNull();
  });
});
