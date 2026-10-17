//! Chi consuma (F20): consumi locali di Claude Code per progetto, sessione, agente e modello,
//! attribuiti all'utenza attiva in quel momento, più la stima di quanto consumano gli altri
//! dispositivi. Metodo e privacy in `docs/CHI-CONSUMA.md`.
//!
//! Le sessioni di Claude Code si leggono in sola lettura ([`source`]); l'indice incrementale
//! ([`index`]) tiene solo numeri e nomi di cartella; l'utenza viene dallo storico di SwapDeck
//! ([`attribution`]); la stima confronta l'aumento sul server con il consumo locale
//! ([`estimate`]).

pub mod attribution;
pub mod estimate;
pub mod index;
pub mod source;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use estimate::{Confidence, EstimateStatus, OtherDevicesEstimate};
pub use index::{ScanBudget, ScanProgress, ScanStats, UsageIndex};

use crate::history::{DEFAULT_RETENTION_DAYS, History, HistoryRecord, UsageSample};
use attribution::Timeline;

/// Pesi dei token equivalenti: input, output, scrittura e lettura della cache (rapporti dei
/// prezzi delle API rispetto all'input). La differenza tra modelli la assorbe la calibrazione
/// per modello.
pub const WEIGHTS: [f64; 4] = [1.0, 5.0, 1.25, 0.1];
/// Righe restituite al massimo.
pub const MAX_ROWS: usize = 200;

const FIVE_HOURS: i64 = 5 * 3600;
const SEVEN_DAYS: i64 = 7 * 86_400;

/// Conteggi di token.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TokenCounts {
    #[ts(type = "number")]
    pub input: u64,
    #[ts(type = "number")]
    pub output: u64,
    #[ts(type = "number")]
    pub cache_creation: u64,
    #[ts(type = "number")]
    pub cache_read: u64,
}

impl TokenCounts {
    pub fn new(input: u64, output: u64, cache_creation: u64, cache_read: u64) -> Self {
        Self {
            input,
            output,
            cache_creation,
            cache_read,
        }
    }

    pub fn total(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_creation)
            .saturating_add(self.cache_read)
    }

    pub fn add(&mut self, o: TokenCounts) {
        self.input = self.input.saturating_add(o.input);
        self.output = self.output.saturating_add(o.output);
        self.cache_creation = self.cache_creation.saturating_add(o.cache_creation);
        self.cache_read = self.cache_read.saturating_add(o.cache_read);
    }

    pub fn max(self, o: TokenCounts) -> Self {
        Self::new(
            self.input.max(o.input),
            self.output.max(o.output),
            self.cache_creation.max(o.cache_creation),
            self.cache_read.max(o.cache_read),
        )
    }

    pub fn saturating_sub(self, o: TokenCounts) -> Self {
        Self::new(
            self.input.saturating_sub(o.input),
            self.output.saturating_sub(o.output),
            self.cache_creation.saturating_sub(o.cache_creation),
            self.cache_read.saturating_sub(o.cache_read),
        )
    }
}

/// Finestra della classifica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum BreakdownWindow {
    #[serde(rename = "5h")]
    FiveHour,
    #[serde(rename = "7d")]
    SevenDay,
    #[serde(rename = "custom")]
    Custom,
}

/// Raggruppamento della classifica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum UsageGroupBy {
    Project,
    Session,
    Agent,
    Model,
}

/// Richiesta della classifica.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UsageQuery {
    /// Solo questa utenza; senza, tutte (compresa la sconosciuta).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account: Option<u32>,
    pub window: BreakdownWindow,
    /// Inizio e fine (secondi Unix) per `custom`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub from_seconds: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub to_seconds: Option<i64>,
    pub group_by: UsageGroupBy,
}

/// Totale di un insieme di messaggi.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    pub tokens: TokenCounts,
    /// Token equivalenti (pesi [`WEIGHTS`]).
    pub equivalent: f64,
    pub messages: u32,
}

impl UsageTotals {
    fn add(&mut self, g: &index::Agg) {
        self.tokens.add(g.tokens);
        self.equivalent += g.tokens.equivalent();
        self.messages = self.messages.saturating_add(g.messages);
    }
}

/// Riga della classifica. I campi descrittivi dipendono dal raggruppamento: `project` per
/// progetto e sessione, `session` per sessione e agente, `agent` (assente = conversazione
/// principale della sessione), `agentType` e `description` per agente, `model` per modello.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    /// Chiave unica nella classifica.
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project: Option<String>,
    /// Nome da mostrare: solo l'ultima cartella, dal `cwd` delle sessioni quando corrisponde alla
    /// cartella del progetto ([`source::project_label_from_cwd`]), altrimenti dal nome della
    /// cartella ([`source::project_label`]). `project`
    /// (nome della cartella di Claude Code, cioè il percorso intero) serve solo da chiave.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agent_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    pub totals: UsageTotals,
    /// Quota sul totale (locale più altri dispositivi stimati, se disponibili), in percento.
    pub share_pct: f64,
    /// Ultimo uso (secondi Unix).
    #[ts(type = "number")]
    pub last_used: i64,
}

/// Classifica dei consumi.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageBreakdown {
    pub account: Option<u32>,
    pub window: BreakdownWindow,
    pub group_by: UsageGroupBy,
    /// Intervallo (secondi Unix); le ore si contano intere (risoluzione dell'indice: un'ora).
    #[ts(type = "number")]
    pub from: i64,
    #[ts(type = "number")]
    pub to: i64,
    /// Dal più alto per token equivalenti.
    pub rows: Vec<UsageRow>,
    /// Righe oltre [`MAX_ROWS`] non restituite.
    pub truncated_rows: u32,
    /// Consumo locale delle righe (tutte, anche quelle tagliate).
    pub total: UsageTotals,
    /// Consumo dell'intervallo senza utenza nota (storico assente), mostrato a parte.
    pub unattributed: UsageTotals,
    /// Stime degli altri dispositivi per utenza e finestra del server (5h e 7d).
    pub other_devices: Vec<OtherDevicesEstimate>,
    /// Quota degli altri dispositivi sul totale, se l'utenza è scelta e la stima c'è.
    pub other_devices_share_pct: Option<f64>,
    /// Cartella delle sessioni di Claude Code trovata.
    pub source_available: bool,
    pub scan: ScanStats,
    /// Lettura fermata dal budget: righe ancora da leggere. La classifica è parziale; si richiama
    /// finché il campo non manca.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub partial: Option<ScanProgress>,
}

/// Dove leggere e dove tenere l'indice.
#[derive(Debug, Clone)]
pub struct UsagePaths {
    /// Cartella dati di SwapDeck (indice e storico); senza, indice solo in memoria.
    pub data_dir: Option<PathBuf>,
    /// Cartella `projects` di Claude Code.
    pub projects_dir: Option<PathBuf>,
}

impl UsagePaths {
    /// Cartella dati data e `projects` di Claude Code da [`source::claude_dir`].
    pub fn standard(data_dir: Option<PathBuf>) -> Self {
        Self {
            data_dir,
            projects_dir: source::claude_dir().map(|d| d.join("projects")),
        }
    }
}

/// Errore della classifica: richiesta non valida o lettura fallita.
#[derive(Debug, thiserror::Error)]
pub enum UsageError {
    #[error("{0}")]
    Invalid(String),
    #[error("storico non leggibile: {0}")]
    Io(#[from] io::Error),
}

/// Una lettura alla volta per processo (app e CLI possono leggere insieme: l'ultimo salvataggio
/// vince, ed entrambi partono dagli stessi file).
static SCAN_LOCK: Mutex<()> = Mutex::new(());

/// Legge le righe nuove entro il budget predefinito, aggiorna l'indice e restituisce la
/// classifica (con `partial` se restano righe da leggere: si richiama).
pub fn usage_breakdown(
    paths: &UsagePaths,
    query: &UsageQuery,
    now: i64,
) -> Result<UsageBreakdown, UsageError> {
    usage_breakdown_with(paths, query, now, ScanBudget::default())
}

/// Come [`usage_breakdown`], con il budget dato. Storico illeggibile: errore, prima di toccare
/// l'indice (niente salvataggio, posizioni ferme).
pub fn usage_breakdown_with(
    paths: &UsagePaths,
    query: &UsageQuery,
    now: i64,
    budget: ScanBudget,
) -> Result<UsageBreakdown, UsageError> {
    resolve_window(query, None, now)?;
    let _guard = SCAN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let cutoff = now - i64::from(DEFAULT_RETENTION_DAYS) * 86_400;
    let records = match paths.data_dir.as_deref() {
        Some(d) => History::new(d).read_range(cutoff - attribution::LOOKBACK_SECS, now),
        None => Ok(Vec::new()),
    };
    scan_and_rank(paths, query, now, budget, records)
}

/// Lettura e classifica con lo storico già letto (errore: niente lettura né salvataggio).
fn scan_and_rank(
    paths: &UsagePaths,
    query: &UsageQuery,
    now: i64,
    budget: ScanBudget,
    records: io::Result<Vec<HistoryRecord>>,
) -> Result<UsageBreakdown, UsageError> {
    let records = records?;
    let cutoff = now - i64::from(DEFAULT_RETENTION_DAYS) * 86_400;
    let (mut idx, reset) = match &paths.data_dir {
        Some(d) => index::load(d),
        None => (UsageIndex::default(), false),
    };
    let projects = paths.projects_dir.as_deref().filter(|p| p.is_dir());
    let before = (idx.files.len(), idx.hours.len());
    let mut stats = match projects {
        Some(p) => {
            let timeline = Timeline::from_records(&records);
            index::scan(&mut idx, p, &timeline, now, cutoff, budget)
        }
        None => {
            idx.prune(cutoff);
            ScanStats::default()
        }
    };
    stats.index_reset = reset;
    if let Some(d) = &paths.data_dir {
        let dirty = stats.files_read > 0 || reset || before != (idx.files.len(), idx.hours.len());
        if dirty {
            match index::save(d, &idx) {
                Ok(n) => stats.index_bytes = n,
                Err(_) => stats.save_failed = true,
            }
        }
    }
    let mut out = breakdown(&idx, &records, query, now)?;
    if let Some(p) = projects {
        apply_cwd_labels(p, &mut out.rows);
    }
    out.source_available = projects.is_some();
    out.partial = stats.partial;
    out.scan = stats;
    Ok(out)
}

/// Sostituisce `projectLabel` con l'ultima cartella del `cwd` quando le sessioni del progetto la
/// danno ([`source::project_label_from_cwd`]); una lettura per progetto, solo in memoria.
fn apply_cwd_labels(projects: &std::path::Path, rows: &mut [UsageRow]) {
    let mut labels: HashMap<String, Option<String>> = HashMap::new();
    for row in rows {
        let Some(project) = row.project.as_deref() else {
            continue;
        };
        let label = labels
            .entry(project.to_owned())
            .or_insert_with(|| source::project_label_from_cwd(projects, project));
        if let Some(l) = label {
            row.project_label = Some(l.clone());
        }
    }
}

/// Ultimo campione valido di un'utenza con il reset della finestra.
fn current_reset(
    samples: &[UsageSample],
    account: u32,
    window: BreakdownWindow,
    now: i64,
) -> Option<i64> {
    samples
        .iter()
        .rev()
        .filter(|s| s.account == account && s.status == crate::cswap::UsageStatus::Ok)
        .find_map(|s| match window {
            BreakdownWindow::FiveHour => s.five_hour_resets_at.as_deref(),
            BreakdownWindow::SevenDay => s.seven_day_resets_at.as_deref(),
            BreakdownWindow::Custom => None,
        })
        .and_then(crate::timeutil::parse_iso)
        .filter(|&r| r > now)
}

/// Intervallo della richiesta: `custom` con i suoi estremi (dentro la conservazione); 5h e 7d
/// finestra mobile, o la finestra corrente del server se l'utenza è scelta e il reset è noto.
fn resolve_window(
    query: &UsageQuery,
    reset: Option<i64>,
    now: i64,
) -> Result<(i64, i64), UsageError> {
    let oldest = now - i64::from(DEFAULT_RETENTION_DAYS) * 86_400;
    match query.window {
        BreakdownWindow::Custom => {
            let (Some(from), Some(to)) = (query.from_seconds, query.to_seconds) else {
                return Err(UsageError::Invalid(
                    "custom richiede fromSeconds e toSeconds".into(),
                ));
            };
            if from >= to {
                return Err(UsageError::Invalid("fromSeconds dopo toSeconds".into()));
            }
            Ok((from.max(oldest), to.min(now).max(from.max(oldest))))
        }
        w => {
            let len = if w == BreakdownWindow::FiveHour {
                FIVE_HOURS
            } else {
                SEVEN_DAYS
            };
            Ok((reset.map_or(now - len, |r| r - len).max(oldest), now))
        }
    }
}

/// Classifica da un indice e dai campioni dello storico (funzione pura, usata dai test).
pub fn breakdown(
    idx: &UsageIndex,
    history: &[HistoryRecord],
    query: &UsageQuery,
    now: i64,
) -> Result<UsageBreakdown, UsageError> {
    let samples: Vec<UsageSample> = history
        .iter()
        .filter_map(|r| match r {
            HistoryRecord::Sample(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    let samples = samples.as_slice();
    let timeline = Timeline::from_records(history);
    let reset = query
        .account
        .and_then(|a| current_reset(samples, a, query.window, now));
    let (from, to) = resolve_window(query, reset, now)?;
    let from_hour = from - from.rem_euclid(3600);

    let mut groups: HashMap<String, UsageRow> = HashMap::new();
    let mut total = UsageTotals::default();
    let mut unattributed = UsageTotals::default();
    for (&(hour, account, src, model), agg) in &idx.hours {
        if hour < from_hour || hour > to {
            continue;
        }
        if account.is_none() {
            unattributed.add(agg);
        }
        if query.account.is_some_and(|a| account != Some(a)) {
            continue;
        }
        let (Some(source), Some(model_name)) = (
            idx.sources.get(src as usize),
            idx.models.get(model as usize),
        ) else {
            continue;
        };
        total.add(agg);
        let mut row = UsageRow {
            key: String::new(),
            project: None,
            project_label: None,
            session: None,
            agent: None,
            agent_type: None,
            description: None,
            model: None,
            totals: UsageTotals::default(),
            share_pct: 0.0,
            last_used: 0,
        };
        row.key = match query.group_by {
            UsageGroupBy::Project => {
                row.project = Some(source.project.clone());
                row.project_label = Some(source::project_label(&source.project));
                source.project.clone()
            }
            UsageGroupBy::Session => {
                row.project = Some(source.project.clone());
                row.project_label = Some(source::project_label(&source.project));
                row.session = Some(source.session.clone());
                format!("{}/{}", source.project, source.session)
            }
            UsageGroupBy::Agent => {
                row.project = Some(source.project.clone());
                row.project_label = Some(source::project_label(&source.project));
                row.session = Some(source.session.clone());
                match &source.agent {
                    Some(a) => {
                        row.agent = Some(a.clone());
                        if let Some(m) = idx.agents.get(a) {
                            row.agent_type = m.agent_type.clone();
                            row.description = m.description.clone();
                        }
                        format!("agent:{a}")
                    }
                    None => format!("main:{}/{}", source.project, source.session),
                }
            }
            UsageGroupBy::Model => {
                row.model = Some(model_name.clone());
                model_name.clone()
            }
        };
        let entry = groups.entry(row.key.clone()).or_insert(row);
        entry.totals.add(agg);
        entry.last_used = entry.last_used.max(agg.last_ts);
    }

    let accounts: Vec<u32> = {
        let recent = now - SEVEN_DAYS - FIVE_HOURS;
        let mut a: Vec<u32> = samples
            .iter()
            .filter(|s| s.ts >= recent && query.account.is_none_or(|q| q == s.account))
            .map(|s| s.account)
            .collect();
        a.sort_unstable();
        a.dedup();
        a
    };
    let mut other_devices = Vec::new();
    for a in accounts {
        let own: Vec<&UsageSample> = samples.iter().filter(|s| s.account == a).collect();
        let series = estimate::MinuteSeries::for_account(idx, a);
        for w in [BreakdownWindow::FiveHour, BreakdownWindow::SevenDay] {
            other_devices.push(estimate::estimate(a, w, &own, &series, &timeline, now));
        }
    }
    let other_equivalent = query
        .account
        .and_then(|a| {
            other_devices
                .iter()
                .find(|e| e.account == a && e.window == query.window)
        })
        .filter(|e| e.status == EstimateStatus::Available)
        .and_then(|e| e.other_equivalent);
    let denominator = total.equivalent + other_equivalent.unwrap_or(0.0);
    let pct = |v: f64| {
        if denominator > 0.0 {
            (v / denominator * 10_000.0).round() / 100.0
        } else {
            0.0
        }
    };
    let mut rows: Vec<UsageRow> = groups
        .into_values()
        .map(|mut r| {
            r.share_pct = pct(r.totals.equivalent);
            r
        })
        .collect();
    rows.sort_by(|a, b| {
        b.totals
            .equivalent
            .total_cmp(&a.totals.equivalent)
            .then_with(|| a.key.cmp(&b.key))
    });
    let truncated_rows = rows.len().saturating_sub(MAX_ROWS) as u32;
    rows.truncate(MAX_ROWS);
    Ok(UsageBreakdown {
        account: query.account,
        window: query.window,
        group_by: query.group_by,
        from,
        to,
        rows,
        truncated_rows,
        total,
        unattributed,
        other_devices,
        other_devices_share_pct: other_equivalent.map(pct),
        source_available: true,
        scan: ScanStats::default(),
        partial: None,
    })
}
