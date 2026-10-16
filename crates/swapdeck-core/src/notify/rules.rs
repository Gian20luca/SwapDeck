//! Regole delle notifiche (sezione `notifications` di `config.json`): un interruttore e i
//! parametri per ogni avviso, non disturbare e riepilogo giornaliero. Ogni sotto-oggetto ha i
//! suoi default, così un file scritto a mano o da una versione vecchia resta valido.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Numero massimo di soglie per elenco.
pub const MAX_THRESHOLDS: usize = 5;

/// Utenze su cui vale un avviso.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum NotifyScope {
    /// Solo l'utenza attiva.
    #[default]
    Active,
    /// Tutte le utenze.
    All,
}

/// Avviso senza parametri.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct Toggle {
    pub enabled: bool,
}

impl Default for Toggle {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Avviso che vale per l'attiva o per tutte.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ScopedToggle {
    pub enabled: bool,
    pub scope: NotifyScope,
}

impl Default for ScopedToggle {
    fn default() -> Self {
        Self {
            enabled: true,
            scope: NotifyScope::Active,
        }
    }
}

/// Preavviso di switch (solo con il motore acceso).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct SwitchPreviewRule {
    pub enabled: bool,
    /// Punti percentuali dalla soglia del motore (0 = solo il tempo).
    pub points_before: f64,
    /// Minuti al raggiungimento della soglia al ritmo attuale (0 = solo i punti).
    pub minutes_before: u32,
}

impl Default for SwitchPreviewRule {
    fn default() -> Self {
        Self {
            enabled: true,
            points_before: 5.0,
            minutes_before: 10,
        }
    }
}

/// Soglie percentuali, separate per 5 ore e 7 giorni.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct PercentRule {
    pub enabled: bool,
    pub scope: NotifyScope,
    pub five_hour: Vec<f64>,
    pub seven_day: Vec<f64>,
}

impl Default for PercentRule {
    fn default() -> Self {
        Self {
            enabled: true,
            scope: NotifyScope::Active,
            five_hour: default_thresholds(),
            seven_day: default_thresholds(),
        }
    }
}

/// Reset imminente.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ResetSoonRule {
    pub enabled: bool,
    pub scope: NotifyScope,
    pub minutes_before: u32,
}

impl Default for ResetSoonRule {
    fn default() -> Self {
        Self {
            enabled: false,
            scope: NotifyScope::Active,
            minutes_before: 15,
        }
    }
}

/// Limite per modello (finestre `scoped`, per esempio Fable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ModelLimitRule {
    pub enabled: bool,
    pub scope: NotifyScope,
    pub thresholds: Vec<f64>,
}

impl Default for ModelLimitRule {
    fn default() -> Self {
        Self {
            enabled: true,
            scope: NotifyScope::Active,
            thresholds: default_thresholds(),
        }
    }
}

/// Login da rifare o in scadenza.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct LoginRule {
    pub enabled: bool,
    pub scope: NotifyScope,
    pub days_before: u32,
}

impl Default for LoginRule {
    fn default() -> Self {
        Self {
            enabled: true,
            scope: NotifyScope::All,
            days_before: 3,
        }
    }
}

/// Dati non aggiornati.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct StaleRule {
    pub enabled: bool,
    pub scope: NotifyScope,
    pub minutes: u32,
}

impl Default for StaleRule {
    fn default() -> Self {
        Self {
            enabled: true,
            scope: NotifyScope::Active,
            minutes: 15,
        }
    }
}

/// Non disturbare: fascia oraria locale `start`–`end` (`"HH:MM"`, anche a cavallo della
/// mezzanotte; uguali = mai).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct QuietHours {
    pub enabled: bool,
    pub start: String,
    pub end: String,
    /// Gli avvisi critici (nessuna riserva, login) passano comunque.
    pub allow_critical: bool,
}

impl Default for QuietHours {
    fn default() -> Self {
        Self {
            enabled: false,
            start: "22:00".into(),
            end: "08:00".into(),
            allow_critical: true,
        }
    }
}

/// Riepilogo giornaliero all'ora locale `time` (`"HH:MM"`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct DailySummaryRule {
    pub enabled: bool,
    pub time: String,
}

impl Default for DailySummaryRule {
    fn default() -> Self {
        Self {
            enabled: false,
            time: "20:00".into(),
        }
    }
}

/// Notifiche (F2, F18).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NotificationConfig {
    /// Soglie della versione 1 (F2): copia di `percent.fiveHour`, scritta per le versioni
    /// precedenti dell'app. In lettura, senza `percent`, diventano le soglie 5h e 7d.
    #[serde(default = "default_thresholds")]
    pub thresholds: Vec<f64>,
    #[serde(default)]
    pub switch_preview: SwitchPreviewRule,
    #[serde(default)]
    pub switched: Toggle,
    #[serde(default)]
    pub returned_to_primary: Toggle,
    #[serde(default)]
    pub no_reserve: Toggle,
    #[serde(default)]
    pub percent: PercentRule,
    #[serde(default)]
    pub exhaustion: ScopedToggle,
    #[serde(default)]
    pub reset_soon: ResetSoonRule,
    #[serde(default = "reset_done_default")]
    pub reset_done: ScopedToggle,
    #[serde(default)]
    pub model_limit: ModelLimitRule,
    #[serde(default)]
    pub engine_state: Toggle,
    #[serde(default)]
    pub login: LoginRule,
    #[serde(default)]
    pub stale: StaleRule,
    #[serde(default)]
    pub updates: Toggle,
    #[serde(default)]
    pub quiet_hours: QuietHours,
    #[serde(default)]
    pub daily_summary: DailySummaryRule,
    /// Chiavi di una versione più recente, conservate.
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn reset_done_default() -> ScopedToggle {
    ScopedToggle {
        enabled: false,
        scope: NotifyScope::Active,
    }
}

pub(crate) fn default_thresholds() -> Vec<f64> {
    vec![80.0, 95.0]
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            thresholds: default_thresholds(),
            switch_preview: SwitchPreviewRule::default(),
            switched: Toggle::default(),
            returned_to_primary: Toggle::default(),
            no_reserve: Toggle::default(),
            percent: PercentRule::default(),
            exhaustion: ScopedToggle::default(),
            reset_soon: ResetSoonRule::default(),
            reset_done: reset_done_default(),
            model_limit: ModelLimitRule::default(),
            engine_state: Toggle::default(),
            login: LoginRule::default(),
            stale: StaleRule::default(),
            updates: Toggle::default(),
            quiet_hours: QuietHours::default(),
            daily_summary: DailySummaryRule::default(),
            extra: serde_json::Map::new(),
        }
    }
}

/// `"HH:MM"` → minuti dalla mezzanotte.
pub fn parse_hhmm(text: &str) -> Option<u32> {
    let (h, m) = text.trim().split_once(':')?;
    if h.is_empty() || h.len() > 2 || m.len() != 2 {
        return None;
    }
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

fn thresholds_problems(name: &str, t: &[f64], out: &mut Vec<String>) {
    if t.len() > MAX_THRESHOLDS {
        out.push(format!("{name}: al massimo {MAX_THRESHOLDS} soglie"));
    }
    if t.iter().any(|v| !v.is_finite() || *v <= 0.0 || *v > 100.0) {
        out.push(format!("{name}: soglie tra 0 (escluso) e 100"));
    }
    if t.windows(2).any(|w| w[0] >= w[1]) {
        out.push(format!("{name}: soglie crescenti e distinte"));
    }
}

/// Soglie ripulite: valide, ordinate, senza doppioni, al massimo [`MAX_THRESHOLDS`].
pub fn sanitize_thresholds(t: &[f64]) -> Vec<f64> {
    let mut out: Vec<f64> = t
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v > 0.0 && *v <= 100.0)
        .collect();
    out.sort_by(f64::total_cmp);
    out.dedup();
    out.truncate(MAX_THRESHOLDS);
    out
}

impl NotificationConfig {
    /// Problemi che impediscono il salvataggio (vuoto = valida).
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        thresholds_problems("percent.fiveHour", &self.percent.five_hour, &mut out);
        thresholds_problems("percent.sevenDay", &self.percent.seven_day, &mut out);
        thresholds_problems(
            "modelLimit.thresholds",
            &self.model_limit.thresholds,
            &mut out,
        );
        let p = self.switch_preview.points_before;
        if !p.is_finite() || !(0.0..=50.0).contains(&p) {
            out.push("switchPreview.pointsBefore tra 0 e 50".into());
        }
        if self.switch_preview.minutes_before > 600 {
            out.push("switchPreview.minutesBefore al massimo 600".into());
        }
        if !(1..=1440).contains(&self.reset_soon.minutes_before) {
            out.push("resetSoon.minutesBefore tra 1 e 1440".into());
        }
        if !(1..=60).contains(&self.login.days_before) {
            out.push("login.daysBefore tra 1 e 60".into());
        }
        if !(1..=1440).contains(&self.stale.minutes) {
            out.push("stale.minutes tra 1 e 1440".into());
        }
        for (name, v) in [
            ("quietHours.start", &self.quiet_hours.start),
            ("quietHours.end", &self.quiet_hours.end),
            ("dailySummary.time", &self.daily_summary.time),
        ] {
            if parse_hhmm(v).is_none() {
                out.push(format!("{name} nel formato HH:MM"));
            }
        }
        out
    }

    /// Riporta nei limiti i valori scritti a mano e allinea `thresholds` a `percent.fiveHour`.
    /// Restituisce gli avvisi.
    pub fn normalize(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        let defaults = Self::default();
        for (name, list) in [
            ("percent.fiveHour", &mut self.percent.five_hour),
            ("percent.sevenDay", &mut self.percent.seven_day),
            ("modelLimit.thresholds", &mut self.model_limit.thresholds),
        ] {
            let clean = sanitize_thresholds(list);
            if clean != *list {
                warnings.push(format!("{name} corrette"));
                *list = clean;
            }
        }
        let p = &mut self.switch_preview.points_before;
        if !p.is_finite() || !(0.0..=50.0).contains(p) {
            warnings.push("switchPreview.pointsBefore riportato al default".into());
            *p = defaults.switch_preview.points_before;
        }
        let clamp = |v: &mut u32, lo: u32, hi: u32, name: &str, w: &mut Vec<String>| {
            let c = (*v).clamp(lo, hi);
            if c != *v {
                w.push(format!("{name} {v} riportato a {c}"));
                *v = c;
            }
        };
        clamp(
            &mut self.switch_preview.minutes_before,
            0,
            600,
            "switchPreview.minutesBefore",
            &mut warnings,
        );
        clamp(
            &mut self.reset_soon.minutes_before,
            1,
            1440,
            "resetSoon.minutesBefore",
            &mut warnings,
        );
        clamp(
            &mut self.login.days_before,
            1,
            60,
            "login.daysBefore",
            &mut warnings,
        );
        clamp(
            &mut self.stale.minutes,
            1,
            1440,
            "stale.minutes",
            &mut warnings,
        );
        for (name, v, d) in [
            (
                "quietHours.start",
                &mut self.quiet_hours.start,
                &defaults.quiet_hours.start,
            ),
            (
                "quietHours.end",
                &mut self.quiet_hours.end,
                &defaults.quiet_hours.end,
            ),
            (
                "dailySummary.time",
                &mut self.daily_summary.time,
                &defaults.daily_summary.time,
            ),
        ] {
            if parse_hhmm(v).is_none() {
                warnings.push(format!("{name} non valido: si usa {d}"));
                v.clone_from(d);
            }
        }
        self.sync_legacy();
        warnings
    }

    /// `thresholds` (versione 1) = `percent.fiveHour`.
    pub fn sync_legacy(&mut self) {
        self.thresholds.clone_from(&self.percent.five_hour);
    }

    /// Soglie della versione 1 (preferenze dell'interfaccia): valgono per 5h e 7d.
    pub fn set_legacy_thresholds(&mut self, t: &[f64]) {
        self.percent.five_hour = t.to_vec();
        self.percent.seven_day = t.to_vec();
        self.thresholds = t.to_vec();
    }

    /// Applica una JSON merge patch (RFC 7386) e valida il risultato; la config resta
    /// com'era se non è valida.
    pub fn merged(&self, patch: &serde_json::Value) -> Result<Self, String> {
        if !patch.is_object() {
            return Err("le notifiche vanno date come oggetto JSON".into());
        }
        let mut value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        merge_patch(&mut value, patch);
        // Chi cambia le soglie 5h dalla nuova interfaccia non deve vedersele riportare da
        // `thresholds`: la copia della versione 1 segue sempre `percent`.
        let mut next: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if patch.get("percent").is_none() {
            if let Some(t) = patch.get("thresholds").and_then(|v| v.as_array()) {
                let t: Vec<f64> = t.iter().filter_map(serde_json::Value::as_f64).collect();
                next.set_legacy_thresholds(&t);
            }
        }
        next.sync_legacy();
        let problems = next.problems();
        if problems.is_empty() {
            Ok(next)
        } else {
            Err(problems.join("; "))
        }
    }
}

/// JSON merge patch (RFC 7386): `null` toglie la chiave.
pub fn merge_patch(target: &mut serde_json::Value, patch: &serde_json::Value) {
    let Some(p) = patch.as_object() else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = serde_json::Value::Object(serde_json::Map::new());
    }
    if let Some(t) = target.as_object_mut() {
        for (k, v) in p {
            if v.is_null() {
                t.remove(k);
            } else {
                merge_patch(t.entry(k.clone()).or_insert(serde_json::Value::Null), v);
            }
        }
    }
}

/// Migrazione in lettura: senza `percent`, le soglie della versione 1 valgono per 5h e 7d.
pub(crate) fn migrate(obj: &mut serde_json::Map<String, serde_json::Value>) -> Option<String> {
    let n = obj.get_mut("notifications")?.as_object_mut()?;
    if n.contains_key("percent") {
        return None;
    }
    let old = n.get("thresholds")?.clone();
    old.as_array()?;
    n.insert(
        "percent".into(),
        serde_json::json!({ "fiveHour": old, "sevenDay": old }),
    );
    Some("notifications.thresholds migrate in percent (5h e 7d)".into())
}
