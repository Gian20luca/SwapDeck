//! Cartelle dati di SwapDeck, sempre per utente (piano, §2).
//!
//! - Windows: `%APPDATA%\swapdeck`
//! - Linux: `~/.config/swapdeck` (config) e `~/.local/share/swapdeck` (dati)
//! - macOS: `~/Library/Application Support/swapdeck`

use std::io;
use std::path::{Path, PathBuf};

/// Nome della cartella dell'applicazione.
pub const APP_DIR: &str = "swapdeck";

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
