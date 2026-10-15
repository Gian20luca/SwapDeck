//! `config.json` di SwapDeck: default, validazione e scrittura atomica.
//!
//! Per ora: intervallo di controllo, soglie delle notifiche, lingua e tema.
//! Le regole del motore arrivano con lo step 2; le chiavi sconosciute si
//! conservano, così una versione vecchia non cancella quelle di una nuova.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::paths::create_private_dir;

/// Nome del file.
pub const CONFIG_FILE: &str = "config.json";
/// Versione del formato.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;
/// Intervallo minimo: sotto i 60 s cswap non ha dati più freschi.
pub const MIN_POLL_INTERVAL_SECONDS: u32 = 60;
/// Intervallo massimo.
pub const MAX_POLL_INTERVAL_SECONDS: u32 = 3600;
/// Numero massimo di soglie.
pub const MAX_THRESHOLDS: usize = 5;
/// Dimensione massima del file letto.
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Lingua dell'interfaccia.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    It,
    En,
}

/// Tema dell'interfaccia.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

/// Notifiche (F2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NotificationConfig {
    /// Soglie in percentuale, crescenti (default 80 e 95).
    #[serde(default = "default_thresholds")]
    pub thresholds: Vec<f64>,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            thresholds: default_thresholds(),
        }
    }
}

fn default_thresholds() -> Vec<f64> {
    vec![80.0, 95.0]
}

fn default_interval() -> u32 {
    60
}

fn default_schema() -> u32 {
    CONFIG_SCHEMA_VERSION
}

/// Configurazione di SwapDeck.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    /// Intervallo di controllo di `cswap list --json`, in secondi.
    #[serde(default = "default_interval")]
    pub poll_interval_seconds: u32,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub language: Language,
    #[serde(default)]
    pub theme: Theme,
    /// Chiavi non note a questa versione, conservate così come sono.
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: serde_json::Map<String, serde_json::Value>,
    /// Valori sconosciuti di `language` e `theme` letti dal file: si riscrivono al
    /// salvataggio finché il campo vale ancora il default usato al loro posto.
    #[serde(skip)]
    #[ts(skip)]
    pub preserved: serde_json::Map<String, serde_json::Value>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            poll_interval_seconds: default_interval(),
            notifications: NotificationConfig::default(),
            language: Language::default(),
            theme: Theme::default(),
            extra: serde_json::Map::new(),
            preserved: serde_json::Map::new(),
        }
    }
}

/// Errore di lettura o scrittura della config.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("config non leggibile o non scrivibile: {0}")]
    Io(#[from] io::Error),
    #[error("config non valida: {0}")]
    Invalid(String),
    #[error("config scritta da una versione più recente (schemaVersion {0})")]
    UnsupportedVersion(u32),
}

impl AppConfig {
    /// Controllo rigido, usato prima di salvare.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedVersion(self.schema_version));
        }
        if !(MIN_POLL_INTERVAL_SECONDS..=MAX_POLL_INTERVAL_SECONDS)
            .contains(&self.poll_interval_seconds)
        {
            return Err(ConfigError::Invalid(format!(
                "pollIntervalSeconds deve stare tra {MIN_POLL_INTERVAL_SECONDS} e {MAX_POLL_INTERVAL_SECONDS}"
            )));
        }
        let t = &self.notifications.thresholds;
        if t.len() > MAX_THRESHOLDS {
            return Err(ConfigError::Invalid(format!(
                "al massimo {MAX_THRESHOLDS} soglie"
            )));
        }
        if t.iter().any(|v| !v.is_finite() || *v <= 0.0 || *v > 100.0) {
            return Err(ConfigError::Invalid(
                "le soglie devono stare tra 0 (escluso) e 100".into(),
            ));
        }
        if t.windows(2).any(|w| w[0] >= w[1]) {
            return Err(ConfigError::Invalid(
                "le soglie devono essere crescenti e distinte".into(),
            ));
        }
        Ok(())
    }

    /// Riporta nei limiti i valori scritti a mano; restituisce gli avvisi.
    pub fn normalize(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        let clamped = self
            .poll_interval_seconds
            .clamp(MIN_POLL_INTERVAL_SECONDS, MAX_POLL_INTERVAL_SECONDS);
        if clamped != self.poll_interval_seconds {
            warnings.push(format!(
                "pollIntervalSeconds {} riportato a {clamped}",
                self.poll_interval_seconds
            ));
            self.poll_interval_seconds = clamped;
        }
        let before = self.notifications.thresholds.clone();
        let mut t: Vec<f64> = before
            .iter()
            .copied()
            .filter(|v| v.is_finite() && *v > 0.0 && *v <= 100.0)
            .collect();
        t.sort_by(f64::total_cmp);
        t.dedup();
        t.truncate(MAX_THRESHOLDS);
        if t.is_empty() && !before.is_empty() {
            t = default_thresholds();
        }
        if t != before {
            warnings.push("soglie delle notifiche corrette".into());
            self.notifications.thresholds = t;
        }
        warnings
    }
}

/// Esito del caricamento.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub config: AppConfig,
    /// `false` se il file non esiste (si usano i default, niente viene scritto).
    pub from_file: bool,
    pub warnings: Vec<String>,
}

/// Legge la config; file assente = default. JSON non valido = errore (il file non si tocca).
pub fn load(path: &Path) -> Result<Loaded, ConfigError> {
    let mut file = match open_no_follow(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Loaded {
                config: AppConfig::default(),
                from_file: false,
                warnings: Vec::new(),
            });
        }
        Err(e) => return Err(e.into()),
    };
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Err(ConfigError::Invalid("non è un file regolare".into()));
    }
    if meta.len() > MAX_CONFIG_BYTES {
        return Err(ConfigError::Invalid("file troppo grande".into()));
    }
    let mut text = String::new();
    Read::by_ref(&mut file)
        .take(MAX_CONFIG_BYTES)
        .read_to_string(&mut text)?;
    let mut value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|e| ConfigError::Invalid(e.to_string()))?;
    let mut preserved = serde_json::Map::new();
    let mut warnings = Vec::new();
    if let Some(obj) = value.as_object_mut() {
        // Lingua o tema sconosciuti (per esempio scritti da una versione più recente):
        // default con avviso, e il valore originale torna nel file al salvataggio.
        let known = |key: &str, v: &serde_json::Value| match key {
            "language" => Language::deserialize(v).is_ok(),
            _ => Theme::deserialize(v).is_ok(),
        };
        for key in ["language", "theme"] {
            if let Some(v) = obj.get(key).filter(|v| !known(key, v)) {
                warnings.push(format!("{key} {v} sconosciuto: si usa il default"));
                let v = v.clone();
                obj.remove(key);
                preserved.insert(key.to_owned(), v);
            }
        }
    }
    let mut config: AppConfig =
        serde_json::from_value(value).map_err(|e| ConfigError::Invalid(e.to_string()))?;
    if config.schema_version > CONFIG_SCHEMA_VERSION {
        return Err(ConfigError::UnsupportedVersion(config.schema_version));
    }
    config.schema_version = CONFIG_SCHEMA_VERSION;
    config.preserved = preserved;
    warnings.extend(config.normalize());
    Ok(Loaded {
        config,
        from_file: true,
        warnings,
    })
}

/// Valida e scrive in modo atomico (file temporaneo nella stessa cartella, poi rinomina).
pub fn save(path: &Path, config: &AppConfig) -> Result<(), ConfigError> {
    config.validate()?;
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .ok_or_else(|| ConfigError::Invalid("percorso senza cartella".into()))?;
    create_private_dir(dir)?;
    let mut value =
        serde_json::to_value(config).map_err(|e| ConfigError::Invalid(e.to_string()))?;
    let defaults = serde_json::to_value(AppConfig::default())
        .map_err(|e| ConfigError::Invalid(e.to_string()))?;
    if let Some(obj) = value.as_object_mut() {
        for (key, original) in &config.preserved {
            if obj.get(key).is_some_and(|v| Some(v) == defaults.get(key)) {
                obj.insert(key.clone(), original.clone());
            }
        }
    }
    let mut json =
        serde_json::to_vec_pretty(&value).map_err(|e| ConfigError::Invalid(e.to_string()))?;
    json.push(b'\n');
    let mut tmp = tempfile::Builder::new()
        .prefix(".config.")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    tmp.write_all(&json)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| ConfigError::Io(e.error))?;
    Ok(())
}

/// Apre in lettura senza seguire un link simbolico finale (Unix).
fn open_no_follow(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    opts.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let c = AppConfig::default();
        c.validate().unwrap();
        assert_eq!(c.poll_interval_seconds, 60);
        assert_eq!(c.notifications.thresholds, [80.0, 95.0]);
        assert_eq!(c.language, Language::It);
        assert_eq!(c.theme, Theme::System);
    }

    #[test]
    fn missing_file_gives_defaults_and_save_roundtrips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sub").join(CONFIG_FILE);
        let loaded = load(&path).unwrap();
        assert!(!loaded.from_file);
        assert!(!path.exists());
        let mut c = loaded.config;
        c.language = Language::En;
        c.extra
            .insert("rules".into(), serde_json::json!({"primary": 6}));
        save(&path, &c).unwrap();
        let again = load(&path).unwrap();
        assert!(again.from_file);
        assert_eq!(again.config, c);
        let left: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left.len(), 1, "niente file temporanei rimasti");
    }

    #[test]
    fn invalid_values() {
        let mut c = AppConfig {
            poll_interval_seconds: 5,
            ..Default::default()
        };
        assert!(c.validate().is_err());
        c.notifications.thresholds = vec![95.0, 80.0, 80.0, f64::NAN, 120.0];
        let w = c.normalize();
        assert_eq!(w.len(), 2);
        assert_eq!(c.poll_interval_seconds, 60);
        assert_eq!(c.notifications.thresholds, [80.0, 95.0]);
        c.validate().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(CONFIG_FILE);
        c.notifications.thresholds = vec![95.0, 80.0];
        assert!(save(&path, &c).is_err());
        assert!(!path.exists());
    }

    #[test]
    fn load_handles_hand_written_files() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(CONFIG_FILE);
        std::fs::write(
            &path,
            "\u{feff}{\"pollIntervalSeconds\": 10, \"theme\": \"dark\"}",
        )
        .unwrap();
        let l = load(&path).unwrap();
        assert_eq!(l.config.poll_interval_seconds, 60);
        assert_eq!(l.config.theme, Theme::Dark);
        assert_eq!(l.warnings.len(), 1);
        std::fs::write(&path, "{non json").unwrap();
        assert!(matches!(load(&path), Err(ConfigError::Invalid(_))));
        std::fs::write(&path, "{\"schemaVersion\": 2}").unwrap();
        assert!(matches!(
            load(&path),
            Err(ConfigError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn unknown_language_and_theme_are_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(CONFIG_FILE);
        std::fs::write(
            &path,
            r#"{"language": "fr", "theme": 3, "pollIntervalSeconds": 120}"#,
        )
        .unwrap();
        let l = load(&path).unwrap();
        assert_eq!(l.config.language, Language::It);
        assert_eq!(l.config.theme, Theme::System);
        assert_eq!(l.config.poll_interval_seconds, 120);
        assert_eq!(l.warnings.len(), 2);
        // Lingua invariata: torna "fr"; tema cambiato dall'utente: vale il nuovo.
        let mut c = l.config;
        c.theme = Theme::Dark;
        save(&path, &c).unwrap();
        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(raw["language"], "fr");
        assert_eq!(raw["theme"], "dark");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("altro.json");
        std::fs::write(&target, "{}").unwrap();
        let link = tmp.path().join(CONFIG_FILE);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(load(&link).is_err());
    }
}
