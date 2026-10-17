//! Logica di SwapDeck: client di cswap, config, cartelle dati, motore delle regole, lock,
//! storico, rilevamento dei conflitti e runner senza interfaccia.

pub mod applog;
pub mod autostart;
pub mod config;
pub mod conflicts;
pub mod cswap;
pub mod diagnostics;
pub mod engine;
pub mod filelock;
pub mod headless;
pub mod history;
pub mod lock;
pub mod notify;
pub mod paths;
pub mod redact;
pub mod setup;
pub mod timeutil;
pub mod usage;

#[cfg(test)]
mod ts_export;

/// Versione del crate, uguale a quella del workspace.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_not_empty() {
        assert!(!version().is_empty());
    }
}
