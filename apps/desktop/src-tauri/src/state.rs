//! Stato condiviso dell'app: ultima lettura di cswap, impostazioni, notifiche.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use swapdeck_core::config::{AppConfig, ConfigError};
use swapdeck_core::conflicts::Conflict;
use swapdeck_core::cswap::{CswapError, CswapErrorKind, ListResponse};

use crate::api::{CswapApi, Res};
use crate::engine_loop::EngineView;
use crate::notify::Notifier;
use crate::settings::{Settings, SettingsStore};

/// Evento con lo [`Snapshot`] aggiornato, verso tutte le finestre.
pub const STATE_EVENT: &str = "state-changed";

/// Ultimo stato noto, inviato alla UI con [`STATE_EVENT`].
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Ultima lista letta con successo (resta anche se la lettura dopo fallisce).
    pub list: Option<ListResponse>,
    /// Errore dell'ultima lettura, se è fallita.
    pub error: Option<CswapError>,
    /// Istante dell'ultima lettura, in ms Unix (0 = mai).
    pub updated_at_ms: u64,
    /// Motore sospeso (conflitto o pausa): variante «sospeso» dell'icona.
    pub engine_suspended: bool,
    /// `cswap auto` avviato dall'app è in esecuzione.
    pub auto_running: bool,
    /// Stato del motore dall'ultimo giro che ha deciso.
    pub engine: Option<EngineView>,
}

impl Snapshot {
    pub fn apply(&mut self, result: &Res<ListResponse>, now_ms: u64) {
        match result {
            Ok(list) => {
                self.list = Some(list.clone());
                self.error = None;
            }
            Err(e) => self.error = Some(e.clone()),
        }
        self.updated_at_ms = now_ms;
    }

    pub fn set_engine(&mut self, view: EngineView) {
        self.engine_suspended = view.paused();
        self.engine = Some(view);
    }

    pub fn active(&self) -> Option<u32> {
        self.list.as_ref().and_then(|l| l.active_account_number)
    }
}

/// Errore della config per la UI: valori non validi = `invalidUsage`.
pub fn config_error(e: ConfigError) -> CswapError {
    let kind = match e {
        ConfigError::Invalid(_) => CswapErrorKind::InvalidUsage,
        _ => CswapErrorKind::Failed,
    };
    CswapError::new(kind, e.to_string())
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Blocca un mutex anche se un thread è andato in panico tenendolo.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Shared {
    pub api: Arc<dyn CswapApi>,
    pub snapshot: Mutex<Snapshot>,
    pub settings: Mutex<Settings>,
    /// Ultime impostazioni scritte (o lette): si riscrive solo se cambiano.
    saved: Mutex<Settings>,
    pub store: SettingsStore,
    pub notifier: Mutex<Notifier>,
    /// Firma dell'ultimo menu costruito: si ricostruisce solo se cambia.
    pub menu_signature: Mutex<String>,
    /// Ultimo salvataggio della posizione del mini-widget.
    pub mini_saved_at: Mutex<Option<Instant>>,
    /// Conflitti dell'ultima lettura del motore (vuoti a motore spento).
    pub conflicts: Mutex<Vec<Conflict>>,
}

impl Shared {
    pub fn new(api: Arc<dyn CswapApi>, store: SettingsStore, settings: Settings) -> Self {
        Self {
            api,
            snapshot: Mutex::new(Snapshot::default()),
            notifier: Mutex::new(Notifier::new(&settings.thresholds)),
            saved: Mutex::new(settings.clone()),
            settings: Mutex::new(settings),
            store,
            menu_signature: Mutex::new(String::new()),
            mini_saved_at: Mutex::new(None),
            conflicts: Mutex::new(Vec::new()),
        }
    }

    /// Cambia preferenze o regole nella config (scrittura atomica, altri campi conservati) e le
    /// applica subito alle impostazioni in memoria: il ciclo di lettura le usa dal giro dopo.
    pub fn update_config(
        &self,
        change: impl FnOnce(&mut AppConfig) -> Result<(), ConfigError>,
    ) -> Res<AppConfig> {
        let current = lock(&self.settings).clone();
        let saved = self.store.update(&current, change).map_err(config_error)?;
        lock(&self.settings).apply_config(&saved);
        lock(&self.saved).apply_config(&saved);
        lock(&self.notifier).set_thresholds(&saved.notifications.thresholds);
        Ok(saved)
    }

    pub fn save_settings(&self) -> Result<(), String> {
        let settings = lock(&self.settings).clone();
        let mut saved = lock(&self.saved);
        if *saved == settings {
            return Ok(());
        }
        self.store.save(&settings)?;
        *saved = settings;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::fixture_list;
    use swapdeck_core::cswap::CswapErrorKind;

    #[test]
    fn error_keeps_last_list() {
        let mut s = Snapshot::default();
        s.apply(&Ok(fixture_list()), 10);
        assert_eq!(s.active(), Some(6));
        s.apply(&Err(CswapError::new(CswapErrorKind::Timeout, "lento")), 20);
        assert_eq!(s.active(), Some(6));
        assert_eq!(
            s.error.as_ref().map(|e| e.kind),
            Some(CswapErrorKind::Timeout)
        );
        assert_eq!(s.updated_at_ms, 20);
        let json = serde_json::to_value(&s).unwrap();
        assert!(json.get("updatedAtMs").is_some());
        assert!(json.get("engineSuspended").is_some());
        s.apply(&Ok(fixture_list()), 30);
        assert!(s.error.is_none());
    }
}
