//! Notifiche (F2, F18): funzione pura [`evaluate`] (lista di cswap, motore, storico, regole,
//! memoria, ora) → elenco di [`Notification`] con testo in italiano e in inglese, gravità e
//! chiave anti-ripetizione. La memoria ([`NotifyMemory`]) sta nella cartella dati
//! (`notify-memory.json`): un riavvio non ripete gli avvisi già dati nella stessa finestra.
//!
//! Un solo mittente: le notifiche le manda chi tiene il lock del motore (app desktop o
//! `headless`); senza lock vivo le chiede l'estensione VS Code con `notifications poll`, che
//! ricontrolla il lock ed è esclusiva (`notify-poll.lock`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::config::Language;
use crate::cswap::Account;
use crate::engine::{Candidate, Decision, EngineRules, SwitchCause};
use crate::history::HistoryRecord;
use crate::setup::UpdateInfo;

mod eval;
pub mod rules;
mod store;
mod text;

pub use eval::{SummaryState, evaluate, summary_state};
pub use rules::{
    DailySummaryRule, LoginRule, ModelLimitRule, NotificationConfig, NotifyScope, PercentRule,
    QuietHours, ResetSoonRule, ScopedToggle, StaleRule, SwitchPreviewRule, Toggle, parse_hhmm,
};
pub use store::{
    MEMORY_FILE, Notifier, NotifyStore, POLL_LOCK_FILE, PollGuard, local_offset_secs, poll,
};

/// Gravità di un avviso. `critical` passa il non disturbare se l'utente lo lascia attivo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// Tipo di avviso (uno per voce del catalogo F18, più il riepilogo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum NotificationKind {
    SwitchPreview,
    Switched,
    ReturnedToPrimary,
    NoReserve,
    Threshold,
    Exhaustion,
    ResetSoon,
    ResetDone,
    ModelLimit,
    EngineSuspended,
    EngineResumed,
    Login,
    Stale,
    Update,
    DailySummary,
}

/// Testo in italiano e in inglese.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LocalizedText {
    pub it: String,
    pub en: String,
}

impl LocalizedText {
    pub fn get(&self, lang: Language) -> &str {
        match lang {
            Language::It => &self.it,
            Language::En => &self.en,
        }
    }
}

/// Avviso da mostrare.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    pub kind: NotificationKind,
    pub severity: Severity,
    /// Chiave anti-ripetizione (nella memoria finché la finestra o la condizione dura).
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account: Option<u32>,
    pub title: LocalizedText,
    pub body: LocalizedText,
}

/// Chi manda le notifiche in questo momento (esito di `notifications poll`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum NotifySender {
    /// Questa chiamata: nessun motore vivo, le notifiche restituite vanno mostrate.
    Caller,
    /// Il lock del motore è tenuto da un'istanza viva (app o headless): manda lei.
    Engine,
    /// Un'altra richiesta è in corso (altra finestra di VS Code): niente da mostrare.
    Busy,
}

/// Esito di `notifications poll`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NotifyPoll {
    pub sender: NotifySender,
    pub notifications: Vec<Notification>,
    /// Lingua della config, per scegliere il testo.
    pub language: Language,
}

/// Evento JSON di `headless` su stdout (una riga): `{"event":"notification","notification":…}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum HeadlessEvent {
    Notification { notification: Notification },
}

/// Cambio eseguito dal motore in questo giro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchDone {
    pub from: Option<u32>,
    pub to: u32,
    pub cause: SwitchCause,
}

/// Candidato del motore per il prossimo cambio (preavviso, da [`Candidate`]). `target`
/// assente = «un'altra utenza» (motore senza candidato).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SwitchPreview {
    pub target: Option<u32>,
    pub reason: Option<String>,
}

impl From<&Candidate> for SwitchPreview {
    fn from(c: &Candidate) -> Self {
        Self {
            target: Some(c.number),
            reason: Some(c.why.clone()),
        }
    }
}

/// Motore visto dal mittente (solo chi tiene il lock).
#[derive(Debug, Clone, Copy)]
pub struct EngineContext<'a> {
    pub rules: &'a EngineRules,
    /// Decisione del giro; `None` se la lettura è fallita.
    pub decision: Option<&'a Decision>,
    pub switched: Option<SwitchDone>,
    pub preview: Option<&'a SwitchPreview>,
    /// Nome della mappa (profilo) in uso, se c'è.
    pub profile: Option<&'a str>,
}

/// Ingressi di [`evaluate`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NotifyInput<'a> {
    /// Lista appena letta; `None` se la lettura è fallita.
    pub list: Option<&'a crate::cswap::ListResponse>,
    /// `None` se il mittente non ha il motore (estensione senza lock).
    pub engine: Option<EngineContext<'a>>,
    /// Record dello storico del giorno locale, solo quando il riepilogo è dovuto.
    pub history: Option<&'a [HistoryRecord]>,
    /// Versione di cswap dalla cache del modulo setup.
    pub update: Option<&'a UpdateInfo>,
    /// Scostamento dell'ora locale da UTC, in secondi.
    pub local_offset_secs: i32,
}

/// Memoria delle notifiche tra un giro e l'altro (e tra un mittente e l'altro).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotifyMemory {
    /// Chiavi già notificate → istante (potate dopo 8 giorni).
    pub sent: BTreeMap<String, i64>,
    pub last_active: Option<u32>,
    /// È già arrivata una lista (la prima fa da base per i cambi).
    pub seen_list: bool,
    /// Ultima valutazione: dopo una pausa lunga i cambi visti non si notificano.
    pub updated_at: Option<i64>,
    /// Sospensione del motore già notificata (motivo).
    pub engine_paused: Option<String>,
    /// Ultima % per `account:finestra` (un calo forte riarma gli avvisi senza `resetsAt`).
    pub pct: BTreeMap<String, f64>,
    /// Prossimo reset per `account:finestra` (reset avvenuto).
    pub resets: BTreeMap<String, i64>,
    /// Istante dell'ultimo dato fresco per account (dati non aggiornati).
    pub fresh_at: BTreeMap<u32, i64>,
    /// Nomi da mostrare (alias o `#N`, mai email) per gli avvisi senza lista.
    pub names: BTreeMap<u32, String>,
    /// Giorno locale (giorni dal 1970) dell'ultimo riepilogo.
    pub summary_day: Option<i64>,
}

/// Nome da mostrare: alias, altrimenti il numero (niente email nelle notifiche di sistema).
pub fn display_name(account: &Account) -> String {
    match account.alias.as_deref().map(str::trim) {
        Some(a) if !a.is_empty() => a.to_owned(),
        _ => format!("#{}", account.number),
    }
}

#[cfg(test)]
mod tests;
