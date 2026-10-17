//! Cartelle dati di SwapDeck, sempre per utente (piano, §2).
//!
//! - Windows: `%APPDATA%\swapdeck`
//! - Linux: `~/.config/swapdeck` (config) e `~/.local/share/swapdeck` (dati)
//! - macOS: `~/Library/Application Support/swapdeck`

use std::io;
use std::path::{Path, PathBuf};

/// Nome della cartella dell'applicazione.
pub const APP_DIR: &str = "swapdeck";
/// Radice alternativa (assoluta) per config e dati: prove con il finto cswap e
/// installazioni portabili. Usa `config/` e `data/` sotto la radice.
pub const HOME_OVERRIDE_ENV: &str = "SWAPDECK_HOME";

/// Percorsi di SwapDeck.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl AppPaths {
    /// Percorsi standard dell'utente corrente; `None` se la home non è determinabile.
    pub fn from_system() -> Option<Self> {
        let base = directories::BaseDirs::new()?;
        Some(Self {
            config_dir: base.config_dir().join(APP_DIR),
            data_dir: base.data_dir().join(APP_DIR),
        })
    }

    /// `SWAPDECK_HOME` se impostata e assoluta, altrimenti i percorsi standard.
    pub fn from_env_or_system() -> Option<Self> {
        Self::from_override(std::env::var_os(HOME_OVERRIDE_ENV)).or_else(Self::from_system)
    }

    fn from_override(value: Option<std::ffi::OsString>) -> Option<Self> {
        let root = PathBuf::from(value?);
        root.is_absolute().then(|| Self::from_root(&root))
    }

    /// Percorsi sotto una radice qualsiasi (test, installazioni portabili).
    pub fn from_root(root: &Path) -> Self {
        Self {
            config_dir: root.join("config"),
            data_dir: root.join("data"),
        }
    }

    /// `config.json`.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join(crate::config::CONFIG_FILE)
    }

    /// Cartella dello storico (JSONL giornalieri, step 2).
    pub fn history_dir(&self) -> PathBuf {
        self.data_dir.join("history")
    }

    /// Crea le cartelle se mancano (0700 su Unix).
    pub fn ensure(&self) -> io::Result<()> {
        create_private_dir(&self.config_dir)?;
        create_private_dir(&self.data_dir)
    }
}

/// Crea una cartella (e le mancanti sopra) leggibile solo dall'utente su Unix.
pub fn create_private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Rinomina il file temporaneo sopra `target` (scrittura atomica). Su Windows la rinomina
/// fallisce per un attimo se un altro processo (antivirus, indicizzatore, un lettore) tiene
/// aperto il file senza condivisione: pochi tentativi ravvicinati, poi l'errore.
pub fn persist_retrying(mut tmp: tempfile::NamedTempFile, target: &Path) -> io::Result<()> {
    let mut attempt = 0u32;
    loop {
        match tmp.persist(target) {
            Ok(_) => return Ok(()),
            Err(e) if attempt < 4 && e.error.kind() == io::ErrorKind::PermissionDenied => {
                tmp = e.file;
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(25 << attempt));
            }
            Err(e) => return Err(e.error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_paths_end_with_app_dir() {
        if let Some(p) = AppPaths::from_system() {
            assert!(p.config_dir.ends_with(APP_DIR));
            assert!(p.data_dir.ends_with(APP_DIR));
            assert!(p.config_file().ends_with("config.json"));
        }
    }

    #[test]
    fn override_must_be_absolute() {
        let abs = std::env::temp_dir().join("sd");
        let p = AppPaths::from_override(Some(abs.clone().into_os_string())).unwrap();
        assert_eq!(p, AppPaths::from_root(&abs));
        assert_eq!(AppPaths::from_override(Some("relativa".into())), None);
        assert_eq!(AppPaths::from_override(None), None);
    }

    #[test]
    fn ensure_creates_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let p = AppPaths::from_root(tmp.path());
        p.ensure().unwrap();
        assert!(p.config_dir.is_dir() && p.data_dir.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p.config_dir)
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }
}

/// Causa del test instabile `legacy_memory_is_absorbed_once_when_loaded`: su Windows un file
/// appena scritto può essere tenuto aperto per un attimo (antivirus) senza condivisione della
/// cancellazione, e la rinomina sopra di lui fallisce con «accesso negato».
#[cfg(all(test, windows))]
mod windows_persist_tests {
    use super::*;
    use std::io::Write as _;
    use std::os::windows::fs::OpenOptionsExt as _;
    use std::time::Duration;

    /// Apre `path` come farebbe uno scanner: lettura, senza `FILE_SHARE_DELETE`.
    fn hold(path: &Path) -> std::fs::File {
        const FILE_SHARE_READ: u32 = 1;
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(path)
            .unwrap()
    }

    fn tmp_with(dir: &Path, text: &str) -> tempfile::NamedTempFile {
        let mut tmp = tempfile::NamedTempFile::new_in(dir).unwrap();
        tmp.write_all(text.as_bytes()).unwrap();
        tmp
    }

    #[test]
    fn rename_over_a_briefly_held_file_is_retried() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("memoria.json");
        std::fs::write(&target, "vecchio").unwrap();

        // Senza tentativi la rinomina fallisce finché il file è tenuto aperto.
        let held = hold(&target);
        let err = tmp_with(dir.path(), "x").persist(&target).unwrap_err();
        assert_eq!(err.error.kind(), io::ErrorKind::PermissionDenied);
        drop(held);

        // Con i tentativi passa appena il file viene rilasciato.
        let held = hold(&target);
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            drop(held);
        });
        persist_retrying(tmp_with(dir.path(), "nuovo"), &target).unwrap();
        release.join().unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "nuovo");

        // Tenuto aperto troppo a lungo: l'errore arriva comunque, senza attese infinite.
        let _held = hold(&target);
        let err = persist_retrying(tmp_with(dir.path(), "z"), &target).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }
}
