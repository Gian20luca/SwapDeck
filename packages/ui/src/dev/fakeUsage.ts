import type {
  OtherDevicesEstimate,
  TokenCounts,
  UsageBreakdown,
  UsageQuery,
  UsageRow,
  UsageTotals,
} from '@swapdeck/types';

/** Pesi dei token equivalenti, come `usage::WEIGHTS` del Rust. */
const WEIGHTS = [1, 5, 1.25, 0.1] as const;

interface FakeMessage {
  /** Secondi prima di «ora». */
  ago: number;
  /** `null` = utenza sconosciuta. */
  account: number | null;
  project: string;
  session: string;
  agent?: { id: string; type: string; description: string };
  model: string;
  tokens: TokenCounts;
}

const t = (input: number, output: number, cacheCreation: number, cacheRead: number) => ({
  input,
  output,
  cacheCreation,
  cacheRead,
});

/** Consumi sintetici: progetti, sessioni, sotto-agenti, modelli diversi e un tratto senza storico. */
const MESSAGES: FakeMessage[] = [
  {
    ago: 600,
    account: 1,
    project: 'c--Progetti-SwapDeck',
    session: 'sess-a1',
    model: 'claude-opus-4-8',
    tokens: t(1200, 9000, 40000, 900000),
  },
  {
    ago: 1800,
    account: 1,
    project: 'c--Progetti-SwapDeck',
    session: 'sess-a1',
    agent: { id: 'ag1', type: 'Explore', description: 'Cerca nel codice' },
    model: 'claude-haiku-4-5',
    tokens: t(800, 3000, 20000, 150000),
  },
  {
    ago: 2400,
    account: 1,
    project: 'c--Progetti-SwapDeck',
    session: 'sess-a1',
    agent: { id: 'ag2', type: 'revisore', description: 'Revisione del diff' },
    model: 'claude-opus-4-8',
    tokens: t(500, 6000, 30000, 400000),
  },
  {
    ago: 7200,
    account: 1,
    project: 'c--Progetti-Sito',
    session: 'sess-b1',
    model: 'claude-sonnet-4-6',
    tokens: t(900, 4000, 25000, 300000),
  },
  {
    ago: 3 * 3600,
    account: 2,
    project: 'c--Progetti-Sito',
    session: 'sess-b2',
    model: 'claude-sonnet-4-6',
    tokens: t(600, 2500, 12000, 120000),
  },
  {
    ago: 2 * 86400,
    account: 1,
    project: 'c--Progetti-SwapDeck',
    session: 'sess-a0',
    model: 'claude-opus-4-8',
    tokens: t(2000, 15000, 60000, 1500000),
  },
  {
    ago: 3 * 86400,
    account: null,
    project: 'c--Progetti-Vecchio',
    session: 'sess-c0',
    model: 'claude-opus-4-8',
    tokens: t(1000, 5000, 20000, 500000),
  },
];

const equivalent = (c: TokenCounts) =>
  c.input * WEIGHTS[0] +
  c.output * WEIGHTS[1] +
  c.cacheCreation * WEIGHTS[2] +
  c.cacheRead * WEIGHTS[3];

/** Come `source::project_label` del Rust (senza `cwd`): ultimo pezzo dopo il trattino. */
function projectLabel(project: string): string {
  return (
    project
      .split('-')
      .reverse()
      .find((s) => s !== '') ?? '?'
  );
}

function emptyTotals(): UsageTotals {
  return { tokens: t(0, 0, 0, 0), equivalent: 0, messages: 0 };
}

function addTo(totals: UsageTotals, c: TokenCounts) {
  totals.tokens.input += c.input;
  totals.tokens.output += c.output;
  totals.tokens.cacheCreation += c.cacheCreation;
  totals.tokens.cacheRead += c.cacheRead;
  totals.equivalent += equivalent(c);
  totals.messages += 1;
}

function estimates(account: number | undefined, now: number): OtherDevicesEstimate[] {
  const base = {
    from: null,
    to: null,
    serverPct: null,
    localPct: null,
    otherPct: null,
    otherEquivalent: null,
    tokensPerPoint: null,
    confidence: null,
    samples: 0,
    historyGaps: false,
  };
  const all: OtherDevicesEstimate[] = [
    // Utenza 1: stima disponibile sulle 5 ore (fiducia media), 7 giorni ancora da calibrare.
    {
      ...base,
      account: 1,
      window: '5h',
      status: 'available',
      from: now - 3 * 3600,
      to: now - 60,
      serverPct: 42,
      localPct: 30.5,
      otherPct: 11.5,
      otherEquivalent: 115000,
      tokensPerPoint: 10000,
      confidence: 'medium',
      samples: 6,
    },
    {
      ...base,
      account: 1,
      window: '7d',
      status: 'notCalibrated',
      from: now - 4 * 86400,
      to: now - 60,
      serverPct: 18,
      samples: 1,
    },
    // Utenza 2: stima con storico lacunoso (fiducia bassa), nessuna lettura della settimana.
    {
      ...base,
      account: 2,
      window: '5h',
      status: 'available',
      from: now - 4 * 3600,
      to: now - 300,
      serverPct: 9,
      localPct: 9,
      otherPct: 0,
      otherEquivalent: 0,
      tokensPerPoint: 8000,
      confidence: 'low',
      samples: 3,
      historyGaps: true,
    },
    { ...base, account: 2, window: '7d', status: 'noData' },
  ];
  return account === undefined ? all : all.filter((e) => e.account === account);
}

/** Classifica come `usage_breakdown` del Rust, su dati sintetici. */
export function fakeUsageBreakdown(query: UsageQuery, nowMs: number): UsageBreakdown {
  const now = Math.floor(nowMs / 1000);
  let from: number;
  let to = now;
  if (query.window === 'custom') {
    if (
      query.fromSeconds === undefined ||
      query.toSeconds === undefined ||
      query.fromSeconds >= query.toSeconds
    ) {
      throw new Error('custom richiede fromSeconds e toSeconds');
    }
    from = query.fromSeconds;
    to = Math.min(query.toSeconds, now);
  } else {
    from = now - (query.window === '5h' ? 5 * 3600 : 7 * 86400);
  }
  const rows = new Map<string, UsageRow>();
  const total = emptyTotals();
  const unattributed = emptyTotals();
  for (const m of MESSAGES) {
    const ts = now - m.ago;
    if (ts < from || ts > to) continue;
    if (m.account === null) addTo(unattributed, m.tokens);
    if (query.account !== undefined && m.account !== query.account) continue;
    addTo(total, m.tokens);
    let row: UsageRow;
    switch (query.groupBy) {
      case 'project':
        row = {
          key: m.project,
          project: m.project,
          projectLabel: projectLabel(m.project),
          totals: emptyTotals(),
          sharePct: 0,
          lastUsed: 0,
        };
        break;
      case 'session':
        row = {
          key: `${m.project}/${m.session}`,
          project: m.project,
          projectLabel: projectLabel(m.project),
          session: m.session,
          totals: emptyTotals(),
          sharePct: 0,
          lastUsed: 0,
        };
        break;
      case 'agent':
        row = m.agent
          ? {
              key: `agent:${m.agent.id}`,
              project: m.project,
              projectLabel: projectLabel(m.project),
              session: m.session,
              agent: m.agent.id,
              agentType: m.agent.type,
              description: m.agent.description,
              totals: emptyTotals(),
              sharePct: 0,
              lastUsed: 0,
            }
          : {
              key: `main:${m.project}/${m.session}`,
              project: m.project,
              projectLabel: projectLabel(m.project),
              session: m.session,
              totals: emptyTotals(),
              sharePct: 0,
              lastUsed: 0,
            };
        break;
      default:
        row = { key: m.model, model: m.model, totals: emptyTotals(), sharePct: 0, lastUsed: 0 };
    }
    const current = rows.get(row.key) ?? row;
    addTo(current.totals, m.tokens);
    current.lastUsed = Math.max(current.lastUsed, ts);
    rows.set(row.key, current);
  }
  const otherDevices = estimates(query.account, now);
  const own = otherDevices.find(
    (e) => e.account === query.account && e.window === query.window && e.status === 'available',
  );
  const other = own?.otherEquivalent ?? null;
  const denominator = total.equivalent + (other ?? 0);
  const pct = (v: number) => (denominator > 0 ? Math.round((v / denominator) * 10000) / 100 : 0);
  const sorted = [...rows.values()]
    .map((r) => ({ ...r, sharePct: pct(r.totals.equivalent) }))
    .sort((a, b) => b.totals.equivalent - a.totals.equivalent || a.key.localeCompare(b.key));
  return {
    account: query.account ?? null,
    window: query.window,
    groupBy: query.groupBy,
    from,
    to,
    rows: sorted,
    truncatedRows: 0,
    total,
    unattributed,
    otherDevices,
    otherDevicesSharePct: other === null ? null : pct(other),
    sourceAvailable: true,
    scan: {
      files: 6,
      filesRead: 0,
      lines: 0,
      bytes: 0,
      messages: 0,
      skipped: 0,
      durationMs: 3,
      indexBytes: 0,
      indexReset: false,
      saveFailed: false,
      incomplete: false,
    },
  };
}
