//! Utenza attiva in un istante, ricostruita dallo storico di SwapDeck.
//!
//! Eventi: battito del giro (`HeartbeatRecord`, utenza attiva o nessuna), campione con
//! `active: true` (quell'utenza è attiva), campione con `active: false` dell'utenza ritenuta attiva
//! (non si sa più quale lo sia) e cambio riuscito (`SwitchRecord` con `switched`). L'utenza in `t`
//! è quella dell'ultimo evento `<= t`, purché lo storico copra `t`: l'ultima riga di qualsiasi
//! tipo prima di `t` non deve essere più vecchia del silenzio ammesso, ricavato dall'intervallo
//! dell'ultimo battito ([`max_gap`]; senza battiti [`MAX_GAP_SECS`]). SwapDeck spento = storico
//! muto = utenza sconosciuta. Si usano solo righe passate: il risultato non cambia se lo si calcola
//! subito o più tardi. Un cambio fatto fuori da SwapDeck si vede al giro dopo.

use crate::history::HistoryRecord;

/// Silenzio massimo dello storico senza battiti (righe scritte prima di F20).
pub const MAX_GAP_SECS: i64 = 30 * 60;
/// Periodo di un giro quando nessun battito lo dice.
pub const DEFAULT_PERIOD_SECS: i64 = 300;
/// Storico letto prima dell'inizio dell'intervallo da attribuire.
pub const LOOKBACK_SECS: i64 = 6 * 3600;

/// Silenzio ammesso dopo una riga con l'intervallo `interval` del motore: due giri più due minuti
/// (lettura dei conflitti e `cswap list` lenti).
pub fn max_gap(interval: i64) -> i64 {
    2 * interval + 120
}

/// Linea temporale dell'utenza attiva.
#[derive(Debug, Default, Clone)]
pub struct Timeline {
    /// (istante, utenza attiva o `None` se non nota), in ordine di tempo.
    events: Vec<(i64, Option<u32>)>,
    /// (istante, intervallo del motore noto allora) di tutte le righe, in ordine.
    records: Vec<(i64, Option<i64>)>,
}

impl Timeline {
    pub fn from_records(records: &[HistoryRecord]) -> Self {
        let mut rows: Vec<&HistoryRecord> = records.iter().collect();
        rows.sort_by_key(|r| r.ts());
        let mut events: Vec<(i64, Option<u32>)> = Vec::new();
        let mut marks = Vec::with_capacity(rows.len());
        let mut current: Option<u32> = None;
        let mut interval: Option<i64> = None;
        for r in &rows {
            let next = match r {
                HistoryRecord::Heartbeat(h) => {
                    interval = Some(i64::from(h.interval_secs.max(1)));
                    Some(h.active)
                }
                HistoryRecord::Sample(s) if s.active => Some(Some(s.account)),
                HistoryRecord::Sample(s) if current == Some(s.account) => Some(None),
                HistoryRecord::Switch(s) if s.switched => Some(Some(s.to)),
                _ => None,
            };
            if let Some(next) = next {
                if next != current || events.is_empty() {
                    events.push((r.ts(), next));
                }
                current = next;
            }
            marks.push((r.ts(), interval));
        }
        Self {
            events,
            records: marks,
        }
    }

    fn gap_after(&self, i: usize) -> i64 {
        self.records[i].1.map_or(MAX_GAP_SECS, max_gap)
    }

    /// Lo storico copre `t` (riga recente prima di `t`).
    fn covered(&self, t: i64) -> bool {
        let n = self.records.partition_point(|&(r, _)| r <= t);
        n > 0 && t - self.records[n - 1].0 <= self.gap_after(n - 1)
    }

    /// Utenza attiva in `t`, se lo storico la conosce.
    pub fn account_at(&self, t: i64) -> Option<u32> {
        if !self.covered(t) {
            return None;
        }
        let i = self.events.partition_point(|&(ts, _)| ts <= t);
        if i == 0 { None } else { self.events[i - 1].1 }
    }

    /// Periodo di un giro in `t` (intervallo dell'ultimo battito, altrimenti
    /// [`DEFAULT_PERIOD_SECS`]).
    pub fn period_at(&self, t: i64) -> i64 {
        let n = self.records.partition_point(|&(r, _)| r <= t);
        n.checked_sub(1)
            .and_then(|i| self.records[i].1)
            .unwrap_or(DEFAULT_PERIOD_SECS)
    }

    /// `account` attiva e storico senza silenzi per tutto `[from, to]`, e righe fino a `to`
    /// almeno (l'intervallo è già stato testimoniato).
    pub fn active_throughout(&self, account: u32, from: i64, to: i64) -> bool {
        if from > to || self.records.last().is_none_or(|&(r, _)| r < to) {
            return false;
        }
        if self.account_at(from) != Some(account) {
            return false;
        }
        let first_event = self.events.partition_point(|&(ts, _)| ts <= from);
        if self.events[first_event..]
            .iter()
            .take_while(|&&(ts, _)| ts <= to)
            .any(|&(_, a)| a != Some(account))
        {
            return false;
        }
        // Nessun silenzio tra la riga prima di `from` e la prima riga da `to` in poi.
        let start = self.records.partition_point(|&(r, _)| r <= from) - 1;
        let mut i = start;
        while i + 1 < self.records.len() && self.records[i].0 < to {
            if self.records[i + 1].0 - self.records[i].0 > self.gap_after(i) {
                return false;
            }
            i += 1;
        }
        true
    }
}
