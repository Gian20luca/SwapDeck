//! Impostazioni dell'app lette dalla config di SwapDeck (`config.json` di swapdeck-core):
//! intervallo, soglie, lingua, più due chiavi dell'app desktop conservate in `extra`
//! (`autostart`, `miniWidget`).

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value, json};
use swapdeck_core::config::{self, AppConfig, Language};
use swapdeck_core::engine::EngineRules;
use swapdeck_core::paths::AppPaths;

use crate::text::Lang;

/// Chiavi dell'app desktop dentro `config.json`.
const KEY_AUTOSTART: &str = "autostart";
const KEY_MINI: &str = "miniWidget";

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub poll_interval: Duration,
    pub thresholds: Vec<f64>,
    pub lang: Lang,
    /// Avvio automatico al login: attivo di default.
    pub autostart: bool,
    /// La chiave `autostart` c'era nella config (altrimenti è il primo avvio).
    pub autostart_saved: bool,
    /// Ultima posizione del mini-widget, in pixel fisici.
    pub mini_position: Option<(i32, i32)>,
    /// Regole del motore (sezione `engine`), lette all'avvio e mai riscritte dall'app.
    pub engine: EngineRules,
}

impl Default for Settings {
    fn default() -> Self {
        Self::from_config(&AppConfig::default())
    }
}

fn mini_position(v: Option<&Value>) -> Option<(i32, i32)> {
    let v = v?;
    let x = i32::try_from(v.get("x")?.as_i64()?).ok()?;
    let y = i32::try_from(v.get("y")?.as_i64()?).ok()?;
    Some((x, y))
}

impl Settings {
    pub fn from_config(c: &AppConfig) -> Self {
        Self {
            poll_interval: Duration::from_secs(u64::from(
                c.poll_interval_seconds
                    .max(config::MIN_POLL_INTERVAL_SECONDS),
            )),
            thresholds: c.notifications.thresholds.clone(),
            lang: match c.language {
                Language::It => Lang::It,
                Language::En => Lang::En,
            },
            autostart: c
                .extra
                .get(KEY_AUTOSTART)
                .and_then(Value::as_bool)
                .unwrap_or(true),
            autostart_saved: c
                .extra
                .get(KEY_AUTOSTART)
                .and_then(Value::as_bool)
                .is_some(),
            mini_position: mini_position(c.extra.get(KEY_MINI)),
            engine: c.engine.clone(),
        }
    }

    /// Scrive nella config solo le chiavi dell'app desktop, lasciando il resto com'è.
    pub fn patch(&self, c: &mut AppConfig) {
        c.extra
            .insert(KEY_AUTOSTART.into(), Value::Bool(self.autostart));
        match self.mini_position {
            Some((x, y)) => {
                c.extra.insert(KEY_MINI.into(), json!({ "x": x, "y": y }));
            }
            None => {
                c.extra.remove(KEY_MINI);
            }
        }
    }
}

/// Lettura e scrittura delle impostazioni. Senza percorso (home sconosciuta o config non valida)
/// restano in memoria: un file illeggibile non viene mai sovrascritto.
#[derive(Debug, Default)]
pub struct SettingsStore {
    paths: Option<AppPaths>,
}

impl SettingsStore {
    pub fn in_memory() -> Self {
        Self { paths: None }
    }

    pub fn at(paths: AppPaths) -> Self {
        Self { paths: Some(paths) }
    }

    /// `false` se le impostazioni restano solo in memoria (config non valida o home sconosciuta).
    pub fn is_persistent(&self) -> bool {
        self.paths.is_some()
    }

    fn file(&self) -> Option<PathBuf> {
        self.paths.as_ref().map(AppPaths::config_file)
    }

    /// Impostazioni dal sistema (`SWAPDECK_HOME` o cartelle standard, come `--headless`), con gli avvisi da registrare.
    pub fn load() -> (Self, Settings, Vec<String>) {
        match AppPaths::from_env_or_system() {
            Some(paths) => Self::at(paths).read(),
            None => (
                Self::in_memory(),
                Settings::default(),
                vec!["cartella dell'utente non trovata: impostazioni solo in memoria".into()],
            ),
        }
    }

    /// Legge la config; se non è valida passa alla sola memoria.
    pub fn read(self) -> (Self, Settings, Vec<String>) {
        let Some(path) = self.file() else {
            return (self, Settings::default(), Vec::new());
        };
        match config::load(&path) {
            Ok(loaded) => (self, Settings::from_config(&loaded.config), loaded.warnings),
            Err(e) => (
                Self::in_memory(),
                Settings::default(),
                vec![format!("{e}: impostazioni solo in memoria")],
            ),
        }
    }

    /// Rilegge il file, aggiorna le chiavi dell'app e lo riscrive in modo atomico.
    pub fn save(&self, settings: &Settings) -> Result<(), String> {
        let (Some(paths), Some(path)) = (&self.paths, self.file()) else {
            return Ok(());
        };
        let mut current = config::load(&path).map_err(|e| e.to_string())?.config;
        settings.patch(&mut current);
        paths.ensure().map_err(|e| e.to_string())?;
        config::save(&path, &current).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_plan() {
        let s = Settings::default();
        assert_eq!(s.poll_interval, Duration::from_secs(60));
        assert_eq!(s.thresholds, [80.0, 95.0]);
        assert!(s.autostart);
        assert_eq!(s.mini_position, None);
    }

    #[test]
    fn round_trip_keeps_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path());
        paths.ensure().unwrap();
        std::fs::write(
            paths.config_file(),
            r#"{"schemaVersion":1,"pollIntervalSeconds":120,"language":"en","futureKey":{"a":1}}"#,
        )
        .unwrap();
        let (store, mut s, _) = SettingsStore::at(paths.clone()).read();
        assert!(store.is_persistent());
        assert!(!s.autostart_saved, "primo avvio: chiave assente");
        assert_eq!(s.poll_interval, Duration::from_secs(120));
        assert_eq!(s.lang, Lang::En);
        s.autostart = false;
        s.mini_position = Some((-1200, 40));
        store.save(&s).unwrap();
        let (_, again, _) = SettingsStore::at(paths.clone()).read();
        assert!(!again.autostart);
        assert!(again.autostart_saved);
        assert_eq!(again.mini_position, Some((-1200, 40)));
        let text = std::fs::read_to_string(paths.config_file()).unwrap();
        assert!(
            text.contains("\"futureKey\""),
            "chiavi di altri conservate: {text}"
        );
    }

    #[test]
    fn invalid_config_is_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path());
        paths.ensure().unwrap();
        std::fs::write(paths.config_file(), "{ non json").unwrap();
        let (store, s, warnings) = SettingsStore::at(paths.clone()).read();
        assert!(!warnings.is_empty());
        store.save(&s).unwrap();
        let text = std::fs::read_to_string(paths.config_file()).unwrap();
        assert_eq!(text, "{ non json");
    }

    #[test]
    fn bad_mini_position_is_ignored() {
        assert_eq!(mini_position(Some(&json!({"x": 1, "y": "a"}))), None);
        assert_eq!(mini_position(Some(&json!({"x": 1e12, "y": 2}))), None);
        assert_eq!(
            mini_position(Some(&json!({"x": -5, "y": 2}))),
            Some((-5, 2))
        );
    }
}
