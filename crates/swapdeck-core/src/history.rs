//! Storico (F1): un file JSONL al giorno (UTC) in `<dati>/history/AAAA-MM-GG.jsonl`.
//!
//! Righe: campioni di consumo per account (solo quando cambia `usageFetchedAt` o lo stato),
//! cambi di account e decisioni del motore. Lo scrive solo chi tiene il lock del motore;
//! la lettura per intervallo (interfaccia, estensione) salta le righe troncate o illeggibili.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use time::{Date, Duration as TimeDuration, Month};
use ts_rs::TS;

use crate::cswap::{Account, ListResponse, UsageStatus};
use crate::engine::{Decision, SwitchCause};
use crate::lock::LockGuard;
use crate::paths::create_private_dir;
use crate::timeutil::{day_start, utc_date};

pub const HISTORY_DIR: &str = "history";
pub const DEFAULT_RETENTION_DAYS: u32 = 30;
/// Intervallo massimo leggibile in una volta.
const MAX_RANGE_DAYS: i64 = 400;
/// File cancellati al massimo per rotazione (una al giorno).
pub const MAX_PRUNE_PER_CALL: usize = 5;
/// Intervallo di default di [`History::events`] (come l'interfaccia): 7 giorni.
pub const DEFAULT_EVENTS_SECS: i64 = 7 * 86_400;
/// Cambi e decisioni restituiti al massimo da [`History::events`], per tipo.
pub const MAX_EVENTS: usize = 5_000;

/// Filtro dello storico chiesto dall'interfaccia.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    /// Solo questo account; senza, tutti.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account: Option<u32>,
    /// Inizio dell'intervallo in secondi Unix; default: 7 giorni fa.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub since_seconds: Option<i64>,
}

/// Cambi di account e decisioni del motore, dal più vecchio al più recente.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEvents {
    pub switches: Vec<SwitchRecord>,
    pub decisions: Vec<DecisionRecord>,
}

/// Consumo di un account in un istante.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageSample {
    #[ts(type = "number")]
    pub ts: i64,
    pub account: u32,
    pub active: bool,
    pub status: UsageStatus,
    pub five_hour_pct: Option<f64>,
    pub seven_day_pct: Option<f64>,
    pub five_hour_resets_at: Option<String>,
    pub seven_day_resets_at: Option<String>,
    /// Ritmo atteso sulla settimana (`expectedPct`).
    pub seven_day_expected_pct: Option<f64>,
    /// `usageFetchedAt` di cswap: quando il dato è stato letto davvero.
    pub fetched_at: Option<String>,
}

/// Cambio di account tentato dal motore.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SwitchRecord {
    #[ts(type = "number")]
    pub ts: i64,
    pub from: Option<u32>,
    pub to: u32,
    pub cause: SwitchCause,
    pub switched: bool,
    /// Esito di cswap (`reason` o errore ripulito).
    pub message: String,
}

/// Decisione del motore (registrata solo quando cambia).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DecisionRecord {
    #[ts(type = "number")]
    pub ts: i64,
    pub decision: Decision,
    pub dry_run: bool,
}

/// Una riga dello storico.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum HistoryRecord {
    Sample(UsageSample),
    Switch(SwitchRecord),
    Decision(DecisionRecord),
}

impl HistoryRecord {
    pub fn ts(&self) -> i64 {
        match self {
            Self::Sample(s) => s.ts,
            Self::Switch(s) => s.ts,
            Self::Decision(d) => d.ts,
        }
    }
}

impl UsageSample {
    pub fn from_account(a: &Account, ts: i64) -> Self {
        let u = a.usage.as_ref();
        let five = u.and_then(|u| u.five_hour.as_ref());
        let seven = u.and_then(|u| u.seven_day.as_ref());
        Self {
            ts,
            account: a.number,
            active: a.active,
            status: a.usage_status,
            five_hour_pct: five.map(|w| w.pct),
            seven_day_pct: seven.map(|w| w.pct),
            five_hour_resets_at: five.and_then(|w| w.resets_at.clone()),
            seven_day_resets_at: seven.and_then(|w| w.resets_at.clone()),
            seven_day_expected_pct: seven.and_then(|w| w.expected_pct),
            fetched_at: a.usage_fetched_at.clone(),
        }
    }
}

/// Cartella dello storico.
#[derive(Debug, Clone)]
pub struct History {
    dir: PathBuf,
}

fn file_name(date: Date) -> String {
    format!(
        "{:04}-{:02}-{:02}.jsonl",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}

fn parse_file_name(name: &str) -> Option<Date> {
    let stem = name.strip_suffix(".jsonl")?;
    let b = stem.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let year: i32 = stem.get(0..4)?.parse().ok()?;
    let month: u8 = stem.get(5..7)?.parse().ok()?;
    let day: u8 = stem.get(8..10)?.parse().ok()?;
    Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()
}

impl History {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            dir: data_dir.join(HISTORY_DIR),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Righe con `from <= ts <= to`, in ordine di file e di scrittura.
    pub fn read_range(&self, from: i64, to: i64) -> io::Result<Vec<HistoryRecord>> {
        let (Some(first), Some(last)) = (utc_date(from), utc_date(to)) else {
            return Ok(Vec::new());
        };
        if last < first || (last - first).whole_days() > MAX_RANGE_DAYS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "intervallo dello storico non valido",
            ));
        }
        let mut out = Vec::new();
        let mut day = first;
        loop {
            if let Some(text) = read_day(&self.dir.join(file_name(day)))? {
                out.extend(
                    text.lines()
                        .filter_map(|l| serde_json::from_str::<HistoryRecord>(l).ok())
                        .filter(|r| (from..=to).contains(&r.ts())),
                );
            }
            match day.next_day() {
                Some(next) if next <= last => day = next,
                _ => break,
            }
        }
        Ok(out)
    }

    /// Cambi e decisioni da `query.since_seconds` (default 7 giorni, mai prima della
    /// conservazione di [`DEFAULT_RETENTION_DAYS`] giorni né dopo `now`) fino a `now`, dal più
    /// vecchio, al massimo [`MAX_EVENTS`] per tipo (i più recenti). Con `account`, solo i cambi
    /// da o verso quell'account; le decisioni restano tutte.
    pub fn events(&self, query: &HistoryQuery, now: i64) -> io::Result<HistoryEvents> {
        let oldest = now - i64::from(DEFAULT_RETENTION_DAYS) * 86_400;
        let from = query
            .since_seconds
            .unwrap_or(now - DEFAULT_EVENTS_SECS)
            .clamp(oldest, now);
        let mut events = HistoryEvents::default();
        for record in self.read_range(from, now)? {
            match record {
                HistoryRecord::Switch(s)
                    if query
                        .account
                        .is_none_or(|a| s.to == a || s.from == Some(a)) =>
                {
                    events.switches.push(s);
                }
                HistoryRecord::Decision(d) => events.decisions.push(d),
                _ => {}
            }
        }
        events.switches.sort_by_key(|s| s.ts);
        events.decisions.sort_by_key(|d| d.ts);
        let keep_last = |n: usize| n.saturating_sub(MAX_EVENTS);
        events.switches.drain(..keep_last(events.switches.len()));
        events.decisions.drain(..keep_last(events.decisions.len()));
        Ok(events)
    }

    /// Cancella i file più vecchi di `retention_days` giorni, al massimo
    /// [`MAX_PRUNE_PER_CALL`] per chiamata e dai più vecchi: un salto dell'orologio non
    /// svuota lo storico in un colpo. Restituisce quanti.
    pub fn prune(&self, now: i64, retention_days: u32) -> io::Result<usize> {
        let Some(today) = utc_date(now) else {
            return Ok(0);
        };
        let cutoff = today - TimeDuration::days(i64::from(retention_days.max(1)));
        let entries = match fs::read_dir(&self.dir) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(0),
            other => other?,
        };
        let mut old: Vec<(Date, PathBuf)> = entries
            .flatten()
            .filter_map(|entry| {
                let date = entry.file_name().to_str().and_then(parse_file_name)?;
                // Solo file regolari: i link simbolici non si seguono (si rimuove il link).
                let removable = entry
                    .file_type()
                    .is_ok_and(|t| t.is_file() || t.is_symlink());
                (date < cutoff && removable).then(|| (date, entry.path()))
            })
            .collect();
        old.sort();
        old.truncate(MAX_PRUNE_PER_CALL);
        for (_, path) in &old {
            fs::remove_file(path)?;
        }
        Ok(old.len())
    }

    /// Apre la scrittura: richiede il lock del motore.
    pub fn writer(&self, lock: &LockGuard, retention_days: u32) -> io::Result<HistoryWriter> {
        let _ = lock;
        create_private_dir(&self.dir)?;
        Ok(HistoryWriter {
            history: self.clone(),
            retention_days,
            last_seen: HashMap::new(),
            pruned_day: None,
            primed: false,
            last_prune_error: None,
            pending_prune_error: None,
        })
    }
}

/// Testo di un giorno (`None` se manca). Rifiuta link simbolici.
fn read_day(path: &Path) -> io::Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
        Ok(m) if !m.file_type().is_file() => return Ok(None),
        Ok(_) => {}
    }
    let mut buf = Vec::new();
    match File::open(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        other => other?.read_to_end(&mut buf)?,
    };
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

fn open_append(path: &Path) -> io::Result<File> {
    if fs::symlink_metadata(path).is_ok_and(|m| !m.file_type().is_file()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file dello storico non regolare",
        ));
    }
    let mut opts = OpenOptions::new();
    opts.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    opts.open(path)
}

/// Scrittore dello storico, tenuto da chi ha il lock.
#[derive(Debug)]
pub struct HistoryWriter {
    history: History,
    retention_days: u32,
    /// Ultimo (stato, usageFetchedAt) registrato per account.
    last_seen: HashMap<u32, (UsageStatus, Option<String>)>,
    pruned_day: Option<Date>,
    primed: bool,
    /// Ultimo errore di rotazione visto e quello non ancora segnalato (uno per tipo).
    last_prune_error: Option<String>,
    pending_prune_error: Option<String>,
}

impl HistoryWriter {
    /// Errore della rotazione non ancora segnalato (lo registra il runner).
    pub fn take_prune_error(&mut self) -> Option<String> {
        self.pending_prune_error.take()
    }

    /// Aggiunge una riga al file del giorno di `rec.ts()`.
    pub fn append(&mut self, rec: &HistoryRecord) -> io::Result<()> {
        let ts = rec.ts();
        let day = utc_date(ts)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "data fuori scala"))?;
        if self.pruned_day != Some(day) {
            // Una rotazione fallita non ferma la scrittura: si annota e si riprova alla prossima.
            match self.history.prune(ts, self.retention_days) {
                Ok(_) => self.pruned_day = Some(day),
                Err(e) => {
                    let msg = e.to_string();
                    if self.last_prune_error.as_deref() != Some(msg.as_str()) {
                        self.last_prune_error = Some(msg.clone());
                        self.pending_prune_error = Some(msg);
                    }
                }
            }
        }
        let mut file = open_append(&self.history.dir.join(file_name(day)))?;
        let mut line = serde_json::to_string(rec).map_err(io::Error::other)?;
        line.push('\n');
        // Riga precedente troncata (crash a metà): si va a capo per non incollarle.
        let len = file.metadata()?.len();
        if len > 0 {
            let mut last = [0u8; 1];
            file.seek(SeekFrom::Start(len - 1))?;
            file.read_exact(&mut last)?;
            if last[0] != b'\n' {
                line.insert(0, '\n');
            }
        }
        file.write_all(line.as_bytes())?;
        file.flush()
    }

    /// Registra i campioni nuovi di `list`; restituisce quanti.
    pub fn record_samples(&mut self, list: &ListResponse, now: i64) -> io::Result<usize> {
        if !self.primed {
            self.primed = true;
            self.prime(now);
        }
        let mut written = 0;
        for a in &list.accounts {
            let key = (a.usage_status, a.usage_fetched_at.clone());
            if self.last_seen.get(&a.number) == Some(&key) {
                continue;
            }
            self.append(&HistoryRecord::Sample(UsageSample::from_account(a, now)))?;
            self.last_seen.insert(a.number, key);
            written += 1;
        }
        Ok(written)
    }

    /// Dopo un riavvio riprende gli ultimi campioni di oggi, per non duplicarli.
    fn prime(&mut self, now: i64) {
        let from = utc_date(now).map_or(now, day_start);
        if let Ok(rows) = self.history.read_range(from, now) {
            for r in rows {
                if let HistoryRecord::Sample(s) = r {
                    self.last_seen.insert(s.account, (s.status, s.fetched_at));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tests::{acc, list};
    use crate::lock::{Acquire, try_acquire};

    const DAY: i64 = 86_400;
    const T0: i64 = 1_800_000_000 - 1_800_000_000 % DAY + 3600;

    fn guard(dir: &Path) -> LockGuard {
        match try_acquire(dir, 60, T0).unwrap() {
            Acquire::Acquired(g) => g,
            Acquire::Busy(_) => panic!("lock occupato"),
        }
    }

    fn switch(ts: i64) -> HistoryRecord {
        HistoryRecord::Switch(SwitchRecord {
            ts,
            from: Some(6),
            to: 3,
            cause: SwitchCause::PrimaryAtLimit,
            switched: true,
            message: "switched".to_owned(),
        })
    }

    #[test]
    fn writes_per_day_and_reads_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let g = guard(dir.path());
        let h = History::new(dir.path());
        let mut w = h.writer(&g, 30).unwrap();
        for i in 0..3 {
            w.append(&switch(T0 + i * DAY)).unwrap();
        }
        let files = fs::read_dir(h.dir()).unwrap().count();
        assert_eq!(files, 3);
        assert_eq!(h.read_range(T0, T0 + 3 * DAY).unwrap().len(), 3);
        assert_eq!(h.read_range(T0 + 1, T0 + DAY).unwrap().len(), 1);
        assert!(h.read_range(T0, T0 + 500 * DAY).is_err());
    }

    #[test]
    fn tolerates_truncated_lines() {
        let dir = tempfile::tempdir().unwrap();
        let g = guard(dir.path());
        let h = History::new(dir.path());
        let mut w = h.writer(&g, 30).unwrap();
        w.append(&switch(T0)).unwrap();
        let path = h.dir().join(file_name(utc_date(T0).unwrap()));
        // Crash a metà riga, con un byte UTF-8 spezzato.
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{\"type\":\"switch\",\"ts\":1,\"messa\xc3")
            .unwrap();
        drop(f);
        w.append(&switch(T0 + 10)).unwrap();
        let rows = h.read_range(T0, T0 + 20).unwrap();
        assert_eq!(rows, vec![switch(T0), switch(T0 + 10)]);
    }

    #[test]
    fn samples_only_when_fetched_at_changes() {
        let dir = tempfile::tempdir().unwrap();
        let g = guard(dir.path());
        let h = History::new(dir.path());
        let mut a6 = acc(6, 50.0, 30.0);
        a6.usage_fetched_at = Some("2027-01-15T08:00:00Z".to_owned());
        let l = list(Some(6), vec![a6.clone(), acc(1, 0.0, 10.0)]);
        let mut w = h.writer(&g, 30).unwrap();
        assert_eq!(w.record_samples(&l, T0).unwrap(), 2);
        assert_eq!(w.record_samples(&l, T0 + 60).unwrap(), 0);
        a6.usage_fetched_at = Some("2027-01-15T08:03:00Z".to_owned());
        let l2 = list(Some(6), vec![a6, acc(1, 0.0, 10.0)]);
        assert_eq!(w.record_samples(&l2, T0 + 180).unwrap(), 1);
        // Dopo un riavvio non si duplicano i campioni già scritti oggi.
        let mut w2 = h.writer(&g, 30).unwrap();
        assert_eq!(w2.record_samples(&l2, T0 + 240).unwrap(), 0);
        let samples = h.read_range(T0, T0 + 300).unwrap();
        assert_eq!(samples.len(), 3);
        assert!(matches!(&samples[0], HistoryRecord::Sample(s) if s.five_hour_pct == Some(50.0)));
    }

    #[test]
    fn prunes_old_days_only() {
        let dir = tempfile::tempdir().unwrap();
        let g = guard(dir.path());
        let h = History::new(dir.path());
        let mut w = h.writer(&g, 30).unwrap();
        w.append(&switch(T0 - 40 * DAY)).unwrap();
        w.append(&switch(T0 - 29 * DAY)).unwrap();
        fs::write(h.dir().join("note.txt"), "x").unwrap();
        w.append(&switch(T0)).unwrap();
        let mut names: Vec<String> = fs::read_dir(h.dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names.len(), 3, "{names:?}");
        assert!(names.contains(&"note.txt".to_owned()));
        assert_eq!(parse_file_name("2026-02-30.jsonl"), None);
    }

    #[test]
    fn clock_jump_prunes_a_few_files_per_day() {
        let dir = tempfile::tempdir().unwrap();
        let h = History::new(dir.path());
        create_private_dir(h.dir()).unwrap();
        for d in 0..8 {
            let day = utc_date(T0 - d * DAY).unwrap();
            fs::write(h.dir().join(file_name(day)), "").unwrap();
        }
        // Orologio avanti di un anno: tutto sarebbe da cancellare, si tolgono solo i più vecchi.
        assert_eq!(h.prune(T0 + 365 * DAY, 30).unwrap(), MAX_PRUNE_PER_CALL);
        let left = fs::read_dir(h.dir()).unwrap().count();
        assert_eq!(left, 8 - MAX_PRUNE_PER_CALL);
        assert!(h.dir().join(file_name(utc_date(T0).unwrap())).exists());
        // Orologio indietro: niente da cancellare.
        assert_eq!(h.prune(T0 - 100 * DAY, 30).unwrap(), 0);
    }

    #[test]
    fn failed_rotation_is_reported_not_propagated() {
        let dir = tempfile::tempdir().unwrap();
        let g = guard(dir.path());
        let h = History::new(dir.path());
        let mut w = h.writer(&g, 30).unwrap();
        w.append(&switch(T0 - 40 * DAY)).unwrap();
        let old = h.dir().join(file_name(utc_date(T0 - 40 * DAY).unwrap()));
        // Il file di oggi esiste già: su Unix la cartella diventa di sola lettura.
        fs::write(h.dir().join(file_name(utc_date(T0).unwrap())), "").unwrap();
        let hold = block_removal(&old);
        if hold.is_none() {
            return; // root su Linux: la rimozione non si può impedire.
        }
        w.append(&switch(T0)).unwrap();
        assert!(w.take_prune_error().is_some());
        assert_eq!(w.take_prune_error(), None);
        assert_ne!(
            w.pruned_day,
            utc_date(T0),
            "si riprova alla prossima scrittura"
        );
        restore_removal(&old, hold);
        w.append(&switch(T0 + 1)).unwrap();
        assert_eq!(w.pruned_day, utc_date(T0));
        assert!(!old.exists());
    }

    /// Impedisce la rimozione di `path`: handle senza condivisione (Windows) o cartella in
    /// sola lettura (Unix, non per root).
    #[cfg(windows)]
    fn block_removal(path: &Path) -> Option<File> {
        use std::os::windows::fs::OpenOptionsExt;
        OpenOptions::new().read(true).share_mode(0).open(path).ok()
    }

    #[cfg(windows)]
    fn restore_removal(_path: &Path, hold: Option<File>) {
        drop(hold);
    }

    #[cfg(unix)]
    fn block_removal(path: &Path) -> Option<()> {
        use std::os::unix::fs::PermissionsExt;
        if rustix::process::geteuid().is_root() {
            return None;
        }
        let dir = path.parent()?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o500)).ok()
    }

    #[cfg(unix)]
    fn restore_removal(path: &Path, _hold: Option<()>) {
        use std::os::unix::fs::PermissionsExt;
        let dir = path.parent().unwrap();
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
    }
}

#[cfg(test)]
mod events_tests {
    use super::*;
    use crate::engine::{IdleReason, SwitchCause};

    fn switch(ts: i64, from: u32, to: u32) -> String {
        serde_json::to_string(&HistoryRecord::Switch(SwitchRecord {
            ts,
            from: Some(from),
            to,
            cause: SwitchCause::PrimaryAtLimit,
            switched: true,
            message: String::new(),
        }))
        .unwrap()
    }

    #[test]
    fn events_filter_clamp_and_order() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::new(dir.path());
        fs::create_dir_all(history.dir()).unwrap();
        let now = 1_800_000_000;
        let decision = serde_json::to_string(&HistoryRecord::Decision(DecisionRecord {
            ts: now - 50,
            decision: serde_json::from_value(serde_json::json!({
                "kind": "noAction", "reason": IdleReason::BelowThreshold
            }))
            .unwrap(),
            dry_run: false,
        }))
        .unwrap();
        let lines = [
            switch(now - 10, 3, 6),
            switch(now - 100, 6, 3),
            switch(now - 40 * 86_400, 6, 1),
            decision,
        ];
        for (i, line) in lines.iter().enumerate() {
            let ts = [now - 10, now - 100, now - 40 * 86_400, now - 50][i];
            let file = history.dir().join(file_name(utc_date(ts).unwrap()));
            let mut f = OpenOptions::new().create(true).append(true).open(file).unwrap();
            writeln!(f, "{line}").unwrap();
        }
        let all = history
            .events(
                &HistoryQuery {
                    account: None,
                    since_seconds: Some(0),
                },
                now,
            )
            .unwrap();
        let ts: Vec<i64> = all.switches.iter().map(|s| s.ts).collect();
        assert_eq!(ts, [now - 100, now - 10], "ordine e limite di conservazione");
        assert_eq!(all.decisions.len(), 1);
        let only1 = history
            .events(
                &HistoryQuery {
                    account: Some(1),
                    since_seconds: None,
                },
                now,
            )
            .unwrap();
        assert!(only1.switches.is_empty());
        let future = history
            .events(
                &HistoryQuery {
                    account: None,
                    since_seconds: Some(now + 1000),
                },
                now,
            )
            .unwrap();
        assert!(future.switches.is_empty() && future.decisions.is_empty());
    }
}
