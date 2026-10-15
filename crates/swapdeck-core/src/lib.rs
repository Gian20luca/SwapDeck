//! Logica di SwapDeck. Lo scheletro espone solo la versione; il resto arriva dallo step 1.

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
