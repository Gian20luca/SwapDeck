//! Indice incrementale dei consumi (`<dati>/usage/index.json`).
//!
//! Per ogni file di sessione: posizione letta, lunghezza, ora di modifica, identità e gli ultimi
//! id di messaggio (Claude Code ripete lo stesso messaggio su più righe, una per blocco: si conta
//! una volta sola, prendendo il massimo di ogni conteggio). Totali aggregati per ora, utenza,
//! sorgente (progetto, sessione, agente) e modello, più una serie al minuto per utenza e modello
//! che serve alla calibrazione. Nessun testo dei messaggi: solo numeri, nomi di cartella, id e la
//! descrizione dell'agente presa dai metadati. Scrittura atomica; un indice illeggibile o di
//! un'altra versione si ricostruisce da capo.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::TokenCounts;
use super::attribution::{LOOKBACK_SECS, Timeline};
use super::source::{self, AgentMeta, Parsed, SessionFile};
use crate::history::History;
use crate::paths::create_private_dir;

pub const USAGE_DIR: &str = "usage";
pub const INDEX_FILE: &str = "index.json";
pub const INDEX_VERSION: u32 = 1;
/// Id di messaggio ricordati per file (le righe dello stesso messaggio sono vicine).
const RECENT_IDS: usize = 16;
/// Dimensione massima dell'indice letto.
const MAX_INDEX_BYTES: u64 = 256 * 1024 * 1024;

/// Progetto, sessione e agente di un file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub project: String,
    pub session: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileState {
    pub source: u32,
    pub offset: u64,
    pub len: u64,
    pub mtime_ms: i64,
    pub identity: u64,
    /// Istante dell'ultimo messaggio contato.
    #[serde(default)]
    pub last_ts: i64,
    #[serde(default)]
    pub recent: Vec<(String, TokenCounts)>,
}

/// Totale di un gruppo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Agg {
    pub tokens: TokenCounts,
    pub last_ts: i64,
    pub messages: u32,
}

impl Agg {
    fn add(&mut self, tokens: TokenCounts, ts: i64, new_message: bool) {
        self.tokens.add(tokens);
        self.last_ts = self.last_ts.max(ts);
        self.messages += u32::from(new_message);
    }
}

/// (inizio dell'ora, utenza, sorgente, modello).
pub type HourKey = (i64, Option<u32>, u32, u32);
/// (inizio del minuto, utenza, modello).
pub type MinuteKey = (i64, Option<u32>, u32);

type StoredHour = (i64, Option<u32>, u32, u32, u64, u64, u64, u64, i64, u32);
type StoredMinute = (i64, Option<u32>, u32, u64, u64, u64, u64);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    version: u32,
    updated_at: i64,
    sources: Vec<Source>,
    models: Vec<String>,
    #[serde(default)]
    agents: BTreeMap<String, AgentMeta>,
    files: BTreeMap<String, FileState>,
    /// `[ora, utenza, sorgente, modello, input, output, cacheCreation, cacheRead, ultimo, messaggi]`.
    hours: Vec<StoredHour>,
    /// `[minuto, utenza, modello, input, output, cacheCreation, cacheRead]`.
    minutes: Vec<StoredMinute>,
}

/// Indice in memoria.
#[derive(Debug, Default, Clone)]
pub struct UsageIndex {
    pub updated_at: i64,
    pub sources: Vec<Source>,
    pub models: Vec<String>,
    pub agents: BTreeMap<String, AgentMeta>,
    pub files: BTreeMap<String, FileState>,
    pub hours: HashMap<HourKey, Agg>,
    pub minutes: HashMap<MinuteKey, TokenCounts>,
    source_ids: HashMap<Source, u32>,
    model_ids: HashMap<String, u32>,
}

/// Statistiche di una lettura (solo numeri).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ScanStats {
    /// File di sessione visti (modificati negli ultimi 30 giorni).
    pub files: u32,
    /// File letti perché cambiati.
    pub files_read: u32,
    #[ts(type = "number")]
    pub lines: u64,
    #[ts(type = "number")]
    pub bytes: u64,
    /// Messaggi con consumo aggiunti.
    #[ts(type = "number")]
    pub messages: u64,
    /// Righe illeggibili o troppo lunghe saltate.
    #[ts(type = "number")]
    pub skipped: u64,
    #[ts(type = "number")]
    pub duration_ms: u64,
    /// Dimensione dell'indice salvato.
    #[ts(type = "number")]
    pub index_bytes: u64,
    /// L'indice precedente era illeggibile o di un'altra versione ed è stato rifatto.
    pub index_reset: bool,
    /// Salvataggio dell'indice non riuscito (si riparte dalla lettura precedente).
    pub save_failed: bool,
    /// Elenco dei file incompleto (cartella illeggibile): i file non visti restano nell'indice.
    pub incomplete: bool,
}

impl UsageIndex {
    fn from_stored(s: Stored) -> Self {
        let mut idx = Self {
            updated_at: s.updated_at,
            sources: s.sources,
            models: s.models,
            agents: s.agents,
            files: s.files,
            ..Self::default()
        };
        idx.rebuild_ids();
        let (ns, nm) = (idx.sources.len() as u32, idx.models.len() as u32);
        for (h, a, src, m, i, o, cc, cr, last, n) in s.hours {
            if src < ns && m < nm {
                let tokens = TokenCounts::new(i, o, cc, cr);
                idx.hours.insert(
                    (h, a, src, m),
                    Agg {
                        tokens,
                        last_ts: last,
                        messages: n,
                    },
                );
            }
        }
        for (t, a, m, i, o, cc, cr) in s.minutes {
            if m < nm {
                idx.minutes
                    .insert((t, a, m), TokenCounts::new(i, o, cc, cr));
            }
        }
        idx.files.retain(|_, f| f.source < ns);
        idx
    }

    fn to_stored(&self) -> Stored {
        let mut hours: Vec<_> = self
            .hours
            .iter()
            .map(|(&(h, a, src, m), g)| {
                let t = g.tokens;
                (
                    h,
                    a,
                    src,
                    m,
                    t.input,
                    t.output,
                    t.cache_creation,
                    t.cache_read,
                    g.last_ts,
                    g.messages,
                )
            })
            .collect();
        hours.sort_unstable_by_key(|r| (r.0, r.1, r.2, r.3));
        let mut minutes: Vec<_> = self
            .minutes
            .iter()
            .map(|(&(t, a, m), c)| (t, a, m, c.input, c.output, c.cache_creation, c.cache_read))
            .collect();
        minutes.sort_unstable_by_key(|r| (r.0, r.1, r.2));
        Stored {
            version: INDEX_VERSION,
            updated_at: self.updated_at,
            sources: self.sources.clone(),
            models: self.models.clone(),
            agents: self.agents.clone(),
            files: self.files.clone(),
            hours,
            minutes,
        }
    }

    fn source_id(&mut self, s: Source) -> u32 {
        if let Some(&id) = self.source_ids.get(&s) {
            return id;
        }
        let id = self.sources.len() as u32;
        self.sources.push(s.clone());
        self.source_ids.insert(s, id);
        id
    }

    fn model_id(&mut self, m: &str) -> u32 {
        if let Some(&id) = self.model_ids.get(m) {
            return id;
        }
        let id = self.models.len() as u32;
        self.models.push(m.to_owned());
        self.model_ids.insert(m.to_owned(), id);
        id
    }

    /// Toglie le ore e i minuti prima di `cutoff` e le sorgenti e i modelli non più usati
    /// (rinumerati).
    pub fn prune(&mut self, cutoff: i64) {
        let hour_cut = cutoff - cutoff.rem_euclid(3600);
        self.hours.retain(|k, _| k.0 >= hour_cut);
        self.minutes
            .retain(|k, _| k.0 >= cutoff - cutoff.rem_euclid(60));
        let used_src: HashSet<u32> = self
            .hours
            .keys()
            .map(|k| k.2)
            .chain(self.files.values().map(|f| f.source))
            .collect();
        let used_models: HashSet<u32> = self
            .hours
            .keys()
            .map(|k| k.3)
            .chain(self.minutes.keys().map(|k| k.2))
            .collect();
        if used_src.len() == self.sources.len() && used_models.len() == self.models.len() {
            return;
        }
        let remap = |used: &HashSet<u32>, n: usize| -> Vec<Option<u32>> {
            let mut next = 0u32;
            (0..n as u32)
                .map(|i| {
                    used.contains(&i).then(|| {
                        next += 1;
                        next - 1
                    })
                })
                .collect()
        };
        let src_map = remap(&used_src, self.sources.len());
        let model_map = remap(&used_models, self.models.len());
        let sources = std::mem::take(&mut self.sources);
        self.sources = sources
            .into_iter()
            .enumerate()
            .filter(|(i, _)| src_map[*i].is_some())
            .map(|(_, s)| s)
            .collect();
        let models = std::mem::take(&mut self.models);
        self.models = models
            .into_iter()
            .enumerate()
            .filter(|(i, _)| model_map[*i].is_some())
            .map(|(_, m)| m)
            .collect();
        self.hours = std::mem::take(&mut self.hours)
            .into_iter()
            .filter_map(|((h, a, s, m), g)| {
                Some(((h, a, src_map[s as usize]?, model_map[m as usize]?), g))
            })
            .collect();
        self.minutes = std::mem::take(&mut self.minutes)
            .into_iter()
            .filter_map(|((t, a, m), c)| Some(((t, a, model_map[m as usize]?), c)))
            .collect();
        for f in self.files.values_mut() {
            f.source = src_map[f.source as usize].unwrap_or(0);
        }
        let agents: HashSet<&str> = self
            .sources
            .iter()
            .filter_map(|s| s.agent.as_deref())
            .collect();
        self.agents.retain(|k, _| agents.contains(k.as_str()));
        self.rebuild_ids();
    }

    fn rebuild_ids(&mut self) {
        self.source_ids = self
            .sources
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i as u32))
            .collect();
        self.model_ids = self
            .models
            .iter()
            .enumerate()
            .map(|(i, m)| (m.clone(), i as u32))
            .collect();
    }
}

/// Carica l'indice; `(indice vuoto, true)` se manca è illeggibile o di un'altra versione
/// (`false` solo se manca: primo avvio).
pub fn load(data_dir: &Path) -> (UsageIndex, bool) {
    let path = data_dir.join(USAGE_DIR).join(INDEX_FILE);
    let file = match source::open_regular(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return (UsageIndex::default(), false),
        Err(_) => return (UsageIndex::default(), true),
    };
    let mut buf = Vec::new();
    if file.take(MAX_INDEX_BYTES).read_to_end(&mut buf).is_err() {
        return (UsageIndex::default(), true);
    }
    match serde_json::from_slice::<Stored>(&buf) {
        Ok(s) if s.version == INDEX_VERSION => (UsageIndex::from_stored(s), false),
        _ => (UsageIndex::default(), true),
    }
}

/// Salva in modo atomico; restituisce i byte scritti.
pub fn save(data_dir: &Path, index: &UsageIndex) -> io::Result<u64> {
    let dir = data_dir.join(USAGE_DIR);
    create_private_dir(&dir)?;
    let json = serde_json::to_vec(&index.to_stored()).map_err(io::Error::other)?;
    let mut tmp = tempfile::Builder::new()
        .prefix(".index.")
        .suffix(".tmp")
        .tempfile_in(&dir)?;
    tmp.write_all(&json)?;
    tmp.as_file().sync_all()?;
    let target = dir.join(INDEX_FILE);
    let mut attempt = 0u32;
    loop {
        match tmp.persist(&target) {
            Ok(_) => return Ok(json.len() as u64),
            // Antivirus o indicizzatore con il file aperto (Windows): pochi tentativi.
            Err(e) if attempt < 4 && e.error.kind() == io::ErrorKind::PermissionDenied => {
                tmp = e.file;
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(25 << attempt));
            }
            Err(e) => return Err(e.error),
        }
    }
}

/// Legge le righe nuove dei file di sessione e aggiorna l'indice. L'utenza di ogni messaggio
/// viene dallo storico; `cutoff` è l'inizio della conservazione.
pub fn scan(
    index: &mut UsageIndex,
    projects: &Path,
    history: Option<&History>,
    now: i64,
    cutoff: i64,
) -> ScanStats {
    let started = Instant::now();
    let mut stats = ScanStats::default();
    let mut found = Vec::new();
    let complete = source::discover(projects, cutoff.saturating_mul(1000), &mut found);
    stats.incomplete = !complete;
    stats.files = found.len() as u32;

    let changed: Vec<&SessionFile> = found
        .iter()
        .filter(|f| {
            index
                .files
                .get(&f.rel)
                .is_none_or(|s| s.len != f.len || s.mtime_ms != f.mtime_ms)
        })
        .collect();

    if !changed.is_empty() {
        // Storico dall'istante più vecchio che può comparire nelle righe nuove.
        let earliest = changed
            .iter()
            .map(|f| {
                index
                    .files
                    .get(&f.rel)
                    .map_or(cutoff, |s| s.mtime_ms / 1000 - 3600)
            })
            .min()
            .unwrap_or(cutoff)
            .max(cutoff);
        let records = history
            .and_then(|h| h.read_range(earliest - LOOKBACK_SECS, now).ok())
            .unwrap_or_default();
        let timeline = Timeline::from_records(&records);
        for f in changed {
            read_file(index, f, &timeline, cutoff, &mut stats);
        }
    }

    if complete {
        let present: HashSet<&str> = found.iter().map(|f| f.rel.as_str()).collect();
        index.files.retain(|k, _| present.contains(k.as_str()));
    }
    index.prune(cutoff);
    index.updated_at = now;
    stats.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    stats
}

fn read_file(
    index: &mut UsageIndex,
    f: &SessionFile,
    timeline: &Timeline,
    cutoff: i64,
    stats: &mut ScanStats,
) {
    let Ok(mut file) = source::open_regular(&f.path) else {
        return;
    };
    let Ok(meta) = file.metadata() else {
        return;
    };
    let identity = source::identity(&meta);
    let len = meta.len();
    let mtime_ms = source::mtime_ms(&meta);
    let src = index.source_id(Source {
        project: f.project.clone(),
        session: f.session.clone(),
        agent: f.agent.clone(),
    });
    let mut state = match index.files.get(&f.rel) {
        Some(s) if s.identity == identity && s.offset <= len && s.source == src => s.clone(),
        // File nuovo, sostituito o accorciato: si rilegge da capo contando solo i messaggi
        // successivi all'ultimo già contato.
        previous => FileState {
            source: src,
            offset: 0,
            len: 0,
            mtime_ms: 0,
            identity,
            last_ts: previous.map_or(i64::MIN, |p| p.last_ts),
            recent: Vec::new(),
        },
    };
    let floor = if state.offset == 0 {
        state.last_ts
    } else {
        i64::MIN
    };
    if let Some(agent) = &f.agent {
        if !index.agents.contains_key(agent) {
            if let Some(m) = source::read_agent_meta(&f.path, agent) {
                index.agents.insert(agent.clone(), m);
            }
        }
    }
    let mut entries = Vec::new();
    let outcome = source::read_lines(
        &mut file,
        state.offset,
        len,
        |line| match source::parse_line(line) {
            Parsed::Usage(e) => {
                entries.push(e);
                true
            }
            Parsed::Other => true,
            Parsed::Invalid => false,
        },
    );
    let Ok(outcome) = outcome else {
        return;
    };
    stats.files_read += 1;
    stats.lines += outcome.lines;
    stats.bytes += outcome.bytes;
    stats.skipped += outcome.skipped;
    for e in entries {
        let (delta, new_message) = dedupe(&mut state.recent, e.id.as_deref(), e.tokens);
        if delta.total() == 0 || e.ts < cutoff || (new_message && e.ts <= floor) {
            continue;
        }
        state.last_ts = state.last_ts.max(e.ts);
        stats.messages += u64::from(new_message);
        let account = timeline.account_at(e.ts);
        let model = index.model_id(&e.model);
        let hour = e.ts - e.ts.rem_euclid(3600);
        index
            .hours
            .entry((hour, account, src, model))
            .or_default()
            .add(delta, e.ts, new_message);
        index
            .minutes
            .entry((e.ts - e.ts.rem_euclid(60), account, model))
            .or_default()
            .add(delta);
    }
    state.offset = outcome.offset;
    state.len = len;
    state.mtime_ms = mtime_ms;
    index.files.insert(f.rel.clone(), state);
}

/// Quota nuova di un messaggio: Claude Code scrive lo stesso messaggio (stesso id) su più righe
/// con conteggi uguali o crescenti. Restituisce l'aumento rispetto a quanto già contato e se il
/// messaggio è nuovo.
pub fn dedupe(
    recent: &mut Vec<(String, TokenCounts)>,
    id: Option<&str>,
    tokens: TokenCounts,
) -> (TokenCounts, bool) {
    let Some(id) = id else {
        return (tokens, true);
    };
    if let Some(pos) = recent.iter().position(|(k, _)| k == id) {
        let (_, seen) = &mut recent[pos];
        let merged = seen.max(tokens);
        let delta = merged.saturating_sub(*seen);
        *seen = merged;
        return (delta, false);
    }
    if recent.len() >= RECENT_IDS {
        recent.remove(0);
    }
    recent.push((id.to_owned(), tokens));
    (tokens, true)
}
