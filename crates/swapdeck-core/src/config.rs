//! `config.json` di SwapDeck: default, validazione e scrittura atomica.
//!
//! Intervallo di controllo (unico, usato anche dal motore), soglie delle notifiche,
//! lingua, tema e regole del motore (`engine`). Le chiavi sconosciute si conservano,
//! così una versione vecchia non cancella quelle di una nuova.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::engine::{EngineProfiles, EngineRules, ProfileCommand};
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

/// Notifiche (F2, F18): regole nel modulo `notify`.
pub use crate::notify::NotificationConfig;
use crate::notify::rules::default_thresholds;

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
    /// Regole del motore.
    #[serde(default)]
    pub engine: EngineRules,
    /// Mappe (profili con nome); `engine` sono le regole di quella attiva. Assenti nel file
    /// = migrazione: le regole attuali diventano la mappa «Personale», più i preset.
    #[serde(default)]
    #[ts(skip)]
    pub engine_profiles: EngineProfiles,
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
            engine: EngineRules::default(),
            engine_profiles: EngineProfiles::seeded(&EngineRules::default()),
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
        let problems = self.notifications.problems();
        if !problems.is_empty() {
            return Err(ConfigError::Invalid(problems.join("; ")));
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
        self.engine.sync_order();
        warnings.extend(self.engine_profiles.normalize(&self.engine));
        warnings.extend(self.notifications.normalize());
        warnings
    }
}

/// Impostazioni modificabili dall'interfaccia: la config senza le regole del motore.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferences {
    pub poll_interval_seconds: u32,
    /// Soglie delle notifiche in percentuale, crescenti.
    pub notification_thresholds: Vec<f64>,
    pub language: Language,
    pub theme: Theme,
}

/// Modifica di [`AppPreferences`]: solo i campi presenti; un campo sconosciuto è un errore.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreferencesPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub poll_interval_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub notification_thresholds: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub language: Option<Language>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub theme: Option<Theme>,
}

impl AppConfig {
    pub fn preferences(&self) -> AppPreferences {
        AppPreferences {
            poll_interval_seconds: self.poll_interval_seconds,
            notification_thresholds: self.notifications.thresholds.clone(),
            language: self.language,
            theme: self.theme,
        }
    }

    /// Applica la modifica con gli stessi limiti di [`AppConfig::validate`]; se non è valida
    /// la config resta com'era. Lingua o tema scelti qui prendono il posto di un valore
    /// sconosciuto conservato dal file.
    pub fn apply_preferences(&mut self, patch: &PreferencesPatch) -> Result<(), ConfigError> {
        let mut next = self.clone();
        if let Some(v) = patch.poll_interval_seconds {
            next.poll_interval_seconds = v;
        }
        if let Some(v) = &patch.notification_thresholds {
            next.notifications.set_legacy_thresholds(v);
        }
        if let Some(v) = patch.language {
            next.language = v;
            next.preserved.remove("language");
        }
        if let Some(v) = patch.theme {
            next.theme = v;
            next.preserved.remove("theme");
        }
        next.validate()?;
        *self = next;
        Ok(())
    }

    /// Applica una merge patch (RFC 7386) alle notifiche e la valida; se non è valida la
    /// config resta com'era.
    pub fn patch_notifications(&mut self, patch: &serde_json::Value) -> Result<(), ConfigError> {
        self.notifications = self
            .notifications
            .merged(patch)
            .map_err(ConfigError::Invalid)?;
        Ok(())
    }

    /// Sostituisce le regole del motore dopo [`EngineRules::validate_for_save`]. Con un preset
    /// attivo cambiano solo le parti comuni a tutte le mappe (il preset è in sola lettura).
    pub fn set_engine_rules(&mut self, rules: EngineRules) -> Result<(), ConfigError> {
        let mut rules = rules;
        rules.sync_order();
        // La conferma della prima accensione resta anche se chi scrive non la conosce.
        rules.first_enable_confirmed |= self.engine.enable_confirmed();
        let mut profiles = self.engine_profiles.clone();
        profiles.normalize(&self.engine);
        profiles
            .check_editable(&rules)
            .and_then(|()| profiles.check_list(&rules))
            .map_err(ConfigError::Invalid)?;
        let problems = rules.validate_for_save();
        if !problems.is_empty() {
            return Err(ConfigError::Invalid(problems.join("; ")));
        }
        self.engine_profiles = profiles;
        self.engine_profiles.normalize(&rules);
        self.engine = rules;
        Ok(())
    }

    /// Operazione sulle mappe; le regole della mappa attiva passano in `engine` (il motore le
    /// usa dal giro dopo). Se le regole risultanti non sono valide la config resta com'era.
    pub fn apply_profile_command(&mut self, cmd: &ProfileCommand) -> Result<(), ConfigError> {
        let mut profiles = self.engine_profiles.clone();
        profiles.normalize(&self.engine);
        let rules = profiles
            .apply(cmd, &self.engine)
            .map_err(ConfigError::Invalid)?;
        // Si controllano solo regole attive nuove: rinominare o creare resta sempre possibile.
        let mut current = self.engine.clone();
        current.sync_order();
        let problems = rules.validate_for_save();
        if rules != current && !problems.is_empty() {
            profiles.check_list(&rules).map_err(ConfigError::Invalid)?;
            return Err(ConfigError::Invalid(problems.join("; ")));
        }
        self.engine = rules;
        self.engine_profiles = profiles;
        Ok(())
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

/// `engine.intervalSecs` (prime versioni dello step 2) diventa `pollIntervalSeconds`
/// se questo manca; in ogni caso la chiave doppia si toglie.
fn migrate_engine_interval(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    warnings: &mut Vec<String>,
) {
    let Some(old) = obj
        .get_mut("engine")
        .and_then(serde_json::Value::as_object_mut)
        .and_then(|e| e.remove("intervalSecs"))
    else {
        return;
    };
    if !obj.contains_key("pollIntervalSeconds") && old.is_u64() {
        obj.insert("pollIntervalSeconds".to_owned(), old);
        warnings.push("engine.intervalSecs spostato in pollIntervalSeconds".to_owned());
    }
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
        migrate_engine_interval(obj, &mut warnings);
        warnings.extend(crate::notify::rules::migrate(obj));
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

/// Rilegge la config, applica `change` e la riscrive in modo atomico: le altre chiavi restano.
/// Un file non valido dà errore e non viene mai sovrascritto.
pub fn update(
    path: &Path,
    change: impl FnOnce(&mut AppConfig) -> Result<(), ConfigError>,
) -> Result<AppConfig, ConfigError> {
    let mut config = load(path)?.config;
    change(&mut config)?;
    save(path, &config)?;
    Ok(config)
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
    // Su Windows la rinomina fallisce per un attimo se un altro processo (antivirus, indicizzatore)
    // tiene aperto il file senza condivisione: pochi tentativi ravvicinati.
    let mut attempt = 0u32;
    loop {
        match tmp.persist(path) {
            Ok(_) => return Ok(()),
            Err(e) if attempt < 4 && e.error.kind() == io::ErrorKind::PermissionDenied => {
                tmp = e.file;
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(25 << attempt));
            }
            Err(e) => return Err(ConfigError::Io(e.error)),
        }
    }
}

/// Apre in lettura senza seguire un link simbolico finale (Unix).
pub(crate) fn open_no_follow(path: &Path) -> io::Result<File> {
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
    fn engine_section_and_interval_migration() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(CONFIG_FILE);
        std::fs::write(
            &path,
            r#"{"engine":{"enabled":true,"primary":6,"intervalSecs":120,"ignoredConflicts":["task:cswap-guard"]}}"#,
        )
        .unwrap();
        let l = load(&path).unwrap();
        assert!(l.config.engine.enabled);
        assert_eq!(l.config.engine.primary, Some(6));
        assert_eq!(l.config.engine.ignored_conflicts, vec!["task:cswap-guard"]);
        assert_eq!(l.config.poll_interval_seconds, 120);
        assert_eq!(l.warnings.len(), 1);
        assert!(!l.config.extra.contains_key("engine"));
        // Con pollIntervalSeconds presente vince quello; la chiave doppia sparisce.
        std::fs::write(
            &path,
            r#"{"pollIntervalSeconds":300,"engine":{"intervalSecs":120}}"#,
        )
        .unwrap();
        let l = load(&path).unwrap();
        assert_eq!(l.config.poll_interval_seconds, 300);
        save(&path, &l.config).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("intervalSecs") && text.contains("\"engine\""));
    }

    #[test]
    fn engine_profiles_migration_and_commands() {
        use crate::engine::{PresetKind, ProfileCommand, ReserveCriterion};
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(CONFIG_FILE);
        std::fs::write(
            &path,
            r#"{"engine":{"enabled":true,"primary":6,"reserves":[1,3]}}"#,
        )
        .unwrap();
        // Regole vecchie: diventano la mappa «Personale» attiva, più i preset.
        let l = load(&path).unwrap();
        assert!(l.warnings.is_empty(), "{:?}", l.warnings);
        let p = &l.config.engine_profiles;
        assert_eq!(p.active_id, "personal");
        assert_eq!(p.profiles.len(), 1 + PresetKind::ALL.len());
        assert_eq!(l.config.engine.order, Some(vec![6, 1, 3]));
        assert_eq!(p.active().unwrap().rules, l.config.engine);

        // Selezione: regole della mappa in `engine`, motore ancora acceso, file riscritto.
        let select = ProfileCommand::Select { id: "sport".into() };
        let c = update(&path, |c| c.apply_profile_command(&select)).unwrap();
        assert!(c.engine.enabled);
        assert_eq!(c.engine.reserve_criterion, ReserveCriterion::Lowest5h);
        let reread = load(&path).unwrap().config;
        assert_eq!(reread.engine, c.engine);
        assert_eq!(reread.engine_profiles.active_id, "sport");

        // Preset attivo in sola lettura: cambiano solo le parti comuni.
        let edited = EngineRules {
            five_hour_threshold: 80.0,
            ..c.engine.clone()
        };
        let err = update(&path, |c| c.set_engine_rules(edited.clone())).unwrap_err();
        assert!(err.to_string().contains("sola lettura"), "{err}");
        let observe = EngineRules {
            dry_run: true,
            ..c.engine.clone()
        };
        let c = update(&path, |c| c.set_engine_rules(observe.clone())).unwrap();
        assert!(c.engine.dry_run);
        assert_eq!(c.engine_profiles.active_id, "sport");

        // Clonato, il preset diventa una mappa personale attiva e modificabile.
        let clone = ProfileCommand::CloneAs {
            id: "sport".into(),
            name: "Mia sport".into(),
        };
        let c = update(&path, |c| c.apply_profile_command(&clone)).unwrap();
        assert_eq!(c.engine_profiles.active().unwrap().name, "Mia sport");
        // Config con il motore già acceso: la prima accensione conta come confermata.
        let observe = EngineRules {
            first_enable_confirmed: true,
            ..observe
        };
        assert_eq!(c.engine, observe);
        let edited = EngineRules {
            five_hour_threshold: 80.0,
            ..c.engine.clone()
        };
        let c = update(&path, |c| c.set_engine_rules(edited.clone())).unwrap();
        assert_eq!(c.engine_profiles.active().unwrap().rules, edited);
        let sport = c.engine_profiles.profiles.iter().find(|p| p.id == "sport");
        assert_eq!(
            sport.unwrap().rules.five_hour_threshold,
            98.0,
            "preset invariato"
        );

        // Mappa senza lista con il motore acceso: selezione rifiutata, config invariata;
        // le operazioni che non cambiano le regole attive restano possibili.
        let mut on = AppConfig::default();
        let create = ProfileCommand::Create {
            name: "Vuota".into(),
        };
        on.apply_profile_command(&create).unwrap();
        on.set_engine_rules(EngineRules {
            enabled: true,
            primary: Some(1),
            ..EngineRules::default()
        })
        .unwrap();
        let before = on.clone();
        let empty = on
            .engine_profiles
            .profiles
            .iter()
            .find(|p| p.name == "Vuota");
        let select = ProfileCommand::Select {
            id: empty.unwrap().id.clone(),
        };
        assert!(on.apply_profile_command(&select).is_err());
        assert_eq!(on, before);
        let create = ProfileCommand::Create {
            name: "Altra".into(),
        };
        on.apply_profile_command(&create).unwrap();
        assert_eq!(on.engine, before.engine);
    }

    /// Preset attivo dopo la migrazione dalla v0.1.0 (esclusi dentro la lista): dalla pagina
    /// Regole si salvano le parti comuni; senza lista il motore non parte e l'errore lo dice.
    #[test]
    fn active_preset_saves_common_parts() {
        use crate::engine::ProfileCommand;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(CONFIG_FILE);
        std::fs::write(
            &path,
            r#"{"engine":{"order":[6,1,3],"excluded":[3],"reserveCriterion":"lowest5h",
                "fiveHourThreshold":98},
               "engineProfiles":{"activeId":"sport","profiles":[
                {"id":"personal","name":"Personale","rules":{"order":[2]}},
                {"id":"sport","name":"Sport","preset":"sport","rules":{"order":[6,1,3],
                 "excluded":[3],"reserveCriterion":"lowest5h","fiveHourThreshold":98}}]}}"#,
        )
        .unwrap();
        let l = load(&path).unwrap();
        assert_eq!(l.config.engine_profiles.active_id, "sport");
        let observe = EngineRules {
            dry_run: true,
            ignored_conflicts: vec!["x".into()],
            ..l.config.engine.clone()
        };
        let c = update(&path, |c| c.set_engine_rules(observe.clone())).unwrap();
        assert!(c.engine.dry_run);
        let on = EngineRules {
            enabled: true,
            ..c.engine.clone()
        };
        update(&path, |c| c.set_engine_rules(on.clone())).unwrap();

        let mut fresh = AppConfig::default();
        let select = ProfileCommand::Select { id: "sport".into() };
        fresh.apply_profile_command(&select).unwrap();
        let on = EngineRules {
            enabled: true,
            ..fresh.engine.clone()
        };
        let err = fresh.set_engine_rules(on).unwrap_err().to_string();
        assert!(
            err.contains("non ha una lista") && err.contains("clonalo"),
            "{err}"
        );
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

#[cfg(test)]
mod preferences_tests {
    use super::*;

    #[test]
    fn preferences_patch_is_validated_and_partial() {
        let mut c = AppConfig::default();
        let patch: PreferencesPatch =
            serde_json::from_str(r#"{"pollIntervalSeconds":120,"theme":"dark"}"#).unwrap();
        c.apply_preferences(&patch).unwrap();
        let p = c.preferences();
        assert_eq!(p.poll_interval_seconds, 120);
        assert_eq!(p.theme, Theme::Dark);
        assert_eq!(p.language, Language::It, "campo assente: invariato");
        for bad in [
            r#"{"pollIntervalSeconds":59}"#,
            r#"{"pollIntervalSeconds":3601}"#,
            r#"{"notificationThresholds":[95,80]}"#,
            r#"{"notificationThresholds":[0]}"#,
            r#"{"notificationThresholds":[10,20,30,40,50,60]}"#,
        ] {
            let patch: PreferencesPatch = serde_json::from_str(bad).unwrap();
            assert!(c.apply_preferences(&patch).is_err(), "{bad}");
            assert_eq!(
                c.poll_interval_seconds, 120,
                "config invariata dopo l'errore"
            );
        }
        for bad in [
            r#"{"language":"fr"}"#,
            r#"{"theme":"auto"}"#,
            r#"{"engine":{}}"#,
        ] {
            assert!(
                serde_json::from_str::<PreferencesPatch>(bad).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn update_keeps_other_keys_and_never_overwrites_invalid_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE);
        std::fs::write(
            &path,
            r#"{"schemaVersion":1,"theme":"neon","autostart":false,"engine":{"primary":6,"enabled":true},"futureKey":{"a":1}}"#,
        )
        .unwrap();
        let patch = PreferencesPatch {
            poll_interval_seconds: Some(300),
            ..PreferencesPatch::default()
        };
        update(&path, |c| c.apply_preferences(&patch)).unwrap();
        let saved: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["pollIntervalSeconds"], 300);
        assert_eq!(saved["autostart"], false);
        assert_eq!(saved["futureKey"]["a"], 1);
        assert_eq!(saved["theme"], "neon", "valore sconosciuto conservato");
        assert_eq!(saved["engine"]["primary"], 6);
        let rules = EngineRules {
            primary: Some(3),
            ..EngineRules::default()
        };
        let c = update(&path, |c| c.set_engine_rules(rules.clone())).unwrap();
        let rules = EngineRules {
            order: Some(vec![3]),
            // Il motore era acceso nel file: la conferma della prima accensione resta.
            first_enable_confirmed: true,
            ..rules
        };
        assert_eq!(c.engine, rules, "lista allineata alla principale");
        assert_eq!(c.engine_profiles.active().unwrap().rules, rules);
        assert_eq!(c.poll_interval_seconds, 300);
        let invalid = EngineRules {
            enabled: true,
            ..EngineRules::default()
        };
        assert!(update(&path, |c| c.set_engine_rules(invalid)).is_err());
        assert_eq!(load(&path).unwrap().config.engine, rules, "file invariato");
        std::fs::write(&path, "{ rotto").unwrap();
        assert!(update(&path, |c| c.apply_preferences(&patch)).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ rotto");
    }
}
