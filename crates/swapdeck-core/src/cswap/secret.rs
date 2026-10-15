//! Token di `add-token`: mai in log, errori, `Debug` o serializzazione.

use serde::{Deserialize, Deserializer};
use zeroize::Zeroize;

/// Token o chiave API da passare a `cswap add-token -` via stdin.
///
/// Non implementa `Display` né `Serialize`; `Debug` è oscurato e la memoria
/// viene azzerata al rilascio.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretToken(String);

impl SecretToken {
    /// Crea il token togliendo gli spazi ai bordi (come fa cswap).
    pub fn new(value: impl Into<String>) -> Self {
        let mut raw: String = value.into();
        let trimmed = raw.trim().to_owned();
        raw.zeroize();
        Self(trimmed)
    }

    /// `true` se il token è vuoto (cswap lo rifiuterebbe).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `true` se il token contiene un a capo: cswap legge una riga sola.
    pub fn has_line_break(&self) -> bool {
        self.0.contains(['\n', '\r'])
    }

    /// Valore in chiaro, solo per scriverlo su stdin del processo.
    #[allow(dead_code)] // lo usa il client (punto 2)
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretToken(***)")
    }
}

impl Drop for SecretToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<'de> Deserialize<'de> for SecretToken {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(SecretToken::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_is_redacted() {
        let t = SecretToken::new("  sk-ant-oat01-SEGRETO  ");
        assert_eq!(format!("{t:?}"), "SecretToken(***)");
        assert_eq!(t.expose(), "sk-ant-oat01-SEGRETO");
        assert!(!t.has_line_break());
        assert!(SecretToken::new(" ").is_empty());
    }
}
