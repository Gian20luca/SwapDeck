//! Azioni di SwapDeck su cswap (F0) e conferme richieste prima di eseguirle.
//!
//! Gli account sono sempre indicati per numero di slot, mai per email: così il
//! prompt di ambiguità di cswap non può scattare.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::secret::SecretToken;

/// Strategia di `switch --strategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum SwitchStrategy {
    Best,
    NextAvailable,
}

impl SwitchStrategy {
    /// Valore per la riga di comando.
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Best => "best",
            Self::NextAvailable => "next-available",
        }
    }
}

/// Strategia di `auto --strategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum AutoStrategy {
    Best,
    ConsumeFirst,
}

impl AutoStrategy {
    /// Valore per la riga di comando.
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Best => "best",
            Self::ConsumeFirst => "consume-first",
        }
    }
}

/// Opzioni di `cswap auto`; quelle assenti valgono come in `cswap config`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoOptions {
    #[ts(as = "Option<bool>", optional)]
    pub once: bool,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_seconds: Option<f64>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_seconds: Option<f64>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_api_key_accounts: Option<bool>,
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<AutoStrategy>,
    #[ts(as = "Option<bool>", optional)]
    pub dry_run: bool,
}

/// `cswap run`: apre Claude Code come un account, in un terminale nuovo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct RunOptions {
    /// Slot; `None` usa la mappatura della cartella corrente.
    #[ts(optional)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<u32>,
    #[ts(as = "Option<bool>", optional)]
    pub no_share: bool,
    /// Non supportato da cswap su Windows.
    #[ts(as = "Option<bool>", optional)]
    pub share_history: bool,
    #[ts(as = "Option<bool>", optional)]
    pub require_session: bool,
    /// Argomenti passati a `claude` dopo `--`.
    #[ts(as = "Option<Vec<String>>", optional)]
    pub claude_args: Vec<String>,
}

/// Un'azione richiesta dall'interfaccia. Arriva dal TypeScript, quindi solo `Deserialize`.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CswapAction {
    /// `switch [<n>] [--strategy S [--model M]] [--force] --json`.
    #[serde(rename_all = "camelCase")]
    Switch {
        #[serde(default)]
        #[ts(optional)]
        target: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        strategy: Option<SwitchStrategy>,
        #[serde(default)]
        #[ts(optional)]
        models: Option<Vec<String>>,
        #[serde(default)]
        #[ts(as = "Option<bool>", optional)]
        force: bool,
    },
    /// `add [--slot N] [--alias A]`; `overwrite` risponde al prompt di slot occupato.
    #[serde(rename_all = "camelCase")]
    Add {
        #[serde(default)]
        #[ts(optional)]
        slot: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        alias: Option<String>,
        #[serde(default)]
        #[ts(as = "Option<bool>", optional)]
        overwrite: bool,
    },
    /// `add-token - [--email E] [--slot N]`, token via stdin.
    #[serde(rename_all = "camelCase")]
    AddToken {
        #[ts(type = "string")]
        token: SecretToken,
        #[serde(default)]
        #[ts(optional)]
        email: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        slot: Option<u32>,
        #[serde(default)]
        #[ts(as = "Option<bool>", optional)]
        overwrite: bool,
    },
    Remove {
        account: u32,
    },
    Disable {
        account: u32,
    },
    Enable {
        account: u32,
    },
    /// `alias <n> <name>`, oppure `alias <n> --unset` con `alias: null`.
    SetAlias {
        account: u32,
        alias: Option<String>,
    },
    Swap {
        a: u32,
        b: u32,
    },
    Move {
        account: u32,
        slot: u32,
    },
    Map {
        account: u32,
        path: String,
    },
    Unmap {
        path: String,
    },
    ConfigSet {
        key: String,
        value: String,
    },
    ConfigUnset {
        key: String,
    },
    /// Il file contiene credenziali in chiaro.
    Export {
        path: String,
        #[serde(default)]
        #[ts(optional)]
        account: Option<u32>,
        #[serde(default)]
        #[ts(as = "Option<bool>", optional)]
        full: bool,
    },
    Import {
        path: String,
        #[serde(default)]
        #[ts(as = "Option<bool>", optional)]
        force: bool,
    },
    UnclaimedPurge {
        id: String,
    },
    Purge,
    /// `uv tool upgrade claude-swap` (su Windows `cswap upgrade` non aggiorna).
    Upgrade,
    Run {
        options: RunOptions,
    },
    AutoStart {
        options: AutoOptions,
    },
    AutoStop,
}

/// Parola da digitare per confermare `purge`.
pub const PURGE_CONFIRM_WORD: &str = "purge";

/// Conferma da chiedere a video prima di un'azione.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Confirmation {
    None,
    Simple,
    /// Doppia conferma: la seconda richiede di digitare `word`.
    Typed {
        word: String,
    },
}

impl CswapAction {
    /// Conferma richiesta (piano, «Azioni con conferma»). `active` è lo slot attivo.
    pub fn confirmation(&self, active: Option<u32>) -> Confirmation {
        match self {
            Self::Purge => Confirmation::Typed {
                word: PURGE_CONFIRM_WORD.to_owned(),
            },
            Self::Remove { .. }
            | Self::Import { .. }
            | Self::Export { .. }
            | Self::Swap { .. }
            | Self::Move { .. }
            | Self::Unmap { .. }
            | Self::UnclaimedPurge { .. }
            | Self::Upgrade => Confirmation::Simple,
            Self::Disable { account } if Some(*account) == active => Confirmation::Simple,
            Self::Add {
                overwrite: true, ..
            }
            | Self::AddToken {
                overwrite: true, ..
            } => Confirmation::Simple,
            _ => Confirmation::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmations_follow_the_plan() {
        let parse = |s: &str| serde_json::from_str::<CswapAction>(s).unwrap();
        assert_eq!(
            parse(r#"{"kind":"switch"}"#).confirmation(Some(1)),
            Confirmation::None
        );
        assert_eq!(
            parse(r#"{"kind":"remove","account":2}"#).confirmation(None),
            Confirmation::Simple
        );
        assert_eq!(
            parse(r#"{"kind":"disable","account":2}"#).confirmation(Some(2)),
            Confirmation::Simple
        );
        assert_eq!(
            parse(r#"{"kind":"disable","account":2}"#).confirmation(Some(3)),
            Confirmation::None
        );
        assert!(matches!(
            parse(r#"{"kind":"purge"}"#).confirmation(None),
            Confirmation::Typed { .. }
        ));
        let token = parse(r#"{"kind":"addToken","token":"sk-ant-x"}"#);
        assert!(!format!("{token:?}").contains("sk-ant-x"));
    }
}
