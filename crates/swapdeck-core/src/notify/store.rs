//! Memoria persistente delle notifiche, mittente a giri ([`Notifier`]) e `notifications poll`
//! per chi non tiene il lock del motore (estensione VS Code).

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::{
    EngineContext, Notification, NotificationConfig, NotifyInput, NotifyMemory, NotifyPoll,
    NotifySender, SummaryState, evaluate, summary_state,
};
use crate::config::Language;
use crate::cswap::ListResponse;
use crate::history::History;
use crate::lock;
use crate::paths::{create_private_dir, persist_retrying};

/// Memoria nella cartella dati.
pub const MEMORY_FILE: &str = "notify-memory.json";
/// Esclusione tra due `notifications poll` contemporanei (due finestre di VS Code).
pub const POLL_LOCK_FILE: &str = "notify-poll.lock";
const MAX_MEMORY_BYTES: u64 = 1024 * 1024;
/// Un lock di poll più vecchio è di un processo morto a metà.
const POLL_LOCK_STALE: Duration = Duration::from_secs(30);
/// La versione installata di cswap si rilegge al massimo ogni 6 ore.
const INSTALLED_TTL: i64 = 6 * 3600;

/// Scostamento dell'ora locale da UTC all'istante `now` (0 se non determinabile).
pub fn local_offset_secs(now: i64) -> i32 {
    use chrono::{Offset as _, TimeZone as _};
    chrono::Local
        .timestamp_opt(now, 0)
        .single()
        .map_or(0, |d| d.offset().fix().local_minus_utc())
}

/// Memoria su file (o solo in memoria, senza cartella dati).
#[derive(Debug, Default)]
pub struct NotifyStore {
    dir: Option<PathBuf>,
    mem: NotifyMemory,
}

impl NotifyStore {
    pub fn new(dir: Option<&Path>) -> Self {
        Self {
            dir: dir.map(Path::to_path_buf),
            mem: NotifyMemory::default(),
        }
    }

    /// Rilegge la memoria: un altro mittente può averla aggiornata. File assente o rovinato
    /// = memoria vuota (al peggio un avviso ripetuto).
    pub fn load(&mut self) -> NotifyMemory {
        if let Some(dir) = &self.dir {
            self.mem = read_memory(&dir.join(MEMORY_FILE)).unwrap_or_default();
        }
        self.mem.clone()
    }

    /// Scrittura atomica (file temporaneo nella stessa cartella, poi rinomina).
    pub fn save(&mut self, mem: &NotifyMemory) -> io::Result<()> {
        self.mem = mem.clone();
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        create_private_dir(dir)?;
        let mut tmp = tempfile::Builder::new()
            .prefix(".notify-memory.")
            .suffix(".tmp")
            .tempfile_in(dir)?;
        tmp.write_all(&serde_json::to_vec(mem).map_err(io::Error::other)?)?;
        tmp.as_file().sync_all()?;
        persist_retrying(tmp, &dir.join(MEMORY_FILE))
    }
}

fn read_memory(path: &Path) -> Option<NotifyMemory> {
    let file = crate::config::open_no_follow(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_MEMORY_BYTES {
        return None;
    }
    let mut text = String::new();
    file.take(MAX_MEMORY_BYTES).read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

/// Mittente a giri: memoria dal file, storico solo quando serve il riepilogo, versione di
/// cswap dalla cache del modulo setup (nessuna chiamata di rete).
#[derive(Debug, Default)]
pub struct Notifier {
    store: NotifyStore,
    data_dir: Option<PathBuf>,
    /// Ultima lettura della versione installata: (istante, versione).
    installed: Option<(i64, Option<String>)>,
}

impl Notifier {
    pub fn new(data_dir: Option<&Path>) -> Self {
        Self {
            store: NotifyStore::new(data_dir),
            data_dir: data_dir.map(Path::to_path_buf),
            installed: None,
        }
    }

    /// Un giro. `installed` legge la versione di cswap (`cswap --version`): chiamato solo se
    /// la cache ha una versione su PyPI, al massimo ogni 6 ore.
    pub fn tick(
        &mut self,
        list: Option<&ListResponse>,
        engine: Option<EngineContext<'_>>,
        rules: &NotificationConfig,
        now: i64,
        installed: &mut dyn FnMut() -> Option<String>,
    ) -> Vec<Notification> {
        let mut mem = self.store.load();
        let offset = local_offset_secs(now);
        let history = match (summary_state(rules, &mem, now, offset), &self.data_dir) {
            (SummaryState::Due { from, to }, Some(dir)) => {
                History::new(dir).read_range(from, to).ok()
            }
            _ => None,
        };
        let update = match (&self.data_dir, rules.updates.enabled) {
            (Some(dir), true) => {
                let cached = &mut self.installed;
                crate::setup::update::cached(dir, || {
                    if let Some((at, v)) = cached
                        && now - *at < INSTALLED_TTL
                    {
                        return v.clone();
                    }
                    let v = installed();
                    *cached = Some((now, v.clone()));
                    v
                })
            }
            _ => None,
        };
        let input = NotifyInput {
            list,
            engine,
            history: history.as_deref(),
            update: update.as_ref(),
            local_offset_secs: offset,
        };
        let out = evaluate(&input, rules, &mut mem, now);
        if let Err(e) = self.store.save(&mem) {
            eprintln!("memoria delle notifiche non salvata: {e}");
        }
        out
    }
}

/// Esclusione di `notifications poll`: il file si toglie al drop.
#[derive(Debug)]
pub struct PollGuard {
    path: PathBuf,
}

impl PollGuard {
    /// `None` se un'altra richiesta è in corso.
    pub fn acquire(dir: &Path) -> io::Result<Option<Self>> {
        create_private_dir(dir)?;
        let path = dir.join(POLL_LOCK_FILE);
        for _ in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut f) => {
                    let _ = writeln!(f, "{}", std::process::id());
                    return Ok(Some(Self { path }));
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    let old = fs::symlink_metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| SystemTime::now().duration_since(t).ok())
                        .is_some_and(|age| age > POLL_LOCK_STALE);
                    if !old {
                        return Ok(None);
                    }
                    // Di un processo morto: si toglie e si riprova una volta.
                    let _ = fs::remove_file(&path);
                }
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}

impl Drop for PollGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// `true` se un'istanza viva tiene il lock del motore.
fn engine_alive(dir: &Path, now: i64) -> bool {
    matches!(lock::read(dir), Ok(Some(info)) if !info.is_stale(now, &lock::host_name()))
}

/// Notifiche per chi non tiene il lock del motore: solo se nessuna istanza viva lo tiene
/// (altrimenti manda lei) e una richiesta alla volta.
pub fn poll(
    dir: &Path,
    list: Option<&ListResponse>,
    rules: &NotificationConfig,
    language: Language,
    now: i64,
    installed: &mut dyn FnMut() -> Option<String>,
) -> io::Result<NotifyPoll> {
    let empty = |sender| NotifyPoll {
        sender,
        notifications: Vec::new(),
        language,
    };
    if engine_alive(dir, now) {
        return Ok(empty(NotifySender::Engine));
    }
    let Some(_guard) = PollGuard::acquire(dir)? else {
        return Ok(empty(NotifySender::Busy));
    };
    // Di nuovo con l'esclusione presa: il motore può essere partito nel frattempo.
    if engine_alive(dir, now) {
        return Ok(empty(NotifySender::Engine));
    }
    let notifications = Notifier::new(Some(dir)).tick(list, None, rules, now, installed);
    Ok(NotifyPoll {
        sender: NotifySender::Caller,
        notifications,
        language,
    })
}
