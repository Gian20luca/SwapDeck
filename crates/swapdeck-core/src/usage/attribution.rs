//! Utenza attiva in un istante, ricostruita dallo storico di SwapDeck.
//!
//! Eventi: campione con `active: true` (quell'utenza è attiva), campione con `active: false`
//! dell'utenza ritenuta attiva (non si sa più quale lo sia) e cambio riuscito (`SwitchRecord` con
//! `switched`). L'utenza in `t` è quella dell'ultimo evento `<= t`, purché lo storico copra `t`:
//! l'ultima riga di qualsiasi tipo prima di `t` non deve essere più vecchia di [`MAX_GAP_SECS`]
//! (SwapDeck spento = storico muto = utenza sconosciuta). Si usano solo righe passate: il
//! risultato non cambia se lo si calcola subito o più tardi. Un cambio fatto fuori da SwapDeck si
//! vede al campione dopo: fino a quel momento il consumo va all'utenza di prima.

use crate::history::HistoryRecord;

/// Silenzio massimo dello storico prima di `t` perché l'attribuzione valga.
pub const MAX_GAP_SECS: i64 = 30 * 60;
/// Storico letto prima dell'inizio dell'intervallo da attribuire.
pub const LOOKBACK_SECS: i64 = 6 * 3600;

/// Linea temporale dell'utenza attiva.
#[derive(Debug, Default, Clone)]
pub struct Timeline {
    /// (istante, utenza attiva o `None` se non nota), in ordine di tempo.
    events: Vec<(i64, Option<u32>)>,
    /// Istanti di tutte le righe, in ordine.
    records: Vec<i64>,
}

impl Timeline {
    pub fn from_records(records: &[HistoryRecord]) -> Self {
        let mut rows: Vec<&HistoryRecord> = records.iter().collect();
        rows.sort_by_key(|r| r.ts());
        let mut events: Vec<(i64, Option<u32>)> = Vec::new();
        let mut current: Option<u32> = None;
        for r in &rows {
            let next = match r {
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
        }
        Self {
            events,
            records: rows.iter().map(|r| r.ts()).collect(),
        }
    }

    /// Utenza attiva in `t`, se lo storico la conosce.
    pub fn account_at(&self, t: i64) -> Option<u32> {
        let last_record = self.records.partition_point(|&r| r <= t);
        if last_record == 0 || t - self.records[last_record - 1] > MAX_GAP_SECS {
            return None;
        }
        let i = self.events.partition_point(|&(ts, _)| ts <= t);
        if i == 0 { None } else { self.events[i - 1].1 }
    }
}
