//! File di log di SwapDeck, per utente: `<cartella dati>/logs/swapdeck.log` (con `SWAPDECK_HOME`
//! la cartella dati è `SWAPDECK_HOME/data`). Una riga per evento:
//!
//! `2026-10-17T10:00:00.123Z INFO  desktop[1234] messaggio`
//!
//! - Scrivono l'app (`desktop`), `swapdeck --headless` (`headless`), `swapdeck-cli` (`cli`, oppure
//!   il valore di `SWAPDECK_LOG_SOURCE`, per esempio `vscode`). Più processi scrivono lo stesso
//!   file: ogni riga si aggiunge con una sola scrittura in append, aprendo e chiudendo il file.
//! - Rotazione per dimensione: oltre [`MAX_FILE_BYTES`] il file diventa `swapdeck.log.1`, i
//!   precedenti scalano fino a `.4` ([`MAX_FILES`] file in tutto). Tra processi la rotazione ha
//!   un turno (`swapdeck.log.lock`, creato in esclusiva): la fa uno solo.
//! - Livello minimo da `SWAPDECK_LOG` (`off`, `error`, `warn`, `info`, `debug`), di default `info`.
//! - Ogni messaggio passa da [`crate::redact::redact`]: mai token, segreti o email complete.
//! - Un errore di scrittura non ferma mai il chiamante: il log è un aiuto, non un requisito.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::paths::{AppPaths, create_private_dir};
use crate::redact::redact;
use crate::timeutil::format_iso;

/// Sottocartella dei log nella cartella dati.
pub const LOG_DIR: &str = "logs";
/// File attivo.
pub const LOG_FILE: &str = "swapdeck.log";
/// Dimensione oltre la quale il file ruota.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// File tenuti in tutto (attivo più `.1` … `.4`).
pub const MAX_FILES: usize = 5;
/// Livello minimo (`off`, `error`, `warn`, `info`, `debug`).
pub const LEVEL_ENV: &str = "SWAPDECK_LOG";
/// Nome del componente al posto di quello predefinito (es. `vscode` per la CLI dell'estensione).
pub const SOURCE_ENV: &str = "SWAPDECK_LOG_SOURCE";
/// Lunghezza massima di un messaggio (caratteri).
const MAX_MESSAGE_CHARS: usize = 2000;
/// Byte letti dalla coda di un file per [`tail`].
const TAIL_BYTES: u64 = 512 * 1024;
/// Turno di rotazione tra processi.
const ROTATION_LOCK: &str = "swapdeck.log.lock";
/// Un turno più vecchio di così è di un processo interrotto a metà.
const ROTATION_LOCK_STALE: Duration = Duration::from_secs(30);

/// Livello di una riga.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO ",
            Self::Warn => "WARN ",
            Self::Error => "ERROR",
        }
    }

    /// Valore di [`LEVEL_ENV`]: `Some(None)` = spento, `None` = valore non riconosciuto.
    pub fn parse_min(value: &str) -> Option<Option<Level>> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "0" => Some(None),
            "error" => Some(Some(Self::Error)),
            "warn" | "warning" => Some(Some(Self::Warn)),
            "info" => Some(Some(Self::Info)),
            "debug" | "trace" => Some(Some(Self::Debug)),
            _ => None,
        }
    }
}

/// Un file di log con rotazione.
#[derive(Debug)]
pub struct LogFile {
    dir: PathBuf,
    component: String,
    min: Option<Level>,
    max_bytes: u64,
    files: usize,
    /// Scritture e rotazione in fila dentro il processo.
    guard: Mutex<()>,
}

impl LogFile {
    /// Log in `dir` (creata alla prima scrittura) per `component`, livello `info`.
    pub fn new(dir: impl Into<PathBuf>, component: &str) -> Self {
        Self {
            dir: dir.into(),
            component: sanitize_component(component).unwrap_or_else(|| "swapdeck".to_owned()),
            min: Some(Level::Info),
            max_bytes: MAX_FILE_BYTES,
            files: MAX_FILES,
            guard: Mutex::new(()),
        }
    }

    /// Limiti diversi (test).
    pub fn with_limits(mut self, max_bytes: u64, files: usize) -> Self {
        self.max_bytes = max_bytes.max(1);
        self.files = files.max(1);
        self
    }

    /// Livello minimo; `None` spegne il log.
    pub fn with_level(mut self, min: Option<Level>) -> Self {
        self.min = min;
        self
    }

    /// Nome del componente nelle righe.
    pub fn component(&self) -> &str {
        &self.component
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Percorso del file attivo.
    pub fn path(&self) -> PathBuf {
        self.dir.join(LOG_FILE)
    }

    /// `true` se una riga di questo livello verrebbe scritta.
    pub fn enabled(&self, level: Level) -> bool {
        self.min.is_some_and(|min| level >= min)
    }

    /// Scrive una riga (redatta). Gli errori tornano al chiamante, che di norma li ignora.
    pub fn write(&self, level: Level, message: &str) -> io::Result<()> {
        if !self.enabled(level) {
            return Ok(());
        }
        let line = format_line(level, &self.component, std::process::id(), message);
        let _turn = self.guard.lock().unwrap_or_else(|p| p.into_inner());
        self.append(line.as_bytes())
    }

    fn append(&self, line: &[u8]) -> io::Result<()> {
        create_private_dir(&self.dir)?;
        let path = self.path();
        let mut file = open_append(&path)?;
        let size = file.metadata()?.len();
        if size > 0 && size + line.len() as u64 > self.max_bytes {
            drop(file);
            // Una rotazione fallita (file aperto da un antivirus, permessi) non perde la riga:
            // si continua sul file attivo e si riprova alla riga dopo.
            let _ = self.rotate(&path, line.len() as u64);
            file = open_append(&path)?;
        }
        // Una sola scrittura in append: con più processi le righe non si mescolano.
        file.write_all(line)
    }

    fn rotate(&self, live: &Path, incoming: u64) -> io::Result<()> {
        // Una rotazione alla volta tra i processi; chi non prende il turno scrive sul file attivo.
        let Some(_turn) = RotationLock::acquire(&self.dir)? else {
            return Ok(());
        };
        // Un altro processo ha già ruotato: il file attivo è di nuovo piccolo.
        match fs::symlink_metadata(live) {
            Ok(m)
                if m.file_type().is_file()
                    && m.len() > 0
                    && m.len() + incoming > self.max_bytes => {}
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        }
        if self.files == 1 {
            return fs::remove_file(live);
        }
        let numbered = |i: usize| self.dir.join(format!("{LOG_FILE}.{i}"));
        // `.3` → `.4`, …, `.1` → `.2`: il più vecchio si sovrascrive. Un errore lascia tutto
        // com'è (il file attivo cresce un poco e si riprova alla riga dopo).
        for i in (1..self.files - 1).rev() {
            match fs::rename(numbered(i), numbered(i + 1)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        // Chi ha ancora aperto il file attivo finisce la sua riga nel `.1`: niente righe perse.
        fs::rename(live, numbered(1))
    }
}

/// Turno di rotazione tra processi: `swapdeck.log.lock` creato in esclusiva, tolto al rilascio.
struct RotationLock(PathBuf);

impl RotationLock {
    fn acquire(dir: &Path) -> io::Result<Option<Self>> {
        let path = dir.join(ROTATION_LOCK);
        for _ in 0..2 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Some(Self(path))),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    // Lasciato da un processo interrotto a metà: si toglie e si riprova una volta.
                    let stale = fs::symlink_metadata(&path)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| SystemTime::now().duration_since(t).ok())
                        .is_some_and(|age| age > ROTATION_LOCK_STALE);
                    if !stale {
                        return Ok(None);
                    }
                    let _ = fs::remove_file(&path);
                }
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}

impl Drop for RotationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// Nome di componente ammesso: 1–24 caratteri tra lettere, cifre, `-` e `_`.
fn sanitize_component(name: &str) -> Option<String> {
    let ok = !name.is_empty()
        && name.len() <= 24
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    ok.then(|| name.to_owned())
}

/// Riga completa, terminata da `\n`.
pub fn format_line(level: Level, component: &str, pid: u32, message: &str) -> String {
    let mut msg = redact(message);
    if msg.chars().count() > MAX_MESSAGE_CHARS {
        msg = msg.chars().take(MAX_MESSAGE_CHARS).collect();
        msg.push('…');
    }
    format!(
        "{} {} {component}[{pid}] {msg}\n",
        timestamp(),
        level.label()
    )
}

/// Ora UTC con i millisecondi (`2026-10-17T10:00:00.123Z`).
fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = i64::try_from(now.as_secs()).unwrap_or(i64::MAX);
    let base = format_iso(secs);
    let base = base.strip_suffix('Z').unwrap_or(&base);
    format!("{base}.{:03}Z", now.subsec_millis())
}

fn open_append(path: &Path) -> io::Result<File> {
    if fs::symlink_metadata(path).is_ok_and(|m| !m.file_type().is_file()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file di log non regolare",
        ));
    }
    let mut opts = OpenOptions::new();
    opts.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    let file = opts.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file di log non regolare",
        ));
    }
    Ok(file)
}

fn open_read(path: &Path) -> io::Result<Option<File>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
        Ok(m) if !m.file_type().is_file() => return Ok(None),
        Ok(_) => {}
    }
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    match opts.open(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
        Ok(f) if f.metadata()?.is_file() => Ok(Some(f)),
        Ok(_) => Ok(None),
    }
}

/// Righe complete della coda di un file (al massimo [`TAIL_BYTES`]).
fn tail_of(path: &Path) -> io::Result<Vec<String>> {
    let Some(mut file) = open_read(path)? else {
        return Ok(Vec::new());
    };
    let len = file.metadata()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut buf)?;
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().collect();
    // Partendo a metà file la prima riga è troncata.
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    Ok(lines.into_iter().map(str::to_owned).collect())
}

/// Ultime `n` righe del log in `dir` (file attivo, poi `.1` se non bastano), di nuovo redatte.
pub fn tail(dir: &Path, n: usize) -> io::Result<Vec<String>> {
    let mut lines = tail_of(&dir.join(LOG_FILE))?;
    if lines.len() < n {
        let mut older = tail_of(&dir.join(format!("{LOG_FILE}.1")))?;
        older.append(&mut lines);
        lines = older;
    }
    let skip = lines.len().saturating_sub(n);
    Ok(lines.into_iter().skip(skip).map(|l| redact(&l)).collect())
}

/// Cartella dei log per dei percorsi dati.
pub fn logs_dir(paths: &AppPaths) -> PathBuf {
    paths.data_dir.join(LOG_DIR)
}

static GLOBAL: OnceLock<LogFile> = OnceLock::new();

/// Attiva il log del processo per `component` nella cartella dati standard (o `SWAPDECK_HOME`).
/// [`SOURCE_ENV`] sostituisce il nome, [`LEVEL_ENV`] il livello. Restituisce la cartella dei log;
/// `None` se le cartelle dell'utente non sono determinabili. Solo la prima chiamata conta.
pub fn init(component: &str) -> Option<PathBuf> {
    let paths = AppPaths::from_env_or_system()?;
    let dir = logs_dir(&paths);
    let name = std::env::var(SOURCE_ENV)
        .ok()
        .and_then(|s| sanitize_component(&s))
        .unwrap_or_else(|| component.to_owned());
    let level = std::env::var(LEVEL_ENV)
        .ok()
        .and_then(|v| Level::parse_min(&v))
        .unwrap_or(Some(Level::Info));
    let _ = GLOBAL.set(LogFile::new(&dir, &name).with_level(level));
    Some(dir)
}

/// Log del processo, se attivato con [`init`].
pub fn global() -> Option<&'static LogFile> {
    GLOBAL.get()
}

/// Una riga nel log del processo (niente se non attivato; errori ignorati).
pub fn log(level: Level, message: &str) {
    if let Some(l) = GLOBAL.get() {
        let _ = l.write(level, message);
    }
}

pub fn debug(message: &str) {
    log(Level::Debug, message);
}

pub fn info(message: &str) {
    log(Level::Info, message);
}

pub fn warn(message: &str) {
    log(Level::Warn, message);
}

pub fn error(message: &str) {
    log(Level::Error, message);
}

/// Riga d'avvio comune: versione, sistema operativo e architettura.
pub fn startup_line(what: &str) -> String {
    format!(
        "{what} avviato: SwapDeck {} su {} ({} {})",
        crate::version(),
        os_description(),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Nome e versione del sistema operativo (es. `Windows 11 Pro 22631`), se leggibili.
pub fn os_description() -> String {
    sysinfo::System::long_os_version()
        .or_else(sysinfo::System::name)
        .unwrap_or_else(|| std::env::consts::OS.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_default()
    }

    #[test]
    fn lines_are_redacted_and_tagged() {
        let tmp = tempfile::tempdir().unwrap();
        let log = LogFile::new(tmp.path().join("logs"), "desktop");
        log.write(
            Level::Warn,
            "cambio per mario.rossi@example.com con sk-ant-oat01-SEGRETO\nriga due",
        )
        .unwrap();
        let text = read(&log.path());
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(text.contains(" WARN  desktop["), "{text}");
        assert!(text.contains("ma***@example.com") && text.contains("sk-ant-***"));
        assert!(!text.contains("mario.rossi") && !text.contains("SEGRETO"));
        assert!(text.ends_with("riga due\n"));
        let ts = text.split(' ').next().unwrap();
        assert!(
            ts.ends_with('Z') && ts.contains('.') && ts.len() == 24,
            "{ts}"
        );
    }

    #[test]
    fn level_filter_and_off() {
        let tmp = tempfile::tempdir().unwrap();
        let log = LogFile::new(tmp.path(), "cli").with_level(Some(Level::Warn));
        log.write(Level::Info, "nascosta").unwrap();
        log.write(Level::Error, "visibile").unwrap();
        let text = read(&log.path());
        assert!(!text.contains("nascosta") && text.contains("visibile"));
        let off = LogFile::new(tmp.path().join("off"), "cli").with_level(None);
        off.write(Level::Error, "x").unwrap();
        assert!(!off.path().exists());
        assert_eq!(Level::parse_min("OFF"), Some(None));
        assert_eq!(Level::parse_min("debug"), Some(Some(Level::Debug)));
        assert_eq!(Level::parse_min("boh"), None);
    }

    #[test]
    fn rotation_keeps_at_most_the_configured_files() {
        let tmp = tempfile::tempdir().unwrap();
        let log = LogFile::new(tmp.path(), "cli").with_limits(400, 3);
        for i in 0..60 {
            log.write(Level::Info, &format!("riga {i:03} {}", "x".repeat(40)))
                .unwrap();
        }
        let mut names: Vec<String> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["swapdeck.log", "swapdeck.log.1", "swapdeck.log.2"]);
        for n in &names {
            let len = fs::metadata(tmp.path().join(n)).unwrap().len();
            assert!(len <= 400, "{n}: {len}");
        }
        // L'ultima riga è nel file attivo, le più vecchie sono state scartate.
        assert!(read(&log.path()).contains("riga 059"));
        assert!(!read(&tmp.path().join("swapdeck.log.2")).contains("riga 000"));
        // Coda: attivo più `.1`, in ordine.
        let last = tail(tmp.path(), 5).unwrap();
        assert_eq!(last.len(), 5);
        assert!(last[4].contains("riga 059") && last[0].contains("riga 055"));
    }

    #[test]
    fn concurrent_writers_lose_no_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_path_buf();
        let threads: Vec<_> = (0..4)
            .map(|t| {
                let dir = dir.clone();
                std::thread::spawn(move || {
                    // Un `LogFile` per thread: come processi diversi (niente mutex comune).
                    let log = LogFile::new(&dir, "cli").with_limits(16 * 1024, 5);
                    for i in 0..200 {
                        log.write(Level::Info, &format!("t{t} n{i}")).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let mut all = String::new();
        for name in [
            LOG_FILE,
            "swapdeck.log.1",
            "swapdeck.log.2",
            "swapdeck.log.3",
            "swapdeck.log.4",
        ] {
            all.push_str(&read(&dir.join(name)));
        }
        let lines: Vec<&str> = all.lines().collect();
        assert_eq!(lines.len(), 800);
        assert!(lines.iter().all(|l| l.contains(" INFO  cli[")));
    }

    #[test]
    fn tail_redacts_lines_written_by_others() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join(LOG_FILE),
            "vecchia riga per luigi.verdi@example.org\nAuthorization: Bearer abc.def\n",
        )
        .unwrap();
        let lines = tail(tmp.path(), 200).unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("lu***@example.org"));
        assert!(!lines[1].contains("abc.def"));
        assert!(tail(&tmp.path().join("manca"), 10).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn file_is_private_and_symlinks_are_refused() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let log = LogFile::new(tmp.path().join("logs"), "cli");
        log.write(Level::Info, "x").unwrap();
        let mode = fs::metadata(log.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let dir_mode = fs::metadata(log.dir()).unwrap().permissions().mode();
        assert_eq!(dir_mode & 0o777, 0o700);
        let target = tmp.path().join("altrove");
        fs::write(&target, "").unwrap();
        fs::remove_file(log.path()).unwrap();
        std::os::unix::fs::symlink(&target, log.path()).unwrap();
        assert!(log.write(Level::Info, "y").is_err());
        assert_eq!(read(&target), "");
    }

    #[test]
    fn component_names_are_checked() {
        assert_eq!(sanitize_component("vscode").as_deref(), Some("vscode"));
        assert_eq!(sanitize_component("a b"), None);
        assert_eq!(sanitize_component(""), None);
        assert_eq!(LogFile::new("x", "../x").component, "swapdeck");
    }
}
