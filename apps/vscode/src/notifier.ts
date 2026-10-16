/** Quando avvisare: soglie superate dall'account attivo e cambi di account (logica pura). */
import type { ListResponse } from '@swapdeck/types';
import { accountLabel } from './statusBar.js';

export type UsageWindowName = '5h' | '7d';

export type Notice =
  | {
      kind: 'threshold';
      account: number;
      name: string;
      window: UsageWindowName;
      threshold: number;
      pct: number;
    }
  | { kind: 'switch'; from: number | null; to: number; name: string };

/**
 * Ricorda per ogni account e finestra quante soglie sono già state annunciate. La prima lettura
 * fa da base (niente avvisi all'avvio di ogni finestra di VS Code); sotto una soglia l'avviso si
 * riarma, così il reset della finestra fa ripartire il conteggio.
 */
export class UsageNotifier {
  private active: number | null | undefined;
  private readonly levels = new Map<string, number>();

  update(list: ListResponse, thresholds: readonly number[]): Notice[] {
    const sorted = [...new Set(thresholds.filter((v) => Number.isFinite(v)))].sort((a, b) => a - b);
    const notices: Notice[] = [];
    const current = list.accounts.find((a) => a.active) ?? null;
    const currentNumber = current?.number ?? null;
    if (
      this.active !== undefined &&
      currentNumber !== null &&
      currentNumber !== this.active &&
      current
    ) {
      notices.push({
        kind: 'switch',
        from: this.active,
        to: currentNumber,
        name: accountLabel(current),
      });
    }
    this.active = currentNumber;
    if (!current?.usage) return notices;
    const windows: Array<[UsageWindowName, number | undefined]> = [
      ['5h', current.usage.fiveHour?.pct],
      ['7d', current.usage.sevenDay?.pct],
    ];
    for (const [window, pct] of windows) {
      if (pct === undefined || !Number.isFinite(pct)) continue;
      const key = `${current.number}:${window}`;
      const level = sorted.filter((t) => pct >= t).length;
      const previous = this.levels.get(key);
      this.levels.set(key, level);
      if (previous !== undefined && level > previous) {
        notices.push({
          kind: 'threshold',
          account: current.number,
          name: accountLabel(current),
          window,
          threshold: sorted[level - 1] ?? 0,
          pct,
        });
      }
    }
    return notices;
  }
}
