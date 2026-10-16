//! Motore delle regole (F3): la funzione pura [`decide`] trasforma stato, regole,
//! memoria e ora in una [`Decision`]. Niente I/O: lo esegue il runner (`headless`).
//!
//! Generalizza `cswap-guard`: principale, riserve in ordine, soglie 5h/7d,
//! ritorno al principale dopo il suo reset con isteresi, pausa minima tra due cambi.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::conflicts::Conflict;
use crate::cswap::{Account, ListResponse, Usage, UsageStatus, UsageWindow};
use crate::timeutil::parse_iso;

/// Account al massimo in riserve ed esclusi (controllo prima del salvataggio).
pub const MAX_RULE_ACCOUNTS: usize = 64;
/// Conflitti ignorati al massimo, e lunghezza massima di un id.
pub const MAX_IGNORED_CONFLICTS: usize = 100;
pub const MAX_CONFLICT_ID_LEN: usize = 200;

/// Criterio di scelta della riserva.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ReserveCriterion {
    /// Settimanale (7d) più bassa; a parità la 5h più bassa, poi l'ordine.
    #[default]
    LowestWeekly,
    /// Più margine rispetto alle soglie (come `cswap switch --strategy best`).
    Best,
    /// La prima utilizzabile nell'ordine delle riserve.
    FixedOrder,
}

/// Regole del motore, salvate nella config (`config.json`, sezione `engine`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineRules {
    /// Motore acceso. Spento al primo avvio.
    pub enabled: bool,
    /// Solo osservazione: decide e registra, ma non cambia account.
    pub dry_run: bool,
    /// Account principale (numero di cswap).
    pub primary: Option<u32>,
    /// Riserve in ordine di preferenza; vuoto = tutti gli altri account, in ordine di numero.
    pub reserves: Vec<u32>,
    /// Account mai scelti dal motore; se uno di questi è attivo il motore non lo tocca.
    pub excluded: Vec<u32>,
    /// Soglia di uscita sulle 5 ore, in percentuale.
    pub five_hour_threshold: f64,
    /// Soglia facoltativa sui 7 giorni (`null` = solo il blocco al 100%).
    pub seven_day_threshold: Option<f64>,
    pub reserve_criterion: ReserveCriterion,
    /// Torna al principale dopo il suo reset.
    pub return_to_primary: bool,
    /// Punti sotto la soglia che il principale deve avere per tornarci.
    pub return_hysteresis: f64,
    /// Pausa minima tra due cambi, in secondi.
    pub min_switch_gap_secs: u32,
    /// Età massima di un consumo per fidarsene, in secondi.
    pub max_usage_age_secs: u32,
    /// Conflitti noti da ignorare (`Conflict::id`, senza distinzione di maiuscole).
    pub ignored_conflicts: Vec<String>,
}

impl Default for EngineRules {
    fn default() -> Self {
        Self {
            enabled: false,
            dry_run: false,
            primary: None,
            reserves: Vec::new(),
            excluded: Vec::new(),
            five_hour_threshold: 95.0,
            seven_day_threshold: Some(98.0),
            reserve_criterion: ReserveCriterion::LowestWeekly,
            return_to_primary: true,
            return_hysteresis: 10.0,
            min_switch_gap_secs: 300,
            max_usage_age_secs: 900,
            ignored_conflicts: Vec::new(),
        }
    }
}

impl EngineRules {
    /// Problemi che impediscono al motore di agire (messaggi per l'utente).
    pub fn validate(&self) -> Vec<String> {
        let Some(primary) = self.primary else {
            return vec!["manca l'account principale".to_owned()];
        };
        let mut out = self.value_problems();
        if self.reserves.contains(&primary) {
            out.push("il principale non può essere anche una riserva".to_owned());
        }
        if self.excluded.contains(&primary) {
            out.push("il principale non può essere escluso".to_owned());
        }
        out
    }

    /// Controllo prima di salvare le regole dall'interfaccia: valori nei limiti sempre, il
    /// principale (e la sua coerenza con riserve ed esclusi) solo con il motore acceso o in
    /// osservazione; in più numeri degli account, riserve ripetute e lunghezze degli elenchi.
    pub fn validate_for_save(&self) -> Vec<String> {
        let mut out = if self.primary.is_some() || self.enabled || self.dry_run {
            self.validate()
        } else {
            self.value_problems()
        };
        let numbers = self
            .primary
            .iter()
            .chain(&self.reserves)
            .chain(&self.excluded);
        if numbers.clone().any(|n| *n == 0) {
            out.push("i numeri degli account partono da 1".to_owned());
        }
        if self.reserves.len() > MAX_RULE_ACCOUNTS || self.excluded.len() > MAX_RULE_ACCOUNTS {
            out.push(format!("al massimo {MAX_RULE_ACCOUNTS} account per elenco"));
        }
        let mut seen = std::collections::HashSet::new();
        if !self.reserves.iter().all(|n| seen.insert(*n)) {
            out.push("riserve ripetute".to_owned());
        }
        if self.ignored_conflicts.len() > MAX_IGNORED_CONFLICTS
            || self
                .ignored_conflicts
                .iter()
                .any(|c| c.trim().is_empty() || c.len() > MAX_CONFLICT_ID_LEN)
        {
            out.push("conflitti ignorati non validi".to_owned());
        }
        out
    }

    /// Soglie e isteresi nei limiti (non dipendono dagli account).
    fn value_problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        let pct_ok = |v: f64| v.is_finite() && v > 0.0 && v <= 100.0;
        if !pct_ok(self.five_hour_threshold) {
            out.push("soglia 5h fuori da 1–100".to_owned());
        }
        if self.seven_day_threshold.is_some_and(|v| !pct_ok(v)) {
            out.push("soglia 7d fuori da 1–100".to_owned());
        }
        if !(self.return_hysteresis.is_finite() && (0.0..=50.0).contains(&self.return_hysteresis)) {
            out.push("isteresi fuori da 0–50".to_owned());
        }
        out
    }

    fn seven_day_limit(&self) -> f64 {
        self.seven_day_threshold.unwrap_or(100.0)
    }

    fn is_reserve(&self, number: u32, list: &ListResponse) -> bool {
        Some(number) != self.primary
            && !self.excluded.contains(&number)
            && (self.reserves.contains(&number)
                || (self.reserves.is_empty() && list.accounts.iter().any(|a| a.number == number)))
    }

    /// Riserve nell'ordine di preferenza.
    fn reserve_order(&self, list: &ListResponse) -> Vec<u32> {
        if !self.reserves.is_empty() {
            return self.reserves.clone();
        }
        let mut all: Vec<u32> = list
            .accounts
            .iter()
            .map(|a| a.number)
            .filter(|n| self.is_reserve(*n, list))
            .collect();
        all.sort_unstable();
        all
    }
}

/// Memoria del motore tra un giro e l'altro (salvata dal runner).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineMemory {
    /// Ultimo cambio fatto dal motore (secondi Unix).
    #[ts(type = "number | null")]
    pub last_switch_at: Option<i64>,
    /// Quando il motore ha lasciato il principale.
    #[ts(type = "number | null")]
    pub left_primary_at: Option<i64>,
    /// Reset della finestra che ha fatto lasciare il principale: prima non si torna.
    #[ts(type = "number | null")]
    pub primary_reset_at: Option<i64>,
    /// Finestre al limite quando il motore ha lasciato il principale: l'isteresi del ritorno
    /// vale solo per loro. `None` (memoria assente o vecchia) = solo la 5h.
    pub primary_left_windows: Option<Vec<LimitWindow>>,
}

/// Finestra di consumo che ha fatto lasciare il principale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LimitWindow {
    FiveHour,
    SevenDay,
}

impl EngineMemory {
    /// Registra un cambio riuscito da `list` (stato prima del cambio) verso `to`.
    pub fn record_switch(&mut self, rules: &EngineRules, list: &ListResponse, to: u32, now: i64) {
        self.last_switch_at = Some(now);
        let from = list.active_account_number;
        if Some(to) == rules.primary {
            self.left_primary_at = None;
            self.primary_reset_at = None;
            self.primary_left_windows = None;
        } else if from.is_some() && from == rules.primary {
            let main = rules.primary.and_then(|p| find(list, p));
            self.left_primary_at = Some(now);
            self.primary_reset_at = main.and_then(|p| limiting_reset(p, rules, now));
            self.primary_left_windows =
                Some(main.map_or_else(Vec::new, |p| limiting_windows(p, rules, now)));
        }
    }
}

/// Stato osservato in un giro.
#[derive(Debug, Clone, Copy)]
pub struct Observation<'a> {
    pub list: &'a ListResponse,
    pub conflicts: &'a [Conflict],
}

/// Perché il motore non fa nulla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum IdleReason {
    /// Principale attivo e sotto soglia.
    BelowThreshold,
    /// Riserva attiva e utilizzabile; il principale non è ancora tornato disponibile.
    OnReserve,
    /// Pausa minima tra due cambi non ancora trascorsa.
    Cooldown,
    /// Consumi dell'account attivo sconosciuti o troppo vecchi.
    StaleUsage,
    /// L'account attivo è tra gli esclusi.
    ActiveExcluded,
    /// L'account attivo non è né il principale né una riserva.
    ActiveOutsideRules,
}

/// Motivo di un cambio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SwitchCause {
    PrimaryAtLimit,
    ReserveAtLimit,
    ActiveNeedsRelogin,
    /// Il principale ha superato il reset ed è sotto soglia meno l'isteresi.
    PrimaryReset,
    /// Le riserve sono al limite ma il principale è sotto soglia.
    ReservesAtLimit,
}

/// Perché il motore è sospeso (serve l'utente).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SuspendReason {
    /// Motore spento (default al primo avvio).
    Disabled,
    InvalidRules,
    /// `cswap auto`, `cswap-guard` o simili attivi.
    Conflict,
    NoActiveAccount,
    /// Serve un cambio ma nessun account è utilizzabile.
    NoViableTarget,
}

/// Esito di [`decide`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Decision {
    NoAction {
        reason: IdleReason,
    },
    Switch {
        from: u32,
        to: u32,
        cause: SwitchCause,
    },
    ReturnToPrimary {
        from: u32,
        to: u32,
        cause: SwitchCause,
    },
    Suspended {
        reason: SuspendReason,
        /// Dettagli: id dei conflitti o problemi delle regole.
        detail: Vec<String>,
    },
}

impl Decision {
    /// Account verso cui cambiare, se la decisione è un cambio.
    pub fn target(&self) -> Option<u32> {
        match self {
            Self::Switch { to, .. } | Self::ReturnToPrimary { to, .. } => Some(*to),
            _ => None,
        }
    }

    fn suspended(reason: SuspendReason) -> Self {
        Self::Suspended {
            reason,
            detail: Vec::new(),
        }
    }

    fn idle(reason: IdleReason) -> Self {
        Self::NoAction { reason }
    }
}

/// Conflitti certi che non sono tra quelli ignorati dalle regole.
pub fn blocking_conflicts<'a>(conflicts: &'a [Conflict], rules: &EngineRules) -> Vec<&'a Conflict> {
    conflicts
        .iter()
        .filter(|c| c.certain)
        .filter(|c| {
            !rules
                .ignored_conflicts
                .iter()
                .any(|i| i.trim().eq_ignore_ascii_case(&c.id))
        })
        .collect()
}

/// Sospensione che non dipende dagli account: motore spento, conflitti, regole non valide
/// (le prime verifiche di [`decide`], usate anche dove non c'è una lettura di cswap).
pub fn suspension(rules: &EngineRules, conflicts: &[Conflict]) -> Option<Decision> {
    if !(rules.enabled || rules.dry_run) {
        return Some(Decision::suspended(SuspendReason::Disabled));
    }
    // Prima i conflitti: un altro motore attivo conta più di regole incomplete.
    let blocking = blocking_conflicts(conflicts, rules);
    if !blocking.is_empty() {
        return Some(Decision::Suspended {
            reason: SuspendReason::Conflict,
            detail: blocking.iter().map(|c| c.id.clone()).collect(),
        });
    }
    let problems = rules.validate();
    (!problems.is_empty()).then_some(Decision::Suspended {
        reason: SuspendReason::InvalidRules,
        detail: problems,
    })
}

/// Stato del motore mostrato nell'interfaccia (dall'ultimo giro che ha deciso).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EngineView {
    /// Il lock è di questa istanza: storico e cambi sono suoi.
    pub holder: bool,
    pub enabled: bool,
    pub dry_run: bool,
    /// Motivo della sospensione (anche `disabled` con il motore spento).
    pub suspended: Option<SuspendReason>,
    /// Dettagli della sospensione (per esempio gli id dei conflitti).
    pub detail: Vec<String>,
}

impl EngineView {
    pub fn new(holder: bool, rules: &EngineRules, decision: Option<&Decision>) -> Self {
        let (suspended, detail) = match decision {
            Some(Decision::Suspended { reason, detail }) => (Some(*reason), detail.clone()),
            _ => (None, Vec::new()),
        };
        Self {
            holder,
            enabled: rules.enabled,
            dry_run: rules.dry_run,
            suspended,
            detail,
        }
    }

    /// Sospeso per un motivo che chiede attenzione (non il motore spento).
    pub fn paused(&self) -> bool {
        matches!(self.suspended, Some(r) if r != SuspendReason::Disabled)
    }
}

/// Regole salvate, stato del motore e conflitti noti (pagina «Regole» dell'interfaccia).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EngineState {
    pub rules: EngineRules,
    /// `null` se il motore non ha ancora fatto un giro.
    pub view: Option<EngineView>,
    pub conflicts: Vec<Conflict>,
}

/// Decide cosa fare. Pura: stessi argomenti, stessa decisione.
pub fn decide(obs: Observation<'_>, rules: &EngineRules, mem: &EngineMemory, now: i64) -> Decision {
    if let Some(suspended) = suspension(rules, obs.conflicts) {
        return suspended;
    }
    let list = obs.list;
    let primary = rules.primary.unwrap_or_default();
    let Some(active) = list.active_account_number.and_then(|n| find(list, n)) else {
        return Decision::suspended(SuspendReason::NoActiveAccount);
    };
    let from = active.number;
    if rules.excluded.contains(&from) {
        return Decision::idle(IdleReason::ActiveExcluded);
    }
    let in_cooldown = mem
        .last_switch_at
        .is_some_and(|t| now < t.saturating_add(i64::from(rules.min_switch_gap_secs)));
    let relogin = active.usage_status == UsageStatus::ReloginRequired;
    let hard = relogin || at_limit(active, rules, now, true);
    let leaving = relogin || at_limit(active, rules, now, false);

    if from == primary {
        if !leaving {
            return Decision::idle(if fresh(active, rules, now) {
                IdleReason::BelowThreshold
            } else {
                IdleReason::StaleUsage
            });
        }
        if in_cooldown && !hard {
            return Decision::idle(IdleReason::Cooldown);
        }
        let cause = if relogin {
            SwitchCause::ActiveNeedsRelogin
        } else {
            SwitchCause::PrimaryAtLimit
        };
        return match pick_reserve(list, rules, now, from) {
            Some(to) => Decision::Switch { from, to, cause },
            None => Decision::suspended(SuspendReason::NoViableTarget),
        };
    }

    if !rules.is_reserve(from, list) {
        return Decision::idle(IdleReason::ActiveOutsideRules);
    }
    let main = find(list, primary);
    if rules.return_to_primary && main.is_some_and(|p| ready_to_return(p, rules, mem, now)) {
        if in_cooldown && !hard {
            return Decision::idle(IdleReason::Cooldown);
        }
        return Decision::ReturnToPrimary {
            from,
            to: primary,
            cause: SwitchCause::PrimaryReset,
        };
    }
    if !leaving {
        return Decision::idle(if fresh(active, rules, now) {
            IdleReason::OnReserve
        } else {
            IdleReason::StaleUsage
        });
    }
    if in_cooldown && !hard {
        return Decision::idle(IdleReason::Cooldown);
    }
    let cause = if relogin {
        SwitchCause::ActiveNeedsRelogin
    } else {
        SwitchCause::ReserveAtLimit
    };
    if let Some(to) = pick_reserve(list, rules, now, from) {
        return Decision::Switch { from, to, cause };
    }
    if main.is_some_and(|p| eligible(p, rules, now, NO_MARGIN)) {
        return Decision::ReturnToPrimary {
            from,
            to: primary,
            cause: SwitchCause::ReservesAtLimit,
        };
    }
    Decision::suspended(SuspendReason::NoViableTarget)
}

fn find(list: &ListResponse, number: u32) -> Option<&Account> {
    list.accounts.iter().find(|a| a.number == number)
}

/// Età del consumo in secondi (da `usageAgeSeconds`, o da `usageFetchedAt`).
fn usage_age(a: &Account, now: i64) -> Option<f64> {
    a.usage_age_seconds.or_else(|| {
        a.usage_fetched_at
            .as_deref()
            .and_then(parse_iso)
            .map(|t| (now - t) as f64)
    })
}

/// Consumo `ok`, presente e non troppo vecchio (età assente = appena letto).
fn fresh(a: &Account, rules: &EngineRules, now: i64) -> bool {
    a.usage_status == UsageStatus::Ok
        && a.usage.is_some()
        && usage_age(a, now).is_none_or(|age| age <= f64::from(rules.max_usage_age_secs))
}

fn reset_passed(w: &UsageWindow, now: i64) -> bool {
    w.resets_at
        .as_deref()
        .and_then(parse_iso)
        .is_some_and(|r| r <= now)
}

/// Percentuale ancora valida: dopo il reset la finestra riparte da zero.
fn effective_pct(w: Option<&UsageWindow>, now: i64) -> f64 {
    match w {
        Some(w) if !reset_passed(w, now) => w.pct,
        _ => 0.0,
    }
}

/// `true` se l'account è al limite. Vale anche con dati vecchi o con `lastGoodUsage`:
/// prima del reset il consumo può solo crescere. `hard` usa il 100% invece delle soglie.
fn at_limit(a: &Account, rules: &EngineRules, now: i64, hard: bool) -> bool {
    let Some(u) = a.usage.as_ref().or(a.last_good_usage.as_ref()) else {
        return false;
    };
    let (t5, t7) = if hard {
        (100.0, 100.0)
    } else {
        (rules.five_hour_threshold, rules.seven_day_limit())
    };
    effective_pct(u.five_hour.as_ref(), now) >= t5 || effective_pct(u.seven_day.as_ref(), now) >= t7
}

/// Margini sottratti alle soglie (5h, 7d).
type Margins = (f64, f64);
const NO_MARGIN: Margins = (0.0, 0.0);

fn below(u: &Usage, rules: &EngineRules, now: i64, (m5, m7): Margins) -> bool {
    effective_pct(u.five_hour.as_ref(), now) < rules.five_hour_threshold - m5
        && effective_pct(u.seven_day.as_ref(), now) < rules.seven_day_limit() - m7
}

/// Account su cui il motore può cambiare: abilitato, non escluso, dati freschi, sotto soglia.
fn eligible(a: &Account, rules: &EngineRules, now: i64, margins: Margins) -> bool {
    !a.is_disabled()
        && !rules.excluded.contains(&a.number)
        && fresh(a, rules, now)
        && a.usage
            .as_ref()
            .is_some_and(|u| below(u, rules, now, margins))
}

/// Ritorno al principale: reset passato e isteresi solo sulle finestre che l'hanno fatto lasciare.
fn ready_to_return(p: &Account, rules: &EngineRules, mem: &EngineMemory, now: i64) -> bool {
    let h = rules.return_hysteresis;
    let margins = match mem.primary_left_windows.as_deref() {
        None => (h, 0.0),
        Some(w) => (
            if w.contains(&LimitWindow::FiveHour) {
                h
            } else {
                0.0
            },
            if w.contains(&LimitWindow::SevenDay) {
                h
            } else {
                0.0
            },
        ),
    };
    mem.primary_reset_at.is_none_or(|r| now >= r) && eligible(p, rules, now, margins)
}

/// Finestre dell'account oltre le soglie del motore.
fn limiting_windows(a: &Account, rules: &EngineRules, now: i64) -> Vec<LimitWindow> {
    let Some(u) = a.usage.as_ref().or(a.last_good_usage.as_ref()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if effective_pct(u.five_hour.as_ref(), now) >= rules.five_hour_threshold {
        out.push(LimitWindow::FiveHour);
    }
    if effective_pct(u.seven_day.as_ref(), now) >= rules.seven_day_limit() {
        out.push(LimitWindow::SevenDay);
    }
    out
}

/// Reset più lontano tra le finestre al limite (quando il principale torna utilizzabile).
fn limiting_reset(a: &Account, rules: &EngineRules, now: i64) -> Option<i64> {
    let u = a.usage.as_ref().or(a.last_good_usage.as_ref())?;
    [
        (u.five_hour.as_ref(), rules.five_hour_threshold),
        (u.seven_day.as_ref(), rules.seven_day_limit()),
    ]
    .into_iter()
    .filter_map(|(w, t)| w.filter(|w| effective_pct(Some(w), now) >= t))
    .filter_map(|w| w.resets_at.as_deref().and_then(parse_iso))
    .max()
}

fn pick_reserve(list: &ListResponse, rules: &EngineRules, now: i64, active: u32) -> Option<u32> {
    let candidates: Vec<(usize, &Account)> = rules
        .reserve_order(list)
        .into_iter()
        .enumerate()
        .filter(|(_, n)| *n != active && Some(*n) != rules.primary)
        .filter_map(|(i, n)| find(list, n).map(|a| (i, a)))
        .filter(|(_, a)| eligible(a, rules, now, NO_MARGIN))
        .collect();
    let pct = |a: &Account, seven: bool| {
        let u = a.usage.as_ref();
        let w = u.and_then(|u| {
            if seven {
                u.seven_day.as_ref()
            } else {
                u.five_hour.as_ref()
            }
        });
        effective_pct(w, now)
    };
    let chosen = match rules.reserve_criterion {
        ReserveCriterion::FixedOrder => candidates.first(),
        ReserveCriterion::LowestWeekly => candidates.iter().min_by(|(i, a), (j, b)| {
            pct(a, true)
                .total_cmp(&pct(b, true))
                .then(pct(a, false).total_cmp(&pct(b, false)))
                .then(i.cmp(j))
        }),
        ReserveCriterion::Best => {
            let margin = |a: &Account| {
                (rules.five_hour_threshold - pct(a, false))
                    .min(rules.seven_day_limit() - pct(a, true))
            };
            candidates
                .iter()
                .min_by(|(i, a), (j, b)| margin(b).total_cmp(&margin(a)).then(i.cmp(j)))
        }
    };
    chosen.map(|(_, a)| a.number)
}

#[cfg(test)]
pub(crate) mod tests;
