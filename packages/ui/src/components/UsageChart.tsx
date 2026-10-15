import { useEffect, useRef } from 'react';
import uPlot from 'uplot';
import 'uplot/dist/uPlot.min.css';
import { useI18n } from '../i18n/index.js';
import { useTheme } from '../theme/index.js';

export interface ChartLine {
  label: string;
  /** Un valore per ogni istante di `x`; `null` interrompe la linea. */
  values: (number | null)[];
  /** `pace` è la curva del ritmo atteso: tratteggiata e sottile. */
  kind?: 'usage' | 'pace';
  /** Variabile CSS o colore; di default scelta dal tema. */
  color?: string;
}

export interface UsageChartProps {
  title: string;
  /** Istanti in secondi Unix, crescenti. */
  x: number[];
  lines: ChartLine[];
  height?: number;
}

const DEFAULT_COLORS = ['--sd-chart-1', '--sd-chart-2'];

function cssColor(el: HTMLElement, value: string): string {
  const name = value.startsWith('--') ? value : value.match(/^var\((--[\w-]+)\)$/)?.[1];
  if (!name) return value;
  return getComputedStyle(el).getPropertyValue(name).trim() || value;
}

/** Ultimo valore non nullo della prima linea di consumo, per l'etichetta accessibile. */
export function lastValue(lines: ChartLine[]): number | null {
  const usage = lines.find((l) => l.kind !== 'pace');
  if (!usage) return null;
  for (let i = usage.values.length - 1; i >= 0; i--) {
    const v = usage.values[i];
    if (v !== null && v !== undefined) return v;
  }
  return null;
}

/** Grafico uPlot piccolo e riusabile: scala 0–100%, tema da variabili CSS, ridimensionamento automatico. */
export function UsageChart({ title, x, lines, height = 160 }: UsageChartProps) {
  const { t } = useI18n();
  const { resolved } = useTheme();
  const hostRef = useRef<HTMLDivElement>(null);
  const hasData = x.length > 0 && lines.some((l) => l.values.some((v) => v !== null));
  const last = lastValue(lines);

  useEffect(() => {
    const host = hostRef.current;
    if (!host || !hasData) return;
    const textColor = cssColor(host, '--sd-muted');
    const gridColor = cssColor(host, '--sd-chart-grid');
    let usageIndex = 0;
    const opts: uPlot.Options = {
      width: Math.max(host.clientWidth, 200),
      height,
      legend: { show: false },
      cursor: { drag: { x: false, y: false } },
      scales: { x: { time: true }, y: { range: [0, 100] } },
      axes: [
        { stroke: textColor, grid: { stroke: gridColor, width: 1 }, ticks: { stroke: gridColor } },
        {
          stroke: textColor,
          grid: { stroke: gridColor, width: 1 },
          ticks: { stroke: gridColor },
          size: 52,
          values: (_u, ticks) => ticks.map((v) => `${v}%`),
        },
      ],
      series: [
        {},
        ...lines.map((line) => {
          const isPace = line.kind === 'pace';
          const color = cssColor(
            host,
            line.color ??
              (isPace
                ? '--sd-chart-pace'
                : (DEFAULT_COLORS[usageIndex++ % DEFAULT_COLORS.length] ?? '')),
          );
          return {
            label: line.label,
            stroke: color,
            width: isPace ? 1 : 2,
            ...(isPace ? { dash: [6, 4] } : {}),
            points: { show: false },
            spanGaps: false,
          } satisfies uPlot.Series;
        }),
      ],
    };
    const plot = new uPlot(opts, [x, ...lines.map((l) => l.values)] as uPlot.AlignedData, host);

    let observer: ResizeObserver | undefined;
    if (typeof ResizeObserver !== 'undefined') {
      observer = new ResizeObserver(() => {
        const width = host.clientWidth;
        if (width > 0) plot.setSize({ width, height });
      });
      observer.observe(host);
    }
    return () => {
      observer?.disconnect();
      plot.destroy();
    };
  }, [x, lines, height, hasData, resolved]);

  const ariaLabel =
    last === null
      ? t('chart.ariaEmpty', { title })
      : t('chart.aria', { title, value: Math.round(last) });

  if (!hasData) {
    return (
      <div className="sd-chart" role="img" aria-label={ariaLabel}>
        <div className="sd-chart__state">{t('chart.empty')}</div>
      </div>
    );
  }

  return (
    <div className="sd-chart" role="img" aria-label={ariaLabel}>
      <div ref={hostRef} data-testid="uplot-host" />
      <ul className="sd-chart__legend" aria-hidden="true">
        {lines.map((line) => (
          <li key={line.label}>{line.label}</li>
        ))}
      </ul>
    </div>
  );
}
