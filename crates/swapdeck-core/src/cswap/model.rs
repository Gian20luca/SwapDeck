//! Tipi serde che rispecchiano il JSON di `cswap` (schema 1, versioni 0.26.0–0.27.2).
//!
//! Regole (vedi `docs/CSWAP-CONTRATTO.md`): i campi sconosciuti si ignorano, quelli
//! assenti in cswap sono `Option` con `#[ts(optional)]`, i valori di enum sconosciuti
//! diventano `Unknown`. Le date restano stringhe ISO 8601, come le scrive cswap.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Unica versione dello schema JSON di cswap supportata.
pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// `cswap list --json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ListResponse {
    pub schema_version: u32,
    pub active_account_number: Option<u32>,
    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub duplicate_account_warnings: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lockstep_usage_warnings: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unclaimed_credentials: Option<Vec<String>>,
}

/// Una riga di `list --json` (`accounts[]`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub number: u32,
    pub email: String,
    #[serde(default)]
    pub organization_name: String,
    #[serde(default)]
    pub organization_uuid: String,
    #[serde(default)]
    pub is_organization: bool,
    #[serde(default)]
    pub active: bool,
    pub usage_status: UsageStatus,
    #[serde(default)]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub disabled: Option<bool>,
    /// 0.27: scadenza del refresh token; assente = sconosciuta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub login_expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage_fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage_age_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_good_usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_good_fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_good_age_seconds: Option<f64>,
    /// 0.27: causa dell'ultimo fetch fallito (`http-429`, `timeout`, ...), solo con `unavailable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage_error: Option<String>,
    /// 0.27: prossimo tentativo durante il backoff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage_retry_at: Option<String>,
}

impl Account {
    /// `true` se l'account è disabilitato (`disabled` assente = abilitato).
    pub fn is_disabled(&self) -> bool {
        self.disabled.unwrap_or(false)
    }
}

/// `usageStatus` di una riga.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UsageStatus {
    Ok,
    Unavailable,
    TokenExpired,
    ReloginRequired,
    ApiKey,
    KeychainUnavailable,
    ForeignCredential,
    NoCredentials,
    /// Valore introdotto da una versione futura di cswap.
    #[serde(other)]
    Unknown,
}

/// Consumi (`usage` e `lastGoodUsage`); ogni finestra è facoltativa.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub five_hour: Option<UsageWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub seven_day: Option<UsageWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scoped: Option<Vec<UsageWindow>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub spend: Option<Spend>,
}

/// Una finestra di consumo. I campi di ritmo ci sono solo sulle settimanali
/// (`sevenDay`, `scoped[]`); `name` solo su `scoped[]`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Percentuale 0–100.
    pub pct: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resets_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub countdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub clock: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub expected_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ahead_of_pace: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub projected_exhaustion_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub will_last_to_reset: Option<bool>,
}

/// Spesa pay-as-you-go.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Spend {
    pub used: f64,
    pub limit: f64,
    pub pct: f64,
    pub currency: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resets_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub countdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub clock: Option<String>,
}

/// `cswap status --json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub schema_version: u32,
    pub active: Option<ActiveAccount>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub total_managed_accounts: Option<u32>,
}

/// Account attivo in `status --json`: con `managed: false` ci sono solo `email` e `managed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ActiveAccount {
    pub email: String,
    pub managed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub organization_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub organization_uuid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub is_organization: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage_status: Option<UsageStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub alias: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage_fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage_age_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_good_usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_good_fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_good_age_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub usage_retry_at: Option<String>,
}

/// Account citato da `switch` e dagli eventi di `auto` (`number: null` = non gestito).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AccountRef {
    pub number: Option<u32>,
    pub email: String,
}

/// `strategy` di `switch --json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum SwitchStrategyUsed {
    Rotation,
    Direct,
    Best,
    NextAvailable,
    #[serde(other)]
    Unknown,
}

/// `reason` di `switch --json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum SwitchReason {
    Switched,
    AlreadyActive,
    Activated,
    UnmanagedAccount,
    OnlyOneAccount,
    UsageUnavailable,
    AlreadyBest,
    CandidatesExhausted,
    NoValidTarget,
    #[serde(other)]
    Unknown,
}

/// `cswap switch ... --json`. Uscita 0 anche con `switched: false`: decide il campo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SwitchResponse {
    pub schema_version: u32,
    pub switched: bool,
    #[serde(default)]
    pub from: Option<AccountRef>,
    #[serde(default)]
    pub to: Option<AccountRef>,
    pub strategy: SwitchStrategyUsed,
    pub reason: SwitchReason,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub models: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model_source: Option<String>,
}

/// Busta d'errore JSON: `{"schemaVersion":1,"error":{"type":"...","message":"..."}}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEnvelope {
    pub schema_version: u32,
    pub error: ErrorBody,
}

/// Corpo della busta d'errore; `type` è il nome della classe Python.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ErrorBody {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
}

/// `cswap config --json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConfigListResponse {
    pub schema_version: u32,
    pub path: String,
    #[serde(default)]
    pub settings: Vec<ConfigSetting>,
}

/// Una voce di `config --json`, uguale a `config get KEY --json` senza `schemaVersion`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSetting {
    pub key: String,
    /// Numero, booleano, stringa o `null` secondo la chiave.
    pub value: serde_json::Value,
    pub is_set: bool,
}

/// `cswap config get KEY --json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ConfigGetResponse {
    pub schema_version: u32,
    pub key: String,
    pub value: serde_json::Value,
    pub is_set: bool,
}

/// Un evento di `cswap auto --json` (una riga). Eventi sconosciuti → `Unknown`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum AutoEvent {
    #[serde(rename_all = "camelCase")]
    Poll {
        #[serde(default)]
        ts: String,
        #[serde(default)]
        active: Option<AccountRef>,
        #[serde(default)]
        headroom_pct: BTreeMap<String, Option<f64>>,
        #[serde(default)]
        threshold: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        fetch_errors: Option<BTreeMap<String, String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        windows_pct: Option<BTreeMap<String, BTreeMap<String, Option<f64>>>>,
    },
    #[serde(rename_all = "camelCase")]
    Switch {
        #[serde(default)]
        ts: String,
        #[serde(default)]
        trigger: String,
        #[serde(default)]
        from: Option<AccountRef>,
        #[serde(default)]
        to: Option<AccountRef>,
        #[serde(default)]
        warnings: Vec<String>,
        #[serde(default)]
        dry_run: bool,
    },
    #[serde(rename_all = "camelCase")]
    NoSwitch {
        #[serde(default)]
        ts: String,
        #[serde(default)]
        reason: String,
        #[serde(default)]
        detail: String,
    },
    #[serde(rename_all = "camelCase")]
    AccountQuarantined {
        #[serde(default)]
        ts: String,
        #[serde(default)]
        number: Option<u32>,
        #[serde(default)]
        email: String,
        #[serde(default)]
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    AccountUnquarantined {
        #[serde(default)]
        ts: String,
        #[serde(default)]
        number: Option<u32>,
        #[serde(default)]
        email: String,
        #[serde(default)]
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    AllExhausted {
        #[serde(default)]
        ts: String,
        #[serde(default)]
        earliest_reset_at: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Sleep {
        #[serde(default)]
        ts: String,
        #[serde(default)]
        seconds: f64,
        #[serde(default)]
        until: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Error {
        #[serde(default)]
        ts: String,
        #[serde(default)]
        message: String,
        #[serde(default)]
        transient: bool,
    },
    #[serde(rename_all = "camelCase")]
    ConfigWarning {
        #[serde(default)]
        ts: String,
        #[serde(default)]
        message: String,
    },
    /// Evento introdotto da una versione futura: si ignora.
    #[serde(other)]
    Unknown,
}
