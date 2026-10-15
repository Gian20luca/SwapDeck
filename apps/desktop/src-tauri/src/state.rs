//! Stato condiviso dell'app: ultima lettura di cswap, impostazioni, notifiche.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use swapdeck_core::cswap::{CswapError, ListResponse};

use crate::api::{CswapApi, Res};
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

    pub fn active(&self) -> Option<u32> {
        self.list.as_ref().and_then(|l| l.active_account_number)
    }
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
    pub store: SettingsStore,
    pub notifier: Mutex<Notifier>,
    /// Firma dell'ultimo menu costruito: si ricostruisce solo se cambia.
    pub menu_signature: Mutex<String>,
    /// Ultimo salvataggio della posizione del mini-widget.
    pub mini_saved_at: Mutex<Option<Instant>>,
}

impl Shared {
    pub fn new(api: Arc<dyn CswapApi>, store: SettingsStore, settings: Settings) -> Self {
        Self {
            api,
            snapshot: Mutex::new(Snapshot::default()),
            notifier: Mutex::new(Notifier::new(&settings.thresholds)),
            settings: Mutex::new(settings),
            store,
            menu_signature: Mutex::new(String::new()),
            mini_saved_at: Mutex::new(None),
        }
    }

    pub fn save_settings(&self) -> Result<(), String> {
        let settings = lock(&self.settings).clone();
        self.store.save(&settings)
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
