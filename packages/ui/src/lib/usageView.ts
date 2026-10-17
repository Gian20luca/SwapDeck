import type { UsageGroupBy, UsageRow } from '@swapdeck/types';

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
  // Solo il nome dato dal core: `project` (e la chiave che lo contiene) è il percorso intero.
  const project = row.projectLabel;
  switch (groupBy) {
    case 'project':
      return { title: project ?? '?' };
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
