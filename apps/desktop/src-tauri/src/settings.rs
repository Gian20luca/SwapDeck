//! Impostazioni dell'app lette dalla config di SwapDeck: intervallo, soglie, lingua, avvio
//! automatico e posizione del mini-widget.

use std::time::Duration;

use crate::notify::DEFAULT_THRESHOLDS;
use crate::text::Lang;

/// Intervallo di default del ciclo di lettura (piano).
pub const DEFAULT_POLL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub poll_interval: Duration,
    pub thresholds: Vec<f64>,
    pub lang: Lang,
    /// Avvio automatico al login: attivo di default.
    pub autostart: bool,
    /// Ultima posizione del mini-widget, in pixel fisici.
    pub mini_position: Option<(i32, i32)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            poll_interval: DEFAULT_POLL,
            thresholds: DEFAULT_THRESHOLDS.to_vec(),
            lang: Lang::system(),
            autostart: true,
            mini_position: None,
        }
    }
}

/// Lettura e scrittura delle impostazioni. Senza config del core resta in memoria.
#[derive(Debug, Default)]
pub struct SettingsStore;

impl SettingsStore {
    pub fn load() -> (Self, Settings, Vec<String>) {
        (Self, Settings::default(), Vec::new())
    }

    pub fn save(&self, _settings: &Settings) -> Result<(), String> {
        Ok(())
    }
}
