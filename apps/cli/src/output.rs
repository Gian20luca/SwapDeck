//! Formati JSON della CLI che non esistono già in `swapdeck-core`. I tipi TypeScript escono in
//! `apps/vscode/src/generated/` (vedi `ts_export.rs`).

use serde::Serialize;
use swapdeck_core::config::AppConfig;
use swapdeck_core::cswap::{CswapError, ListResponse};
use swapdeck_core::lock::LockInfo;
use ts_rs::TS;

/// Versione del protocollo JSON della CLI: cambia solo se un formato cambia in modo incompatibile.
pub const PROTOCOL: u32 = 1;

/// `swapdeck-cli version`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CliVersion {
    pub name: String,
    pub version: String,
    pub protocol: u32,
}

/// Stato del motore: config e lock (`swapdeck-cli engine status`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    /// `engine.enabled` della config: il motore cambia account solo se è vero.
    pub enabled: bool,
    pub dry_run: bool,
    pub poll_interval_seconds: u32,
    /// Un'istanza tiene il lock ed è viva.
    pub running: bool,
    pub lock: Option<LockInfo>,
    /// Nome della mappa attiva (tooltip della barra di stato di VS Code).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_map: Option<String>,
    /// Config o lock non leggibili (il resto vale come default).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub problem: Option<String>,
}

/// `swapdeck-cli state`: come lo `Snapshot` dell'app, ma senza memoria tra due chiamate.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CliState {
    /// Lista letta ora; `null` se la lettura è fallita.
    pub list: Option<ListResponse>,
    pub error: Option<CswapError>,
    /// Istante della lettura, in ms Unix.
    #[ts(type = "number")]
    pub updated_at_ms: u64,
    pub engine: EngineStatus,
}

/// `swapdeck-cli config get|set`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub config: AppConfig,
    /// `false` se il file non esiste ancora (valgono i default).
    pub from_file: bool,
    pub path: String,
    pub warnings: Vec<String>,
}

/// Errore di qualunque comando: stdout `{"error": …}` e codice di uscita diverso da 0.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct CliErrorEnvelope {
    pub error: CswapError,
}
