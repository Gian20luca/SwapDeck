//! Motore delle regole (F3): la funzione pura [`evaluate`] trasforma stato, regole,
//! memoria e ora in una [`Decision`] con il suo motivo e il candidato attuale. Niente I/O:
//! lo esegue il runner (`headless`).
//!
//! Le utenze sono una lista con priorità ([`EngineRules::order`]): la prima è la principale.
//! Con il criterio «ordine» il motore usa sempre la prima utenza libera della lista (anche il
//! ritorno va alla prima libera); con gli altri criteri il criterio sceglie la riserva quando
//! l'attiva supera la soglia e il ritorno va alla principale dopo il suo reset, come
//! `cswap-guard`. Isteresi del ritorno sulle finestre che hanno fatto lasciare l'utenza,
//! pausa minima tra due cambi. Le regole sono quelle della mappa attiva ([`profiles`]).

mod criteria;
mod explain;
pub mod profiles;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::conflicts::Conflict;
use crate::cswap::{Account, ListResponse, Usage, UsageStatus, UsageWindow};
use crate::timeutil::parse_iso;

pub use criteria::ScoreWeights;
pub use profiles::{EngineProfiles, PresetKind, ProfileCommand, RulesProfile};

/// Account al massimo in ordine ed esclusi (controllo prima del salvataggio).
pub const MAX_RULE_ACCOUNTS: usize = 64;
/// Conflitti ignorati al massimo, e lunghezza massima di un id.
pub const MAX_IGNORED_CONFLICTS: usize = 100;
pub const MAX_CONFLICT_ID_LEN: usize = 200;
/// Lunghezza massima del nome del modello di [`ReserveCriterion::ModelHeadroom`].
pub const MAX_MODEL_NAME_LEN: usize = 64;

/// Criterio di scelta della riserva quando l'attiva supera la soglia. A parità vale sempre
/// l'ordine della lista; con i dati che servono al criterio mancanti la candidata si esclude.
/// Se il criterio non può scegliere nessuna candidata (dati mancanti in tutte, modello assente,
/// criterio sconosciuto) ripiega sulla settimanale più bassa e, senza 7d, sull'ordine della
/// lista, e lo scrive nel motivo.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ReserveCriterion {
    /// Settimanale (7d) più bassa; a parità la 5h più bassa.
    #[default]
    LowestWeekly,
    /// Più margine rispetto alle soglie (come `cswap switch --strategy best`).
    Best,
    /// Flusso a lista: sempre la prima utenza libera nell'ordine, anche al ritorno.
    FixedOrder,
    /// La finestra 7d che si azzera prima (consuma quota che andrebbe persa); a parità la 7d
    /// più bassa.
    ResetSoonest,
    /// Chi arriva al reset 7d senza esaurirsi al ritmo attuale (`willLastToReset`), con il
    /// margine maggiore sul ritmo atteso (`expectedPct - pct`); se nessuna ci arriva, chi si
    /// esaurisce più tardi (`projectedExhaustionAt`).
    PaceSafe,
    /// La finestra 5h più libera; a parità la 7d più bassa.
    Lowest5h,
    /// Punteggio pesato di margine 5h, margine 7d, vicinanza del reset 7d e ritmo
    /// ([`ScoreWeights`]).
    BalancedScore,
    /// Livella le settimanali: la candidata più sotto la media 7d della lista; a parità la 5h
    /// più bassa.
    EvenSpread,
    /// Più margine sul limite settimanale del modello [`EngineRules::headroom_model`]
    /// (`scoped[]`); a parità la 7d più bassa.
    ModelHeadroom,
    /// Criterio non noto (scritto da una versione più recente): il motore ripiega sulla
    /// settimanale più bassa e il valore originale torna nel file al salvataggio.
    #[serde(untagged)]
    #[ts(skip)]
    Unknown(String),
}

/// Regole del motore, salvate nella config (`config.json`, sezione `engine`): sono le regole
/// della mappa attiva.
///
/// Lista delle utenze: quando c'è, `order` è l'unica fonte di verità e `primary` e `reserves`
/// ne sono derivati (il primo e i successivi; si riscrivono a ogni salvataggio). Senza `order`
/// valgono `primary` e `reserves` (forma precedente). Un `primary` esplicito a `null` insieme a
/// `order` vuol dire «nessuna principale»: la lista si scarta e restano solo le `reserves`
/// (bozza, motore fermo finché manca la principale).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(remote = "Self", rename_all = "camelCase", default)]
pub struct EngineRules {
    /// Motore acceso. Spento al primo avvio.
    pub enabled: bool,
    /// Solo osservazione: decide e registra, ma non cambia account.
    pub dry_run: bool,
    /// Utenze in ordine di priorità (numeri di cswap): la prima è la principale. Con la sola
    /// principale le altre sono tutte quelle di cswap, in ordine di numero. Le utenze che non
    /// esistono in cswap si saltano. Quando c'è prevale su `primary` e `reserves`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub order: Option<Vec<u32>>,
    /// Principale: primo elemento di `order`, derivato. `null` esplicito con `order` presente =
    /// nessuna principale.
    pub primary: Option<u32>,
    /// Riserve in ordine di preferenza: `order` senza la principale, derivato.
    pub reserves: Vec<u32>,
    /// Account mai scelti dal motore; se uno di questi è attivo il motore non lo tocca.
    pub excluded: Vec<u32>,
    /// Soglia di uscita sulle 5 ore, in percentuale.
    pub five_hour_threshold: f64,
    /// Soglia facoltativa sui 7 giorni (`null` = solo il blocco al 100%).
    pub seven_day_threshold: Option<f64>,
    pub reserve_criterion: ReserveCriterion,
    /// Torna alla principale dopo il suo reset (con il criterio «ordine»: alla prima libera).
    pub return_to_primary: bool,
    /// Punti sotto la soglia che un'utenza lasciata deve avere per tornarci.
    pub return_hysteresis: f64,
    /// Pausa minima tra due cambi, in secondi.
    pub min_switch_gap_secs: u32,
    /// Età massima di un consumo per fidarsene, in secondi.
    pub max_usage_age_secs: u32,
    /// Pesi di [`ReserveCriterion::BalancedScore`]; assenti = [`ScoreWeights::default`].
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub score_weights: Option<ScoreWeights>,
    /// Modello di [`ReserveCriterion::ModelHeadroom`] (nome di `scoped[]`, per esempio «Fable»).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub headroom_model: Option<String>,
    /// Conflitti noti da ignorare (`Conflict::id`, senza distinzione di maiuscole).
    pub ignored_conflicts: Vec<String>,
}

impl Default for EngineRules {
    fn default() -> Self {
        Self {
            enabled: false,
            dry_run: false,
            order: None,
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
            score_weights: None,
            headroom_model: None,
            ignored_conflicts: Vec::new(),
        }
    }
}

/// Lettura con la regola di [`EngineRules`] su `primary` a `null` (serve sapere se il campo
/// c'era: si passa da un valore JSON), poi i campi derivati riallineati.
impl<'de> Deserialize<'de> for EngineRules {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        let no_primary = value.get("primary").is_some_and(serde_json::Value::is_null)
            && value.get("order").is_some_and(|o| !o.is_null());
        let mut rules = Self::deserialize(value).map_err(serde::de::Error::custom)?;
        if no_primary {
            rules.order = None;
        }
        rules.sync_order();
        Ok(rules)
    }
}

/// Scrittura sempre riallineata: `primary` e `reserves` derivati da `order`.
impl Serialize for EngineRules {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut rules = self.clone();
        rules.sync_order();
        Self::serialize(&rules, s)
    }
}

impl EngineRules {
    /// Lista configurata: `order` se c'è, altrimenti `primary` più `reserves`.
    pub fn configured_order(&self) -> Vec<u32> {
        match (&self.order, self.primary) {
            (Some(order), _) => order.clone(),
            (None, Some(p)) => std::iter::once(p)
                .chain(self.reserves.iter().copied())
                .collect(),
            (None, None) => Vec::new(),
        }
    }

    /// Riallinea `primary` e `reserves` con `order` (o crea `order` dalla forma precedente):
    /// in lettura e al salvataggio. Una lista vuota diventa assente; senza principale né
    /// `order` non cambia nulla: le riserve di una bozza restano.
    pub fn sync_order(&mut self) {
        let order = self.configured_order();
        match order.split_first() {
            Some((first, rest)) => {
                self.primary = Some(*first);
                self.reserves = rest.to_vec();
                self.order = Some(order);
            }
            None if self.order.is_some() => {
                self.order = None;
                self.primary = None;
                self.reserves.clear();
            }
            None => {}
        }
    }

    /// Numeri della lista che non esistono in cswap (saltati dal motore; avviso per l'interfaccia).
    pub fn missing_accounts(&self, list: &ListResponse) -> Vec<u32> {
        self.configured_order()
            .into_iter()
            .filter(|n| find(list, *n).is_none())
            .collect()
    }

    /// Come [`Self::validate`], in più i problemi che si vedono solo con la lista di cswap: il
    /// modello di «margine sul modello» che nessuna utenza riporta (solo se qualche utenza
    /// riporta dei modelli, cioè se la lista dei modelli è nota).
    pub fn validate_with_list(&self, list: &ListResponse) -> Vec<String> {
        let mut out = self.validate();
        if self.reserve_criterion == ReserveCriterion::ModelHeadroom {
            if let Some(model) = self.model() {
                let names = scoped_names(list);
                if !names.is_empty() && !names.iter().any(|n| n.eq_ignore_ascii_case(model)) {
                    out.push(format!(
                        "modello «{model}» sconosciuto: nessuna utenza lo riporta"
                    ));
                }
            }
        }
        out
    }

    /// Problemi che impediscono al motore di agire (messaggi per l'utente).
    pub fn validate(&self) -> Vec<String> {
        let order = self.configured_order();
        let Some((primary, rest)) = order.split_first() else {
            return vec!["manca l'account principale".to_owned()];
        };
        let mut out = self.value_problems();
        if rest.contains(primary) {
            out.push("il principale non può essere anche una riserva".to_owned());
        }
        if self.excluded.contains(primary) {
            out.push("il principale non può essere escluso".to_owned());
        }
        out
    }

    /// Controllo prima di salvare le regole dall'interfaccia: valori nei limiti sempre, il
    /// principale (e la sua coerenza con riserve ed esclusi) solo con il motore acceso o in
    /// osservazione; in più numeri degli account, riserve ripetute e lunghezze degli elenchi.
    pub fn validate_for_save(&self) -> Vec<String> {
        let order = self.configured_order();
        let mut out = if !order.is_empty() || self.enabled || self.dry_run {
            self.validate()
        } else {
            self.value_problems()
        };
        let numbers = self
            .primary
            .iter()
            .chain(&self.reserves)
            .chain(self.order.iter().flatten())
            .chain(&self.excluded);
        if numbers.clone().any(|n| *n == 0) {
            out.push("i numeri degli account partono da 1".to_owned());
        }
        if self.reserves.len() > MAX_RULE_ACCOUNTS
            || self.excluded.len() > MAX_RULE_ACCOUNTS
            || self
                .order
                .as_ref()
                .is_some_and(|o| o.len() > MAX_RULE_ACCOUNTS + 1)
        {
            out.push(format!("al massimo {MAX_RULE_ACCOUNTS} account per elenco"));
        }
        let mut seen = std::collections::HashSet::new();
        if !order.iter().skip(1).all(|n| seen.insert(*n)) {
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
        if self
            .headroom_model
            .as_deref()
            .is_some_and(|m| m.len() > MAX_MODEL_NAME_LEN || m.chars().any(char::is_control))
        {
            out.push("nome del modello non valido".to_owned());
        }
        out
    }

    /// Soglie, isteresi e parametri del criterio nei limiti (non dipendono dagli account).
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
        if self.score_weights.is_some_and(|w| !w.is_valid()) {
            out.push("pesi del punteggio non validi (0–100, almeno uno positivo)".to_owned());
        }
        if self.reserve_criterion == ReserveCriterion::ModelHeadroom && self.model().is_none() {
            out.push("manca il modello del criterio «margine sul modello»".to_owned());
        }
        out
    }

    fn seven_day_limit(&self) -> f64 {
        self.seven_day_threshold.unwrap_or(100.0)
    }

    fn weights(&self) -> ScoreWeights {
        self.score_weights.unwrap_or_default()
    }

    fn model(&self) -> Option<&str> {
        self.headroom_model
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
    }

    /// Lista del motore: utenze configurate presenti in cswap, senza ripetizioni né esclusi;
    /// con la sola principale configurata, tutte le altre in ordine di numero.
    fn effective_order(&self, list: &ListResponse) -> Vec<u32> {
        let configured = self.configured_order();
        let mut out: Vec<u32> = Vec::new();
        for n in &configured {
            if !out.contains(n) && !self.excluded.contains(n) && find(list, *n).is_some() {
                out.push(*n);
            }
        }
        if configured.len() == 1 {
            let mut others: Vec<u32> = list
                .accounts
                .iter()
                .map(|a| a.number)
                .filter(|n| !configured.contains(n) && !self.excluded.contains(n))
                .collect();
            others.sort_unstable();
            others.dedup();
            out.extend(others);
        }
        out
    }
}

/// Memoria del motore tra un giro e l'altro (salvata dal runner).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineMemory {
    /// Ultimo cambio fatto dal motore (secondi Unix).
    #[ts(type = "number | null")]
    pub last_switch_at: Option<i64>,
    /// Forma precedente (solo la principale): letta, poi spostata in `left_accounts`.
    #[ts(type = "number | null")]
    pub left_primary_at: Option<i64>,
    /// Forma precedente: reset della finestra che ha fatto lasciare la principale.
    #[ts(type = "number | null")]
    pub primary_reset_at: Option<i64>,
    /// Forma precedente: finestre al limite quando il motore ha lasciato la principale
    /// (`None` = solo la 5h).
    pub primary_left_windows: Option<Vec<LimitWindow>>,
    /// Utenze lasciate dal motore e non ancora riprese: il ritorno aspetta il loro reset e
    /// l'isteresi vale solo per le finestre che le hanno fatte lasciare.
    pub left_accounts: Vec<LeftAccount>,
}

/// Utenza lasciata dal motore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LeftAccount {
    pub number: u32,
    #[ts(type = "number")]
    pub left_at: i64,
    /// Reset più lontano tra le finestre al limite: prima non si torna.
    #[ts(type = "number | null")]
    pub reset_at: Option<i64>,
    /// Finestre al limite quando è stata lasciata (vuoto = lasciata per altri motivi).
    pub windows: Vec<LimitWindow>,
}

/// Finestra di consumo che ha fatto lasciare un'utenza.
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
        self.left_accounts.retain(|r| r.number != to);
        let Some(from) = list.active_account_number.filter(|f| *f != to) else {
            return;
        };
        let account = find(list, from);
        self.left_accounts.retain(|r| r.number != from);
        self.left_accounts.push(LeftAccount {
            number: from,
            left_at: now,
            reset_at: account.and_then(|a| limiting_reset(a, rules, now)),
            windows: account.map_or_else(Vec::new, |a| limiting_windows(a, rules, now)),
        });
        if self.left_accounts.len() > MAX_RULE_ACCOUNTS {
            self.left_accounts.remove(0);
        }
    }

    /// C'è ancora la forma precedente (da assorbire con [`Self::absorb_legacy`]).
    pub fn has_legacy(&self) -> bool {
        self.left_primary_at.is_some()
            || self.primary_reset_at.is_some()
            || self.primary_left_windows.is_some()
    }

    /// Sposta la forma precedente (riferita alla principale di `rules`) in `left_accounts`.
    /// Va chiamata una volta sola, al caricamento della memoria, con le regole della config di
    /// allora: dopo un cambio di mappa la principale potrebbe essere un'altra. Il motore legge
    /// solo `left_accounts`.
    pub fn absorb_legacy(&mut self, rules: &EngineRules) {
        if !self.has_legacy() {
            return;
        }
        if let Some(&p) = rules.configured_order().first() {
            if !self.left_accounts.iter().any(|r| r.number == p) {
                self.left_accounts.push(LeftAccount {
                    number: p,
                    left_at: self.left_primary_at.unwrap_or_default(),
                    reset_at: self.primary_reset_at,
                    windows: self
                        .primary_left_windows
                        .take()
                        .unwrap_or_else(|| vec![LimitWindow::FiveHour]),
                });
            }
        }
        self.left_primary_at = None;
        self.primary_reset_at = None;
        self.primary_left_windows = None;
    }

    fn left(&self, number: u32) -> Option<&LeftAccount> {
        self.left_accounts.iter().find(|r| r.number == number)
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
    /// Criterio «ordine»: un'utenza che viene prima nella lista è di nuovo libera.
    EarlierAvailable,
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

/// Dove andrebbe il motore adesso se l'attiva superasse la soglia (tooltip delle soglie).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub number: u32,
    pub criterion: ReserveCriterion,
    /// Nessuna riserva libera: si tornerebbe a un'utenza precedente sotto soglia.
    pub fallback: bool,
    /// Motivo leggibile, per esempio «reset settimanale più vicino (tra 20 h)».
    pub why: String,
}

/// Esito di [`evaluate`]: decisione, motivo leggibile e candidato attuale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Evaluation {
    pub decision: Decision,
    /// Per esempio «5h di links-enrico al 95%: passo a links-erica, reset settimanale più
    /// vicino (tra 20 h)». Mai email: alias o numero.
    pub reason: String,
    /// `null` senza account attivo nella lista o senza utenze libere.
    pub candidate: Option<Candidate>,
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
    /// Motivo leggibile dell'ultima decisione.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
    /// Dove andrebbe il motore se l'attiva superasse la soglia adesso.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub candidate: Option<Candidate>,
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
            reason: None,
            candidate: None,
        }
    }

    /// Vista con motivo e candidato di una valutazione.
    pub fn explained(mut self, reason: Option<String>, candidate: Option<Candidate>) -> Self {
        self.reason = reason;
        self.candidate = candidate;
        self
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
    /// Avvisi che si vedono solo con la lista di cswap (per esempio un modello di «margine sul
    /// modello» che nessuna utenza riporta); vuoto se la lista non è nota.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub warnings: Vec<String>,
}

impl EngineRules {
    /// I soli problemi di [`Self::validate_with_list`] che [`Self::validate`] non vede.
    pub fn list_warnings(&self, list: &ListResponse) -> Vec<String> {
        let base = self.validate();
        self.validate_with_list(list)
            .into_iter()
            .filter(|p| !base.contains(p))
            .collect()
    }
}

/// Decide cosa fare. Pura: stessi argomenti, stessa decisione.
pub fn decide(obs: Observation<'_>, rules: &EngineRules, mem: &EngineMemory, now: i64) -> Decision {
    evaluate(obs, rules, mem, now).decision
}

/// Come [`decide`], con il motivo leggibile e il candidato attuale.
pub fn evaluate(
    obs: Observation<'_>,
    rules: &EngineRules,
    mem: &EngineMemory,
    now: i64,
) -> Evaluation {
    let ctx = Ctx::new(obs.list, rules, mem, now);
    let (decision, pick) = ctx.decide(obs.conflicts);
    let candidate = ctx.candidate();
    let reason = explain::reason(&ctx, &decision, pick.as_ref());
    Evaluation {
        decision,
        reason,
        candidate,
    }
}

/// Riserva scelta da [`Ctx::pick_target`].
#[derive(Debug, Clone)]
struct Pick {
    number: u32,
    fallback: bool,
    why: String,
}

/// Dati di un giro: lista del motore già calcolata.
struct Ctx<'a> {
    list: &'a ListResponse,
    rules: &'a EngineRules,
    mem: &'a EngineMemory,
    now: i64,
    order: Vec<u32>,
}

impl<'a> Ctx<'a> {
    fn new(
        list: &'a ListResponse,
        rules: &'a EngineRules,
        mem: &'a EngineMemory,
        now: i64,
    ) -> Self {
        Self {
            list,
            rules,
            mem,
            now,
            order: rules.effective_order(list),
        }
    }

    fn account(&self, number: u32) -> Option<&'a Account> {
        find(self.list, number)
    }

    fn flow(&self) -> bool {
        self.rules.reserve_criterion == ReserveCriterion::FixedOrder
    }

    fn eligible(&self, a: &Account, margins: Margins) -> bool {
        eligible(a, self.rules, self.now, margins)
    }

    /// Ritorno a un'utenza lasciata: reset passato e isteresi solo sulle finestre che l'hanno
    /// fatta lasciare; mai lasciata (o memoria assente) = isteresi sulla sola 5h.
    fn return_ready(&self, a: &Account) -> bool {
        let h = self.rules.return_hysteresis;
        let left = self.mem.left(a.number);
        let margins = match left.map(|l| l.windows.as_slice()) {
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
        left.and_then(|l| l.reset_at).is_none_or(|r| self.now >= r) && self.eligible(a, margins)
    }

    fn decide(&self, conflicts: &[Conflict]) -> (Decision, Option<Pick>) {
        if let Some(suspended) = suspension(self.rules, conflicts) {
            return (suspended, None);
        }
        let now = self.now;
        let rules = self.rules;
        let Some(active) = self
            .list
            .active_account_number
            .and_then(|n| self.account(n))
        else {
            return (Decision::suspended(SuspendReason::NoActiveAccount), None);
        };
        let from = active.number;
        if rules.excluded.contains(&from) {
            return (Decision::idle(IdleReason::ActiveExcluded), None);
        }
        let Some(k) = self.order.iter().position(|n| *n == from) else {
            return (Decision::idle(IdleReason::ActiveOutsideRules), None);
        };
        let primary = self.order[0];
        let in_cooldown = self
            .mem
            .last_switch_at
            .is_some_and(|t| now < t.saturating_add(i64::from(rules.min_switch_gap_secs)));
        let relogin = active.usage_status == UsageStatus::ReloginRequired;
        let hard = relogin || at_limit(active, rules, now, true);
        let leaving = relogin || at_limit(active, rules, now, false);
        let idle_fresh = |ok: IdleReason| {
            Decision::idle(if fresh(active, rules, now) {
                ok
            } else {
                IdleReason::StaleUsage
            })
        };

        if k == 0 {
            if !leaving {
                return (idle_fresh(IdleReason::BelowThreshold), None);
            }
            if in_cooldown && !hard {
                return (Decision::idle(IdleReason::Cooldown), None);
            }
            let cause = if relogin {
                SwitchCause::ActiveNeedsRelogin
            } else {
                SwitchCause::PrimaryAtLimit
            };
            return match self.pick_target(k) {
                Some(p) => (
                    Decision::Switch {
                        from,
                        to: p.number,
                        cause,
                    },
                    Some(p),
                ),
                None => (Decision::suspended(SuspendReason::NoViableTarget), None),
            };
        }

        if rules.return_to_primary {
            let back = if self.flow() {
                (0..k).find(|&j| {
                    self.account(self.order[j])
                        .is_some_and(|a| self.return_ready(a))
                })
            } else {
                self.account(primary)
                    .is_some_and(|a| self.return_ready(a))
                    .then_some(0)
            };
            if let Some(j) = back {
                if in_cooldown && !hard {
                    return (Decision::idle(IdleReason::Cooldown), None);
                }
                let to = self.order[j];
                let decision = if j == 0 {
                    Decision::ReturnToPrimary {
                        from,
                        to,
                        cause: SwitchCause::PrimaryReset,
                    }
                } else {
                    Decision::Switch {
                        from,
                        to,
                        cause: SwitchCause::EarlierAvailable,
                    }
                };
                return (decision, None);
            }
        }
        if !leaving {
            return (idle_fresh(IdleReason::OnReserve), None);
        }
        if in_cooldown && !hard {
            return (Decision::idle(IdleReason::Cooldown), None);
        }
        let cause = if relogin {
            SwitchCause::ActiveNeedsRelogin
        } else {
            SwitchCause::ReserveAtLimit
        };
        match self.pick_target(k) {
            Some(p) if p.number == primary => (
                Decision::ReturnToPrimary {
                    from,
                    to: primary,
                    cause: SwitchCause::ReservesAtLimit,
                },
                Some(p),
            ),
            Some(p) => (
                Decision::Switch {
                    from,
                    to: p.number,
                    cause,
                },
                Some(p),
            ),
            None => (Decision::suspended(SuspendReason::NoViableTarget), None),
        }
    }

    /// Dove andare lasciando l'utenza in posizione `k`.
    ///
    /// Criterio «ordine»: la prima della lista libera (dopo l'attiva: sotto soglia; prima
    /// dell'attiva: pronta al ritorno), poi la prima precedente sotto soglia. Altri criteri:
    /// il criterio tra le riserve sotto soglia, poi la principale sotto soglia.
    fn pick_target(&self, k: usize) -> Option<Pick> {
        let at = |j: usize| self.account(self.order[j]).filter(|a| !a.is_disabled());
        if self.flow() {
            let first = (0..self.order.len()).filter(|&j| j != k).find(|&j| {
                at(j).is_some_and(|a| {
                    if j > k {
                        self.eligible(a, NO_MARGIN)
                    } else {
                        self.return_ready(a)
                    }
                })
            });
            if let Some(j) = first {
                return Some(Pick {
                    number: self.order[j],
                    fallback: false,
                    why: explain::order_why(j),
                });
            }
        } else {
            let candidates: Vec<(usize, &Account)> = (1..self.order.len())
                .filter(|&j| j != k)
                .filter_map(|j| at(j).map(|a| (j, a)))
                .filter(|(_, a)| self.eligible(a, NO_MARGIN))
                .collect();
            if let Some((number, why)) = criteria::choose(self, &candidates) {
                return Some(Pick {
                    number,
                    fallback: false,
                    why,
                });
            }
        }
        let back = (0..k)
            .find(|&j| at(j).is_some_and(|a| self.eligible(a, NO_MARGIN)))
            .filter(|&j| self.flow() || j == 0)?;
        Some(Pick {
            number: self.order[back],
            fallback: true,
            why: explain::FALLBACK_WHY.to_owned(),
        })
    }

    /// Candidato attuale: il bersaglio se l'attiva superasse la soglia adesso.
    fn candidate(&self) -> Option<Candidate> {
        let active = self.list.active_account_number?;
        let k = self.order.iter().position(|n| *n == active)?;
        self.pick_target(k).map(|p| Candidate {
            number: p.number,
            criterion: self.rules.reserve_criterion.clone(),
            fallback: p.fallback,
            why: p.why,
        })
    }
}

/// Nomi dei modelli (`scoped[]`) riportati dalle utenze, senza spazi ai lati.
fn scoped_names(list: &ListResponse) -> Vec<&str> {
    list.accounts
        .iter()
        .filter_map(|a| a.usage.as_ref()?.scoped.as_ref())
        .flatten()
        .filter_map(|w| w.name.as_deref().map(str::trim))
        .filter(|n| !n.is_empty())
        .collect()
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

/// Consumo da usare per i limiti: quello attuale o l'ultimo buono.
fn limit_usage(a: &Account) -> Option<&Usage> {
    a.usage.as_ref().or(a.last_good_usage.as_ref())
}

/// `true` se l'account è al limite. Vale anche con dati vecchi o con `lastGoodUsage`:
/// prima del reset il consumo può solo crescere. `hard` usa il 100% invece delle soglie.
fn at_limit(a: &Account, rules: &EngineRules, now: i64, hard: bool) -> bool {
    let Some(u) = limit_usage(a) else {
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

/// Finestre dell'account oltre le soglie del motore.
fn limiting_windows(a: &Account, rules: &EngineRules, now: i64) -> Vec<LimitWindow> {
    let Some(u) = limit_usage(a) else {
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

/// Reset più lontano tra le finestre al limite (quando l'utenza torna utilizzabile).
fn limiting_reset(a: &Account, rules: &EngineRules, now: i64) -> Option<i64> {
    let u = limit_usage(a)?;
    [
        (u.five_hour.as_ref(), rules.five_hour_threshold),
        (u.seven_day.as_ref(), rules.seven_day_limit()),
    ]
    .into_iter()
    .filter_map(|(w, t)| w.filter(|w| effective_pct(Some(w), now) >= t))
    .filter_map(|w| w.resets_at.as_deref().and_then(parse_iso))
    .max()
}

#[cfg(test)]
mod flow_tests;
#[cfg(test)]
pub(crate) mod tests;
