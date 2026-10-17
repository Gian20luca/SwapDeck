import type { UsageGroupBy, UsageRow } from '@swapdeck/types';

/**
 * Nome di progetto più leggibile: Claude Code chiama la cartella come il percorso con i separatori
 * cambiati in trattini (`c--Progetti-SwapDeck`, `-home-utente-progetto`). Si toglie solo il prefisso
 * dell'unità o il trattino iniziale; il resto resta com'è.
 */
export function prettyProject(name: string): string {
  const stripped = name.replace(/^[A-Za-z]--/, '').replace(/^-+/, '');
  return stripped === '' ? name : stripped;
}

/** Identificativo di sessione o agente accorciato (di solito un UUID). */
export function shortId(id: string, max = 8): string {
  return id.length > max + 1 ? `${id.slice(0, max)}…` : id;
}

export interface RowLabel {
  title: string;
  /** Seconda riga, più piccola (progetto, descrizione dell'agente). */
  sub?: string;
}

/** Testo di una riga della classifica secondo il raggruppamento; `main` = «Conversazione principale». */
export function rowLabel(row: UsageRow, groupBy: UsageGroupBy, main: string): RowLabel {
  const project = row.project === undefined ? undefined : prettyProject(row.project);
  switch (groupBy) {
    case 'project':
      return { title: project ?? row.key };
    case 'session':
      return {
        title: row.session === undefined ? row.key : shortId(row.session),
        ...(project ? { sub: project } : {}),
      };
    case 'agent': {
      const session = row.session === undefined ? undefined : shortId(row.session);
      const where = [project, session].filter(Boolean).join(' / ');
      if (row.agent === undefined) return { title: main, ...(where ? { sub: where } : {}) };
      const sub = [row.description, where].filter(Boolean).join(' · ');
      return { title: row.agentType ?? shortId(row.agent), ...(sub ? { sub } : {}) };
    }
    case 'model':
      return { title: row.model ?? row.key };
  }
}

/** Numero compatto per i token (`1,2 Mln`, `850`), nella lingua dell'interfaccia. */
export function formatTokens(value: number, locale: string): string {
  return new Intl.NumberFormat(locale, { notation: 'compact', maximumFractionDigits: 1 }).format(
    Math.round(value),
  );
}

/** Percentuale con al massimo un decimale. */
export function formatPct(value: number, locale: string): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value);
}
