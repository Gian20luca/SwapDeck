//! Lock unico del motore (F11): `engine.lock` nella cartella dati.
//!
//! Il file contiene PID, host, istanza, inizio del processo e battito. Sullo stesso host
//! è scaduto solo se il PID non esiste più o appartiene a un altro processo (PID riusato:
//! l'ora di avvio non coincide); tra host diversi dopo tre intervalli senza battito.
//!
//! - Acquisizione: `create_new` (atomica). Per subentrare a un lock scaduto si prende
//!   prima `engine.lock.takeover` (anch'esso `create_new`), si rilegge il lock e lo si
//!   rimuove solo se è ancora quello giudicato scaduto.
//! - Battito: verifica che il lock sia ancora nostro e lo riscrive con una rinomina atomica.
//! - Rilascio: all'uscita (`Drop`), solo se il lock è ancora nostro.

use std::collections::hash_map::RandomState;
use std::fs::{self, File, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const LOCK_FILE: &str = "engine.lock";
const TAKEOVER_FILE: &str = "engine.lock.takeover";
/// Un file di subentro più vecchio di così è di un processo morto a metà.
const TAKEOVER_STALE: Duration = Duration::from_secs(30);
/// Un lock illeggibile (scrittura in corso) si considera vivo per questo tempo.
const UNREADABLE_GRACE: Duration = Duration::from_secs(60);
/// Battiti mancati prima della scadenza.
pub const MISSED_BEATS: i64 = 3;
/// Tolleranza sull'ora di avvio del processo (secondi).
const START_TOLERANCE: u64 = 2;
/// Dimensione massima letta dal file di lock.
const MAX_LOCK_BYTES: u64 = 4096;

/// Contenuto del file di lock (letto anche dall'interfaccia).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LockInfo {
    pub pid: u32,
    pub host: String,
    pub instance: String,
    /// Avvio del processo (secondi Unix, da sysinfo): distingue un PID riusato.
    #[ts(type = "number | null")]
    pub process_start: Option<u64>,
    #[ts(type = "number")]
    pub acquired_at: i64,
    #[ts(type = "number")]
    pub heartbeat_at: i64,
    pub interval_secs: u32,
}

impl LockInfo {
    /// `true` se il lock non va più rispettato. Sullo stesso host decide il processo: vivo con
    /// la stessa ora di avvio = mai scaduto (anche se un giro lungo ha saltato il battito),
    /// morto o PID riusato = scaduto subito. Il battito decide solo tra host diversi o se
    /// l'ora di avvio non è nota.
    pub fn is_stale(&self, now: i64, host: &str) -> bool {
        if self.host.eq_ignore_ascii_case(host) {
            match (process_start(self.pid), self.process_start) {
                (None, _) => return true,
                (Some(start), Some(s)) => return s.abs_diff(start) > START_TOLERANCE,
                (Some(_), None) => {}
            }
        }
        let ttl = MISSED_BEATS * i64::from(self.interval_secs.max(1));
        now > self.heartbeat_at.saturating_add(ttl)
    }
}

/// Esito di [`try_acquire`].
#[derive(Debug)]
pub enum Acquire {
    Acquired(LockGuard),
    /// Un'altra istanza tiene il lock (`None` se il file è in scrittura o illeggibile).
    Busy(Option<LockInfo>),
}

/// Lock tenuto da questa istanza; rilasciato al `Drop`.
#[derive(Debug)]
pub struct LockGuard {
    path: PathBuf,
    info: LockInfo,
    released: bool,
}

/// Nome dell'host corrente.
pub fn host_name() -> String {
    sysinfo::System::host_name().unwrap_or_else(|| "localhost".to_owned())
}

/// Ora di avvio di un processo vivo, `None` se non esiste.
fn process_start(pid: u32) -> Option<u64> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = Pid::from_u32(pid);
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    sys.process(pid).map(sysinfo::Process::start_time)
}

fn new_instance_id() -> String {
    let mut h = RandomState::new().build_hasher();
    h.write_u32(std::process::id());
    h.write_u128(
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    format!("{:016x}", h.finish())
}

/// Legge il lock corrente (per l'interfaccia e per i controlli). `Ok(None)` se non c'è.
pub fn read(dir: &Path) -> io::Result<Option<LockInfo>> {
    match read_file(&dir.join(LOCK_FILE)) {
        Ok(Some(bytes)) => Ok(serde_json::from_slice(&bytes).ok()),
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Contenuto del file, `None` se manca. Rifiuta link simbolici e file enormi.
fn read_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
        Ok(m) if !m.file_type().is_file() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "il lock non è un file regolare",
            ));
        }
        Ok(_) => {}
    }
    let file = match File::open(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        other => other?,
    };
    let mut buf = Vec::new();
    file.take(MAX_LOCK_BYTES).read_to_end(&mut buf)?;
    Ok(Some(buf))
}

fn file_age(path: &Path) -> Option<Duration> {
    let m = fs::symlink_metadata(path).ok()?;
    SystemTime::now().duration_since(m.modified().ok()?).ok()
}

fn create_new(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
}

/// Prova a prendere il lock in `dir` (che deve esistere).
pub fn try_acquire(dir: &Path, interval_secs: u32, now: i64) -> io::Result<Acquire> {
    let path = dir.join(LOCK_FILE);
    let info = LockInfo {
        pid: std::process::id(),
        host: host_name(),
        instance: new_instance_id(),
        process_start: process_start(std::process::id()),
        acquired_at: now,
        heartbeat_at: now,
        interval_secs,
    };
    // Due giri: il secondo dopo aver rimosso un lock scaduto.
    for _ in 0..2 {
        match create_new(&path) {
            Ok(mut f) => {
                let body = serde_json::to_vec(&info).map_err(io::Error::other)?;
                if let Err(e) = f.write_all(&body).and_then(|()| f.sync_all()) {
                    drop(f);
                    let _ = fs::remove_file(&path);
                    return Err(e);
                }
                return Ok(Acquire::Acquired(LockGuard {
                    path,
                    info,
                    released: false,
                }));
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let current = read_file(&path)?;
        let parsed: Option<LockInfo> = current
            .as_deref()
            .and_then(|b| serde_json::from_slice(b).ok());
        let stale = match (&current, &parsed) {
            (None, _) => continue, // rimosso nel frattempo: si riprova
            (Some(_), Some(p)) => p.is_stale(now, &info.host),
            (Some(_), None) => file_age(&path).is_some_and(|a| a > UNREADABLE_GRACE),
        };
        if !stale {
            return Ok(Acquire::Busy(parsed));
        }
        if !remove_if_unchanged(dir, &path, current.as_deref().unwrap_or_default())? {
            return Ok(Acquire::Busy(None));
        }
    }
    Ok(Acquire::Busy(read(dir).ok().flatten()))
}

/// Rimuove il lock scaduto sotto il file di subentro, solo se il contenuto è ancora `seen`.
fn remove_if_unchanged(dir: &Path, path: &Path, seen: &[u8]) -> io::Result<bool> {
    let takeover = dir.join(TAKEOVER_FILE);
    let guard = match create_new(&takeover) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            if file_age(&takeover).is_some_and(|a| a > TAKEOVER_STALE) {
                let _ = fs::remove_file(&takeover);
            }
            return Ok(false);
        }
        Err(e) => return Err(e),
    };
    let result = match read_file(path) {
        Ok(Some(now_bytes)) if now_bytes == seen => fs::remove_file(path).map(|()| true),
        Ok(None) => Ok(true),
        Ok(Some(_)) => Ok(false),
        Err(e) => Err(e),
    };
    drop(guard);
    let _ = fs::remove_file(&takeover);
    result
}

impl LockGuard {
    pub fn info(&self) -> &LockInfo {
        &self.info
    }

    /// `true` se il file contiene ancora la nostra istanza.
    pub fn still_owned(&self) -> bool {
        matches!(
            read_file(&self.path),
            Ok(Some(b)) if serde_json::from_slice::<LockInfo>(&b)
                .is_ok_and(|i| i.instance == self.info.instance)
        )
    }

    /// Aggiorna il battito. Errore `PermissionDenied` se il lock è passato ad altri.
    pub fn heartbeat(&mut self, now: i64) -> io::Result<()> {
        if !self.still_owned() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "lock passato a un'altra istanza",
            ));
        }
        let mut next = self.info.clone();
        next.heartbeat_at = now;
        let dir = self.path.parent().unwrap_or(Path::new("."));
        let mut tmp = tempfile::Builder::new()
            .prefix(".engine.lock.")
            .tempfile_in(dir)?;
        tmp.write_all(&serde_json::to_vec(&next).map_err(io::Error::other)?)?;
        tmp.as_file().sync_all()?;
        tmp.persist(&self.path).map_err(|e| e.error)?;
        self.info = next;
        Ok(())
    }

    /// Rilascia subito il lock.
    pub fn release(mut self) {
        self.release_inner();
    }

    fn release_inner(&mut self) {
        if !self.released {
            self.released = true;
            if self.still_owned() {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        self.release_inner();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    const NOW: i64 = 1_800_000_000;

    fn acquired(a: Acquire) -> LockGuard {
        match a {
            Acquire::Acquired(g) => g,
            Acquire::Busy(i) => panic!("lock occupato: {i:?}"),
        }
    }

    fn write_lock(dir: &Path, info: &LockInfo) {
        fs::write(dir.join(LOCK_FILE), serde_json::to_vec(info).unwrap()).unwrap();
    }

    fn foreign(pid: u32, host: &str, heartbeat_at: i64) -> LockInfo {
        LockInfo {
            pid,
            host: host.to_owned(),
            instance: "altra".to_owned(),
            process_start: process_start(pid),
            acquired_at: heartbeat_at,
            heartbeat_at,
            interval_secs: 60,
        }
    }

    #[test]
    fn second_instance_is_busy_until_release() {
        let dir = tempfile::tempdir().unwrap();
        let g = acquired(try_acquire(dir.path(), 60, NOW).unwrap());
        assert!(g.still_owned());
        match try_acquire(dir.path(), 60, NOW + 10).unwrap() {
            Acquire::Busy(Some(i)) => assert_eq!(i.instance, g.info().instance),
            other => panic!("atteso Busy, ottenuto {other:?}"),
        }
        drop(g);
        assert!(!dir.path().join(LOCK_FILE).exists(), "rilascio all'uscita");
        acquired(try_acquire(dir.path(), 60, NOW + 20).unwrap());
    }

    #[test]
    fn threads_race_one_winner() {
        let dir = tempfile::tempdir().unwrap();
        // Lock scaduto di un host remoto: tutti provano a subentrare insieme.
        write_lock(dir.path(), &foreign(1, "altro-host", NOW - 1000));
        let n = 8;
        let start = Arc::new(Barrier::new(n));
        let done = Arc::new(Barrier::new(n));
        let handles: Vec<_> = (0..n)
            .map(|_| {
                let d = dir.path().to_path_buf();
                let (s, e) = (Arc::clone(&start), Arc::clone(&done));
                std::thread::spawn(move || {
                    s.wait();
                    let got = try_acquire(&d, 60, NOW).unwrap();
                    // Il vincitore tiene il lock finché tutti hanno provato.
                    e.wait();
                    match got {
                        Acquire::Acquired(g) => i32::from(g.still_owned()),
                        Acquire::Busy(_) => 0,
                    }
                })
            })
            .collect();
        let winners: i32 = handles.into_iter().map(|h| h.join().unwrap()).sum();
        assert_eq!(winners, 1);
    }

    #[test]
    fn stale_by_heartbeat_dead_pid_and_reused_pid() {
        let host = host_name();
        let me = std::process::id();
        // Battito recente su un altro host: vivo.
        assert!(!foreign(me, "altro-host", NOW - 100).is_stale(NOW, &host));
        // Tre intervalli senza battito: scaduto.
        assert!(foreign(me, "altro-host", NOW - 181).is_stale(NOW, &host));
        // Stesso host, processo vivo con la stessa ora di avvio: vivo.
        assert!(!foreign(me, &host, NOW).is_stale(NOW, &host));
        // Stesso host e stesso processo, battito saltato da un giro lungo: ancora vivo.
        assert!(!foreign(me, &host, NOW - 10_000).is_stale(NOW, &host));
        // Stesso host senza ora di avvio: decide il battito.
        let mut no_start = foreign(me, &host, NOW - 181);
        no_start.process_start = None;
        assert!(no_start.is_stale(NOW, &host));
        // Stesso host, PID riusato da un altro processo: scaduto subito.
        let mut reused = foreign(me, &host, NOW);
        reused.process_start = reused.process_start.map(|s| s - 1000);
        assert!(reused.is_stale(NOW, &host));
        // Stesso host, PID inesistente: scaduto subito.
        let dead = foreign(u32::MAX - 7, &host, NOW);
        assert!(dead.is_stale(NOW, &host));
    }

    #[test]
    fn takes_over_dead_lock_and_detects_loss() {
        let dir = tempfile::tempdir().unwrap();
        write_lock(dir.path(), &foreign(u32::MAX - 7, &host_name(), NOW));
        let mut g = acquired(try_acquire(dir.path(), 60, NOW).unwrap());
        g.heartbeat(NOW + 60).unwrap();
        assert_eq!(read(dir.path()).unwrap().unwrap().heartbeat_at, NOW + 60);
        // Un'altra istanza subentra (lock giudicato scaduto): il battito se ne accorge.
        write_lock(dir.path(), &foreign(1, "altro-host", NOW + 61));
        assert_eq!(
            g.heartbeat(NOW + 120).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        drop(g);
        assert!(dir.path().join(LOCK_FILE).exists(), "il lock altrui resta");
    }

    #[test]
    fn unreadable_lock_is_busy_while_young() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(LOCK_FILE), b"{tronc").unwrap();
        assert!(matches!(
            try_acquire(dir.path(), 60, NOW).unwrap(),
            Acquire::Busy(None)
        ));
    }

    /// Due processi veri: il figlio (questo stesso test binario) trova il lock occupato.
    #[test]
    fn second_process_is_busy() {
        if let Ok(d) = std::env::var("SWAPDECK_LOCK_CHILD_DIR") {
            let busy = matches!(
                try_acquire(Path::new(&d), 60, crate::timeutil::now_unix()).unwrap(),
                Acquire::Busy(Some(_))
            );
            std::process::exit(if busy { 0 } else { 9 });
        }
        let dir = tempfile::tempdir().unwrap();
        let _g = acquired(try_acquire(dir.path(), 60, crate::timeutil::now_unix()).unwrap());
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "lock::tests::second_process_is_busy",
                "--nocapture",
            ])
            .env("SWAPDECK_LOCK_CHILD_DIR", dir.path())
            .status()
            .unwrap();
        assert!(
            status.success(),
            "il secondo processo doveva trovare il lock occupato"
        );
    }
}
