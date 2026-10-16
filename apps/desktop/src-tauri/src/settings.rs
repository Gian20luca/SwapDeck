//! Impostazioni dell'app lette dalla config di SwapDeck (`config.json` di swapdeck-core):
//! intervallo, soglie, lingua, più le chiavi dell'app desktop conservate in `extra`
//! (`autostart`, `miniWidget`; `updates.enabled` in sola lettura). Preferenze e regole del
//! motore si cambiano dalla UI con [`SettingsStore::update`] (stessa scrittura atomica, altri
//! campi conservati).

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{Value, json};
use swapdeck_core::config::{self, AppConfig, AppPreferences, ConfigError, Language, Theme};
use swapdeck_core::engine::{EngineProfiles, EngineRules};
use swapdeck_core::paths::AppPaths;

use crate::state::lock;
use crate::text::Lang;

/// Chiavi dell'app desktop dentro `config.json`.
const KEY_AUTOSTART: &str = "autostart";
const KEY_MINI: &str = "miniWidget";
const KEY_UPDATES: &str = "updates";

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub poll_interval: Duration,
    pub thresholds: Vec<f64>,
    pub lang: Lang,
    pub theme: Theme,
    /// Avvio automatico al login: attivo di default.
    pub autostart: bool,
    /// La chiave `autostart` c'era nella config (altrimenti è il primo avvio).
    pub autostart_saved: bool,
    /// Ultima posizione del mini-widget, in pixel fisici.
    pub mini_position: Option<(i32, i32)>,
    /// Il mini-widget è ridotto a una striscia (chiave `miniWidget.compact`).
    pub mini_compact: bool,
    /// Aggiornamenti dalle GitHub Releases (`updates.enabled`): spenti di default finché il
    /// repo è privato. Letti all'avvio e mai riscritti dall'app.
    pub updates_enabled: bool,
    /// Regole del motore (sezione `engine`): il ciclo di lettura le rilegge a ogni giro.
    pub engine: EngineRules,
    /// Mappe (sezione `engineProfiles`); `engine` sono le regole di quella attiva.
    pub engine_profiles: EngineProfiles,
}

impl Default for Settings {
    fn default() -> Self {
        Self::from_config(&AppConfig::default())
    }
}

fn mini_compact(v: Option<&Value>) -> bool {
    v.and_then(|v| v.get("compact")).and_then(Value::as_bool) == Some(true)
}

fn mini_position(v: Option<&Value>) -> Option<(i32, i32)> {
    let v = v?;
    let x = i32::try_from(v.get("x")?.as_i64()?).ok()?;
    let y = i32::try_from(v.get("y")?.as_i64()?).ok()?;
    Some((x, y))
}

impl Settings {
    pub fn from_config(c: &AppConfig) -> Self {
        let mut s = Self {
            poll_interval: Duration::ZERO,
            thresholds: Vec::new(),
            lang: Lang::It,
            theme: Theme::System,
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
            mini_compact: mini_compact(c.extra.get(KEY_MINI)),
            updates_enabled: c
                .extra
                .get(KEY_UPDATES)
                .and_then(|u| u.get("enabled"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            engine: EngineRules::default(),
            engine_profiles: EngineProfiles::default(),
        };
        s.apply_config(c);
        s
    }

    /// Preferenze e regole dalla config appena salvata (le chiavi dell'app restano).
    pub fn apply_config(&mut self, c: &AppConfig) {
        self.poll_interval = Duration::from_secs(u64::from(
            c.poll_interval_seconds
                .max(config::MIN_POLL_INTERVAL_SECONDS),
        ));
        self.thresholds.clone_from(&c.notifications.thresholds);
        self.lang = match c.language {
            Language::It => Lang::It,
            Language::En => Lang::En,
        };
        self.theme = c.theme;
        self.engine.clone_from(&c.engine);
        self.engine_profiles.clone_from(&c.engine_profiles);
    }

    /// Config equivalente, per chi la modifica quando le impostazioni restano in memoria.
    fn to_config(&self) -> AppConfig {
        let mut c = AppConfig {
            poll_interval_seconds: u32::try_from(self.poll_interval.as_secs()).unwrap_or(u32::MAX),
            language: self.preferences().language,
            theme: self.theme,
            engine: self.engine.clone(),
            engine_profiles: self.engine_profiles.clone(),
            ..AppConfig::default()
        };
        c.notifications.thresholds.clone_from(&self.thresholds);
        self.patch(&mut c);
        c
    }

    pub fn preferences(&self) -> AppPreferences {
        AppPreferences {
            poll_interval_seconds: u32::try_from(self.poll_interval.as_secs()).unwrap_or(u32::MAX),
            notification_thresholds: self.thresholds.clone(),
            language: match self.lang {
                Lang::It => Language::It,
                Lang::En => Language::En,
            },
            theme: self.theme,
        }
    }

    /// Scrive nella config solo le chiavi dell'app desktop, lasciando il resto com'è.
    pub fn patch(&self, c: &mut AppConfig) {
        c.extra
            .insert(KEY_AUTOSTART.into(), Value::Bool(self.autostart));
        let mut mini = serde_json::Map::new();
        if let Some((x, y)) = self.mini_position {
            mini.insert("x".into(), json!(x));
            mini.insert("y".into(), json!(y));
        }
        if self.mini_compact {
            mini.insert("compact".into(), Value::Bool(true));
        }
        if mini.is_empty() {
            c.extra.remove(KEY_MINI);
        } else {
            c.extra.insert(KEY_MINI.into(), Value::Object(mini));
        }
    }
}

/// Lettura e scrittura delle impostazioni. Senza percorso (home sconosciuta o config non valida)
/// restano in memoria: un file illeggibile non viene mai sovrascritto.
#[derive(Debug, Default)]
pub struct SettingsStore {
    paths: Option<AppPaths>,
    /// Una scrittura alla volta: ognuna rilegge il file, cambia la sua parte e lo riscrive.
    writing: Mutex<()>,
}

impl SettingsStore {
    pub fn in_memory() -> Self {
        Self::default()
    }

    pub fn at(paths: AppPaths) -> Self {
        Self {
            paths: Some(paths),
            writing: Mutex::new(()),
        }
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
        let _writing = lock(&self.writing);
        let mut current = config::load(&path).map_err(|e| e.to_string())?.config;
        settings.patch(&mut current);
        paths.ensure().map_err(|e| e.to_string())?;
        config::save(&path, &current).map_err(|e| e.to_string())
    }

    /// Cambia preferenze o regole: rilegge il file, applica `change` (che valida), lo riscrive
    /// in modo atomico e restituisce la config salvata. Senza file (impostazioni solo in memoria)
    /// la modifica vale per `current` e non si scrive nulla.
    pub fn update(
        &self,
        current: &Settings,
        change: impl FnOnce(&mut AppConfig) -> Result<(), ConfigError>,
    ) -> Result<AppConfig, ConfigError> {
        let _writing = lock(&self.writing);
        let (Some(paths), Some(path)) = (&self.paths, self.file()) else {
            let mut c = current.to_config();
            change(&mut c)?;
            c.validate()?;
            return Ok(c);
        };
        paths.ensure()?;
        config::update(&path, change)
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
        assert!(!s.mini_compact);
        assert!(!s.updates_enabled, "aggiornamenti spenti di default");
    }

    #[test]
    fn updates_are_read_but_never_written() {
        let mut c = AppConfig::default();
        c.extra
            .insert(KEY_UPDATES.into(), json!({ "enabled": true }));
        let s = Settings::from_config(&c);
        assert!(s.updates_enabled);
        let mut out = AppConfig::default();
        s.patch(&mut out);
        assert!(!out.extra.contains_key(KEY_UPDATES));
        c.extra
            .insert(KEY_UPDATES.into(), json!({ "enabled": "si" }));
        assert!(!Settings::from_config(&c).updates_enabled);
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
    fn compact_state_is_saved_with_the_position() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path());
        paths.ensure().unwrap();
        std::fs::write(
            paths.config_file(),
            r#"{"schemaVersion":1,"miniWidget":{"x":10,"y":20},"futureKey":1}"#,
        )
        .unwrap();
        let (store, mut s, _) = SettingsStore::at(paths.clone()).read();
        assert!(!s.mini_compact, "chiave assente: espanso");
        s.mini_compact = true;
        store.save(&s).unwrap();
        let (_, again, _) = SettingsStore::at(paths.clone()).read();
        assert!(again.mini_compact);
        assert_eq!(again.mini_position, Some((10, 20)), "posizione conservata");
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(paths.config_file()).unwrap()).unwrap();
        assert_eq!(v["miniWidget"]["compact"], true);
        assert_eq!(v["futureKey"], 1);
        // Compresso senza posizione salvata: la chiave c'è comunque e si rilegge.
        let mut s2 = again.clone();
        s2.mini_position = None;
        store.save(&s2).unwrap();
        let (_, back, _) = SettingsStore::at(paths.clone()).read();
        assert!(back.mini_compact);
        assert_eq!(back.mini_position, None);
        // Di nuovo espanso: resta solo la posizione, niente chiave `compact`.
        let mut s3 = again;
        s3.mini_compact = false;
        store.save(&s3).unwrap();
        let text = std::fs::read_to_string(paths.config_file()).unwrap();
        assert!(!text.contains("compact"), "{text}");
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

    #[test]
    fn update_saves_preferences_and_rules_keeping_other_keys() {
        use swapdeck_core::config::PreferencesPatch;
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path());
        paths.ensure().unwrap();
        std::fs::write(
            paths.config_file(),
            r#"{"schemaVersion":1,"autostart":false,"miniWidget":{"x":1,"y":2},"futureKey":7}"#,
        )
        .unwrap();
        let (store, mut s, _) = SettingsStore::at(paths.clone()).read();
        let patch = PreferencesPatch {
            poll_interval_seconds: Some(300),
            theme: Some(Theme::Dark),
            language: Some(Language::En),
            ..PreferencesPatch::default()
        };
        let saved = store.update(&s, |c| c.apply_preferences(&patch)).unwrap();
        s.apply_config(&saved);
        assert_eq!(s.poll_interval, Duration::from_secs(300));
        assert_eq!((s.theme, s.lang), (Theme::Dark, Lang::En));
        assert!(!s.autostart, "chiavi dell'app invariate");
        let rules = EngineRules {
            enabled: true,
            primary: Some(6),
            ..EngineRules::default()
        };
        let saved = store
            .update(&s, |c| c.set_engine_rules(rules.clone()))
            .unwrap();
        s.apply_config(&saved);
        let mut rules = rules;
        rules.sync_order();
        assert_eq!(s.engine, rules);
        assert_eq!(s.engine_profiles.active().unwrap().rules, rules);
        let text = std::fs::read_to_string(paths.config_file()).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["autostart"], false);
        assert_eq!(v["miniWidget"]["x"], 1);
        assert_eq!(v["futureKey"], 7);
        assert_eq!(v["pollIntervalSeconds"], 300);
        assert_eq!(v["engine"]["primary"], 6);
        // Valori non validi: errore e file invariato.
        let bad = PreferencesPatch {
            poll_interval_seconds: Some(5),
            ..PreferencesPatch::default()
        };
        assert!(store.update(&s, |c| c.apply_preferences(&bad)).is_err());
        assert_eq!(std::fs::read_to_string(paths.config_file()).unwrap(), text);
        // Solo in memoria: la modifica vale, niente file.
        let mem = SettingsStore::in_memory()
            .update(&s, |c| c.apply_preferences(&patch))
            .unwrap();
        assert_eq!(mem.preferences(), s.preferences());
        assert_eq!(mem.engine, rules);
    }
}
