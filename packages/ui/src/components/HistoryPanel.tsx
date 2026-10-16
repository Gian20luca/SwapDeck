import { useEffect, useMemo, useState } from 'react';
import { useBackend } from '../backend/context.js';
import { errorMessage } from '../backend/useAccounts.js';
import type { HistoryPoint } from '../backend/types.js';
import { useI18n } from '../i18n/index.js';
import { UsageChart, type ChartLine } from './UsageChart.js';

export interface HistoryPanelProps {
  account: number;
  /** Ore mostrate nel grafico delle 5 ore (default 24); quello dei 7 giorni mostra tutto lo storico. */
  fiveHourSpanHours?: number;
  /** Intervallo da leggere in ore (24, 168, 720); senza, il default dell'host (7 giorni). Vale per entrambi i grafici. */
  rangeHours?: number;
}

type State =
  | { status: 'loading' }
  | { status: 'error'; message: string }
  | { status: 'ready'; points: HistoryPoint[] };

function series(
  points: HistoryPoint[],
  pick: (p: HistoryPoint) => [number | null, number | null],
  labels: { usage: string; pace: string },
): { x: number[]; lines: ChartLine[] } {
  const x: number[] = [];
  const usage: (number | null)[] = [];
  const pace: (number | null)[] = [];
  for (const point of points) {
    const [u, e] = pick(point);
    x.push(point.ts);
    usage.push(u);
    pace.push(e);
  }
  return {
    x,
    lines: [
      { label: labels.usage, values: usage },
      { label: labels.pace, values: pace, kind: 'pace' },
    ],
  };
}

/** Storico di un account: due grafici (5 ore e 7 giorni) con consumo e curva del ritmo atteso. */
export function HistoryPanel({ account, fiveHourSpanHours = 24, rangeHours }: HistoryPanelProps) {
  const { t } = useI18n();
  const backend = useBackend();
  const [state, setState] = useState<State>({ status: 'loading' });

  useEffect(() => {
    let alive = true;
    setState({ status: 'loading' });
    backend
      .getHistory(
        rangeHours === undefined
          ? { account }
          : { account, sinceSeconds: Math.floor(Date.now() / 1000) - rangeHours * 3600 },
      )
      .then((points) => alive && setState({ status: 'ready', points }))
      .catch(
        (error: unknown) => alive && setState({ status: 'error', message: errorMessage(error) }),
      );
    return () => {
      alive = false;
    };
  }, [backend, account, rangeHours]);

  const points = state.status === 'ready' ? state.points : null;
  const charts = useMemo(() => {
    if (!points) return null;
    const labels = { usage: t('chart.usage'), pace: t('chart.pace') };
    const last = points.at(-1)?.ts ?? 0;
    const span =
      rangeHours === undefined ? fiveHourSpanHours : Math.min(fiveHourSpanHours, rangeHours);
    const recent = points.filter((p) => p.ts >= last - span * 3600);
    return {
      five: series(recent, (p) => [p.fiveHourPct, p.fiveHourExpectedPct], labels),
      seven: series(points, (p) => [p.sevenDayPct, p.sevenDayExpectedPct], labels),
    };
  }, [points, t, fiveHourSpanHours, rangeHours]);

  if (state.status === 'loading') {
    return (
      <div className="sd-chart__state" role="status">
        {t('chart.loading')}
      </div>
    );
  }
  if (state.status === 'error') {
    return (
      <div className="sd-chart__state" role="alert">
        {t('chart.error')} {state.message}
      </div>
    );
  }
  if (!charts || points?.length === 0) {
    return <div className="sd-chart__state">{t('chart.empty')}</div>;
  }
  return (
    <>
      <h3>
        {t('chart.title')} · {t('chart.fiveHour')}
      </h3>
      <UsageChart title={`${t('chart.title')} · ${t('chart.fiveHour')}`} {...charts.five} />
      <h3>
        {t('chart.title')} · {t('chart.sevenDay')}
      </h3>
      <UsageChart title={`${t('chart.title')} · ${t('chart.sevenDay')}`} {...charts.seven} />
    </>
  );
}
