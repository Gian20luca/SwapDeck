//! Logica di SwapDeck: client di cswap e config. Motore, lock, storico e
//! rilevamento dei conflitti arrivano con lo step 2.

pub mod cswap;
// Step 2: pub mod engine;
// Step 2: pub mod lock;
// Step 2: pub mod history;
// Step 2: pub mod conflicts;

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
