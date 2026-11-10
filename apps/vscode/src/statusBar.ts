/**
 * Testo e tooltip della barra di stato dal riepilogo della CLI (`swapdeck-cli ide status`): la
 * logica (livelli, ultimo dato noto, titolare del motore, soglie) sta nel Rust; qui restano solo
 * le traduzioni e la composizione dei testi.
 */
import type { CswapError, IdeEngine, IdeLevel, IdeStatus } from '@swapdeck/types';

/** Traduzione con segnaposto `{0}`, `{1}`… (in VS Code è `vscode.l10n.t`). */
export type Translate = (message: string, ...args: Array<string | number>) => string;

export interface StatusModel {
  text: string;
  tooltip: string;
  /** `error`: lettura fallita o cswap assente. */
  level: IdeLevel;
}

/** Nome da mostrare: l'alias, altrimenti il numero (`#6`). */
export function accountLabel(account: { alias?: string | null; number: number }): string {
  return account.alias && account.alias.length > 0 ? account.alias : `#${account.number}`;
}

/**
 * Argomenti di `ide status` con lo stato su stdin: soglie dalle impostazioni delle notifiche
 * (la CLI ignora i valori non validi) e PID del motore avviato da questa finestra.
 */
export function statusArgs(
  thresholds: readonly unknown[] | undefined,
  enginePid: number | undefined,
): string[] {
  const args = ['ide', 'status', '--thresholds', (thresholds ?? []).join(',')];
  if (enginePid !== undefined) args.push('--engine-pid', String(enginePid));
  args.push('-');
  return args;
}

/** Riepilogo d'errore quando la CLI stessa non risponde (niente da chiedere al Rust). */
export function errorStatus(error: CswapError): IdeStatus {
  return {
    level: 'error',
    content: { kind: 'error', error },
    engine: { holder: 'off', pid: null, switching: false, dryRun: false, activeMap: null },
    updatedAtMs: Date.now(),
    thresholds: { warn: 80, danger: 95 },
  };
}

function pctText(pct: number | null): string {
  return pct === null ? '—' : `${pct}%`;
}

function engineLine(engine: IdeEngine, t: Translate): string {
  const holder =
    engine.holder === 'here'
      ? t('Engine: running in this window')
      : engine.holder === 'other'
        ? engine.pid === null
          ? t('Engine: running in another window or instance')
          : t('Engine: running in another window or instance (PID {0})', engine.pid)
        : t('Engine: stopped');
  const switching = engine.switching ? t('automatic switching on') : t('automatic switching off');
  return `${holder} · ${switching}`;
}

/** Testo, tooltip e livello; senza riepilogo (nessuna lettura ancora) è il caricamento. */
export function statusModel(
  summary: IdeStatus | undefined,
  t: Translate,
  formatTime: (ms: number) => string,
): StatusModel {
  if (!summary) {
    return { text: '$(sync~spin) SwapDeck', tooltip: t('SwapDeck: loading…'), level: 'unknown' };
  }
  const engine = engineLine(summary.engine, t);
  const content = summary.content;
  if (content.kind === 'error') {
    return {
      text: '$(warning) SwapDeck',
      tooltip: [
        t('SwapDeck: cswap not available'),
        content.error?.message ?? t('unknown error'),
        engine,
      ].join('\n'),
      level: 'error',
    };
  }
  const updated = t('Updated at {0}', formatTime(summary.updatedAtMs));
  if (content.kind === 'noAccount') {
    return {
      text: '$(account) SwapDeck: —',
      tooltip: [t('No active account'), engine, updated].join('\n'),
      level: summary.level,
    };
  }
  const { account, fiveHour: five, sevenDay: seven, lastKnown } = content;
  const stale = lastKnown ? '~' : '';
  const name = accountLabel(account);
  const text = `$(account) ${name} 5h ${stale}${pctText(five.pct)} · 7d ${stale}${pctText(seven.pct)}`;
  const lines = [`SwapDeck — ${name} (${account.shortEmail})`];
  lines.push(
    five.countdown
      ? t('5h: {0} (resets in {1})', pctText(five.pct), five.countdown)
      : t('5h: {0}', pctText(five.pct)),
  );
  lines.push(
    seven.expectedPct !== null
      ? t('7d: {0} (expected {1})', pctText(seven.pct), pctText(seven.expectedPct))
      : t('7d: {0}', pctText(seven.pct)),
  );
  if (lastKnown) lines.push(t('Last known data (current usage unavailable)'));
  lines.push(engine);
  if (summary.engine.activeMap) lines.push(t('Map: {0}', summary.engine.activeMap));
  lines.push(updated);
  return { text, tooltip: lines.join('\n'), level: summary.level };
}
