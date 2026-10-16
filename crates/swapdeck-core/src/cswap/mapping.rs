//! Elenco delle cartelle mappate (`cswap map`, che non ha `--json`).
//!
//! Formato di cswap 0.27 (`switcher.py`, `list_mappings`), dopo la pulizia dei colori:
//!
//! ```text
//! Directory mappings:
//!   <path> → <slot>: <email> [<organizzazione o personal>]
//!   <path> → <email> (account removed)
//! ```
//!
//! Senza mappature: `No directory mappings yet.` più una riga di suggerimento.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Cartella mappata a un account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryMapping {
    pub path: String,
    /// Numero dell'account; `null` se l'account mappato è stato rimosso.
    pub account: Option<u32>,
    /// Email dell'account mappato (può essere vuota).
    pub email: String,
}

const ARROW: &str = " → ";
const REMOVED: &str = " (account removed)";
const EMPTY: &str = "No directory mappings";

/// Parte dopo la freccia: `(account, email)`.
fn target(right: &str) -> Option<(Option<u32>, String)> {
    if let Some(email) = right.strip_suffix(REMOVED) {
        return Some((None, email.trim().to_owned()));
    }
    let (slot, rest) = right.split_once(": ")?;
    let slot: u32 = slot.trim().parse().ok().filter(|n| *n > 0)?;
    // Etichetta finale ` [organizzazione]` tolta; l'email può essere vuota.
    let email = rest
        .strip_suffix(']')
        .and_then(|r| r.rfind(" [").map(|i| &r[..i]))
        .unwrap_or(rest);
    Some((Some(slot), email.trim().to_owned()))
}

/// Interpreta l'elenco. Una riga di mappatura non riconosciuta è un errore (il messaggio non
/// riporta il testo: contiene percorsi ed email); le righe non rientrate si ignorano.
pub fn parse_map_listing(text: &str) -> Result<Vec<DirectoryMapping>, String> {
    let mut out = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim_end_matches(['\r', ' ']);
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            if line.starts_with(EMPTY) && out.is_empty() {
                return Ok(Vec::new());
            }
            // Intestazione (`Directory mappings:`), suggerimenti o avvisi di cswap.
            continue;
        }
        let line = line.trim_start();
        // Il percorso può contenere la freccia: vale la prima divisione con un account valido.
        let parsed = line.match_indices(ARROW).find_map(|(i, _)| {
            let path = line[..i].trim();
            let (account, email) = target(&line[i + ARROW.len()..])?;
            (!path.is_empty()).then(|| DirectoryMapping {
                path: path.to_owned(),
                account,
                email,
            })
        });
        match parsed {
            Some(m) => out.push(m),
            None => {
                return Err(format!(
                    "formato di `cswap map` non riconosciuto (riga {})",
                    index + 1
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_listing() {
        let text = "No directory mappings yet.\nMap one with: cswap map <NUM|EMAIL> [PATH]\n";
        assert_eq!(parse_map_listing(text).unwrap(), []);
        assert_eq!(parse_map_listing("").unwrap(), []);
    }

    #[test]
    fn mapped_and_removed_accounts() {
        let text = "Directory mappings:\r\n  c:\\work\\project-a → 3: user3@example.com [Example Org 3]\r\n  /home/u/a → b → 12: user12@example.com [personal]\n  ~/old → user9@example.com (account removed)\n";
        let m = parse_map_listing(text).unwrap();
        assert_eq!(
            m,
            [
                DirectoryMapping {
                    path: "c:\\work\\project-a".into(),
                    account: Some(3),
                    email: "user3@example.com".into(),
                },
                DirectoryMapping {
                    path: "/home/u/a → b".into(),
                    account: Some(12),
                    email: "user12@example.com".into(),
                },
                DirectoryMapping {
                    path: "~/old".into(),
                    account: None,
                    email: "user9@example.com".into(),
                },
            ]
        );
    }

    #[test]
    fn unknown_mapping_line_is_an_error_without_its_text() {
        let err =
            parse_map_listing("Directory mappings:\n  /x/secret-user@example.com\n").unwrap_err();
        assert!(err.contains("riga 2"), "{err}");
        assert!(!err.contains("example.com"));
        assert!(parse_map_listing("  /x → 0: a@b [p]\n").is_err(), "slot 0");
    }
}
