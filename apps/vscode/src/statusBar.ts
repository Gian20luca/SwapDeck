/** Testo, tooltip e livello della barra di stato (logica pura, senza `vscode`). */
import type { Account } from '@swapdeck/types';
import { displayUsage } from '@swapdeck/ui/account';
import { displayPct, levelFor, shortEmail, type Level, type Thresholds } from '@swapdeck/ui/format';
import type { CliState } from './generated/index.js';

/** Traduzione con segnaposto `{0}`, `{1}`… (in VS Code è `vscode.l10n.t`). */
export type Translate = (message: string, ...args: Array<string | number>) => string;

export interface StatusModel {
  text: string;
  tooltip: string;
  /** `error`: lettura fallita o cswap assente. */
  level: Level | 'error';
}

/** Engine di questa finestra (processo figlio) o di un'altra istanza (lock). */
export type EngineView = 'child' | 'other' | 'off';

/** Soglie dei colori dalle soglie delle notifiche: la più bassa avvisa, la più alta è critica. */
export function colorThresholds(values: readonly number[] | undefined): Thresholds {
  const valid = (values ?? []).filter((v) => Number.isFinite(v) && v > 0 && v <= 100);
  if (valid.length === 0) return { warn: 80, danger: 95 };
  return { warn: Math.min(...valid), danger: Math.max(...valid) };
}

export function accountLabel(account: Account): string {
  return account.alias && account.alias.length > 0 ? account.alias : `#${account.number}`;
}

function worst(a: Level, b: Level): Level {
  const order: Level[] = ['unknown', 'ok', 'warn', 'danger'];
  return order.indexOf(a) >= order.indexOf(b) ? a : b;
}

function pctText(pct: number | undefined): string {
  return pct === undefined ? '—' : `${displayPct(pct)}%`;
}

export function engineView(state: CliState | undefined, childRunning: boolean): EngineView {
  if (childRunning) return 'child';
  return state?.engine.running ? 'other' : 'off';
}

export function statusModel(
  state: CliState | undefined,
  thresholds: Thresholds,
  engine: EngineView,
  t: Translate,
  formatTime: (ms: number) => string,
): StatusModel {
  const engineLine =
    engine === 'child'
      ? t('Engine: running in this window')
      : engine === 'other'
        ? t('Engine: running in another instance')
        : t('Engine: stopped');
  if (!state) {
    return { text: '$(sync~spin) SwapDeck', tooltip: t('SwapDeck: loading…'), level: 'unknown' };
  }
  if (!state.list) {
    const message = state.error?.message ?? t('unknown error');
    return {
      text: '$(warning) SwapDeck',
      tooltip: [t('SwapDeck: cswap not available'), message, engineLine].join('\n'),
      level: 'error',
    };
  }
  const active = state.list.accounts.find((a) => a.active);
  const updated = t('Updated at {0}', formatTime(state.updatedAtMs));
  if (!active) {
    return {
      text: '$(account) SwapDeck: —',
      tooltip: [t('No active account'), engineLine, updated].join('\n'),
      level: 'unknown',
    };
  }
  const shown = displayUsage(active);
  const five = shown?.usage.fiveHour;
  const seven = shown?.usage.sevenDay;
  const level = worst(levelFor(five?.pct, thresholds), levelFor(seven?.pct, thresholds));
  const stale = shown?.lastKnown ? '~' : '';
  const name = accountLabel(active);
  const text = `$(account) ${name} 5h ${stale}${pctText(five?.pct)} · 7d ${stale}${pctText(seven?.pct)}`;
  const lines = [`SwapDeck — ${name} (${shortEmail(active.email)})`];
  lines.push(
    five?.countdown
      ? t('5h: {0} (resets in {1})', pctText(five.pct), five.countdown)
      : t('5h: {0}', pctText(five?.pct)),
  );
  lines.push(
    seven?.expectedPct !== undefined
      ? t('7d: {0} (expected {1})', pctText(seven.pct), pctText(seven.expectedPct))
      : t('7d: {0}', pctText(seven?.pct)),
  );
  if (shown?.lastKnown) lines.push(t('Last known data (current usage unavailable)'));
  lines.push(engineLine);
  if (state.engine.activeMap) lines.push(t('Map: {0}', state.engine.activeMap));
  lines.push(updated);
  return { text, tooltip: lines.join('\n'), level };
}
