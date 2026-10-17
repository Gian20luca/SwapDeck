//! Mappe (profili con nome): ognuna è un [`EngineRules`] con id e nome. La config tiene in
//! `engine` le regole della mappa attiva (il motore le rilegge a ogni giro, quindi il cambio
//! di mappa vale senza riavvio) e in `engineProfiles` tutte le mappe. La scelta della mappa è
//! solo manuale.
//!
//! I preset (Principale con riserve, Eco, Sport, Sicura) ci sono sempre e sono in sola lettura:
//! criterio, soglie e pesi vengono dal codice, la lista è quella della mappa da cui si sceglie
//! il preset. Per modificarne uno lo si clona come mappa personale. Le mappe personali si
//! creano, rinominano, duplicano, modificano ed eliminano (non quella attiva).
//!
//! Motore acceso, osservazione e conflitti ignorati sono di tutte le mappe: cambiare mappa
//! non accende né spegne il motore.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{EngineRules, ReserveCriterion};

/// Mappe al massimo (preset compresi), e lunghezza massima di un nome.
pub const MAX_PROFILES: usize = 32;
pub const MAX_PROFILE_NAME_LEN: usize = 60;
/// Id della mappa creata dalle regole esistenti alla prima lettura.
pub const PERSONAL_ID: &str = "personal";
/// Nome della mappa creata dalle regole attive quando la mappa attiva non c'è più.
pub const RECOVERED_NAME: &str = "Recuperata";
/// Suffisso delle mappe personali nate da un preset modificato (migrazione) o con un nome
/// riservato ai preset.
const PERSONAL_SUFFIX: &str = "personale";

/// Preset: sempre presenti, in sola lettura.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PresetKind {
    /// Principale con riserve: la lista della mappa attiva (la prima è la principale),
    /// settimanale più bassa, 5h 95, 7d 98, ritorno dopo il reset con la 5h sotto il 50%
    /// (isteresi 45). Prende il posto del vecchio preset `cswapGuard` (letto come questo).
    #[serde(alias = "cswapGuard")]
    PriorityReserves,
    /// Reset settimanale più vicino, 5h 90.
    Eco,
    /// 5h più libera, 5h 98.
    Sport,
    /// Ritmo sicuro.
    Safe,
    /// Preset non noto (scritto da una versione più recente): in sola lettura con i valori del
    /// file, che tornano nel file al salvataggio.
    #[serde(untagged)]
    #[ts(skip)]
    Unknown(String),
}

/// Nome predefinito del vecchio preset: non conta come rinomina.
const LEGACY_PRESET_NAME: &str = "Come cswap-guard";

impl PresetKind {
    pub const ALL: [Self; 4] = [Self::PriorityReserves, Self::Eco, Self::Sport, Self::Safe];

    fn id(&self) -> &'static str {
        match self {
            Self::PriorityReserves => "priority-reserves",
            Self::Eco => "eco",
            Self::Sport => "sport",
            Self::Safe => "safe",
            Self::Unknown(_) => "unknown",
        }
    }

    fn known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }

    /// Nome del preset (`en` = inglese). Nel file resta quello italiano.
    pub fn name(&self, en: bool) -> &'static str {
        match (self, en) {
            (Self::PriorityReserves, false) => "Principale con riserve",
            (Self::PriorityReserves, true) => "Primary with reserves",
            (Self::Eco, _) => "Eco",
            (Self::Sport, _) => "Sport",
            (Self::Safe, false) => "Sicura",
            (Self::Safe, true) => "Safe",
            (Self::Unknown(_), _) => "?",
        }
    }

    /// Regole del preset. Lista (senza gli esclusi) ed esclusi restano quelli di `base`, e così
    /// le parti comuni a tutte le mappe. Un preset sconosciuto lascia `base` com'è.
    pub fn rules(&self, base: &EngineRules) -> EngineRules {
        let mut r = EngineRules {
            enabled: base.enabled,
            dry_run: base.dry_run,
            ignored_conflicts: base.ignored_conflicts.clone(),
            order: Some(
                base.configured_order()
                    .into_iter()
                    .filter(|n| !base.excluded.contains(n))
                    .collect(),
            ),
            primary: None,
            reserves: Vec::new(),
            excluded: base.excluded.clone(),
            ..EngineRules::default()
        };
        match self {
            Self::PriorityReserves => r.return_hysteresis = 45.0,
            Self::Eco => {
                r.reserve_criterion = ReserveCriterion::ResetSoonest;
                r.five_hour_threshold = 90.0;
            }
            Self::Sport => {
                r.reserve_criterion = ReserveCriterion::Lowest5h;
                r.five_hour_threshold = 98.0;
            }
            Self::Safe => r.reserve_criterion = ReserveCriterion::PaceSafe,
            Self::Unknown(_) => r = base.clone(),
        }
        r.sync_order();
        r
    }
}

/// Una mappa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RulesProfile {
    pub id: String,
    pub name: String,
    /// Presente = preset in sola lettura; assente = mappa personale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preset: Option<PresetKind>,
    #[serde(default)]
    pub rules: EngineRules,
}

impl RulesProfile {
    /// Regole che la mappa avrebbe se diventasse attiva partendo da `current`: per un preset
    /// noto i valori del codice sulla lista di `current`, altrimenti le sue regole.
    fn rules_from(&self, current: &EngineRules) -> EngineRules {
        match &self.preset {
            Some(kind) if kind.known() => kind.rules(current),
            _ => self.rules.clone(),
        }
    }
}

/// Tutte le mappe e quella attiva (`config.json`, sezione `engineProfiles`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineProfiles {
    pub active_id: String,
    pub profiles: Vec<RulesProfile>,
}

/// Operazione sulle mappe (comandi dell'app, della CLI e di VS Code).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum ProfileCommand {
    /// Nuova mappa personale con le regole predefinite e la lista della mappa attiva (non
    /// selezionata).
    Create {
        name: String,
    },
    /// Solo mappe personali.
    Rename {
        id: String,
        name: String,
    },
    /// Copia personale, non selezionata (nome assente = «<nome> (copia)»).
    Duplicate {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        name: Option<String>,
    },
    /// Copia personale con il nome dato, che diventa la mappa attiva: per un preset noto i suoi
    /// valori sulla lista della mappa attiva, come se lo si scegliesse.
    #[serde(rename = "clone")]
    CloneAs {
        id: String,
        name: String,
    },
    /// Solo mappe personali e non quella attiva.
    Delete {
        id: String,
    },
    Select {
        id: String,
    },
}

/// Parti delle regole comuni a tutte le mappe: si prendono da `from`.
fn carry_globals(rules: &mut EngineRules, from: &EngineRules) {
    rules.enabled = from.enabled;
    rules.dry_run = from.dry_run;
    rules.ignored_conflicts.clone_from(&from.ignored_conflicts);
}

/// Stesse impostazioni di mappa, a meno delle parti comuni e dei valori che il criterio non
/// usa (pesi e modello). Con `list` conta anche la lista con gli esclusi.
fn same_settings(a: &EngineRules, b: &EngineRules, list: bool) -> bool {
    let plain = |r: &EngineRules| {
        let mut r = r.clone();
        carry_globals(&mut r, &EngineRules::default());
        r.sync_order();
        if !list {
            r.order = None;
            r.primary = None;
            r.reserves.clear();
            r.excluded.clear();
        }
        if r.reserve_criterion != ReserveCriterion::BalancedScore
            || r.score_weights == Some(super::ScoreWeights::default())
        {
            r.score_weights = None;
        }
        if r.reserve_criterion != ReserveCriterion::ModelHeadroom {
            r.headroom_model = None;
        }
        r
    };
    plain(a) == plain(b)
}

fn read_only(name: &str) -> String {
    format!("«{name}» è un preset in sola lettura: clonalo per modificarlo")
}

impl EngineProfiles {
    /// Mappe iniziali: «Personale» (le regole attuali, attiva) e i preset.
    pub fn seeded(current: &EngineRules) -> Self {
        let mut personal = current.clone();
        personal.sync_order();
        let mut profiles = vec![RulesProfile {
            id: PERSONAL_ID.to_owned(),
            name: "Personale".to_owned(),
            preset: None,
            rules: personal,
        }];
        profiles.extend(PresetKind::ALL.iter().map(|k| RulesProfile {
            id: k.id().to_owned(),
            name: k.name(false).to_owned(),
            preset: Some(k.clone()),
            rules: k.rules(current),
        }));
        Self {
            active_id: PERSONAL_ID.to_owned(),
            profiles,
        }
    }

    pub fn active(&self) -> Option<&RulesProfile> {
        self.profiles.iter().find(|p| p.id == self.active_id)
    }

    fn index(&self, id: &str) -> Result<usize, String> {
        self.profiles
            .iter()
            .position(|p| p.id == id)
            .ok_or_else(|| "mappa inesistente".to_owned())
    }

    /// Mappe lette dal file riportate in forma valida; `engine` (le regole attive) prevale
    /// sulla copia della mappa attiva. Migra i preset della v0.1.0 (vedi
    /// [`Self::migrate_presets`]). Idempotente. Restituisce gli avvisi.
    pub fn normalize(&mut self, engine: &EngineRules) -> Vec<String> {
        let mut warnings = Vec::new();
        let before = self.profiles.len();
        let mut seen = std::collections::HashSet::new();
        self.profiles
            .retain(|p| valid_id(&p.id) && seen.insert(p.id.clone()) && name_ok(&p.name).is_ok());
        self.profiles.truncate(MAX_PROFILES);
        if self.profiles.len() != before {
            warnings.push("mappe non valide tolte".to_owned());
        }
        if self.profiles.is_empty() {
            *self = Self::seeded(engine);
            return warnings;
        }
        if self.active().is_none() {
            // Le regole attive (`engine`) non devono finire sopra un'altra mappa.
            let name = (1..)
                .map(|n| match n {
                    1 => RECOVERED_NAME.to_owned(),
                    n => format!("{RECOVERED_NAME} {n}"),
                })
                .find(|c| self.free_name(c, None).is_ok())
                .unwrap_or_default();
            warnings.push(format!(
                "mappa attiva inesistente: creata «{name}» dalle regole attive"
            ));
            let id = self.new_id();
            self.profiles.push(RulesProfile {
                id: id.clone(),
                name,
                preset: None,
                rules: engine.clone(),
            });
            self.active_id = id;
        }
        for p in &mut self.profiles {
            p.rules.sync_order();
        }
        self.store_active(engine);
        warnings.extend(self.migrate_presets(engine));
        warnings.extend(self.fit());
        warnings
    }

    /// Preset in sola lettura con i valori del codice:
    /// - un preset salvato con valori diversi dall'originale (o rinominato) diventa una mappa
    ///   personale con le sue regole, «<Nome> (personale)» o il nome che gli era stato dato, e
    ///   la mappa attiva la segue; il preset torna all'originale con la sua lista;
    /// - le copie di un preset (stesso tipo, «Duplica» della v0.1.0) diventano personali;
    /// - i preset mancanti si aggiungono con la lista delle regole attive;
    /// - una mappa personale con il nome di un preset prende il suffisso «(personale)».
    fn migrate_presets(&mut self, engine: &EngineRules) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut seen: Vec<PresetKind> = Vec::new();
        let mut copies = Vec::new();
        for i in 0..self.profiles.len() {
            let Some(kind) = self.profiles[i].preset.clone().filter(PresetKind::known) else {
                continue;
            };
            if seen.contains(&kind) {
                self.profiles[i].preset = None;
                continue;
            }
            seen.push(kind.clone());
            let code_name = kind.name(false);
            let p = &self.profiles[i];
            let original = kind.rules(&p.rules);
            let renamed = p.name.trim() != code_name && p.name.trim() != LEGACY_PRESET_NAME;
            if renamed || !same_settings(&p.rules, &original, false) {
                let base = if renamed {
                    p.name.trim().to_owned()
                } else {
                    code_name.to_owned()
                };
                copies.push((i, base, p.rules.clone()));
            }
            let p = &mut self.profiles[i];
            p.name = code_name.to_owned();
            p.rules = original;
        }
        for (i, base, rules) in copies {
            let id = self.new_id();
            let name = self.personal_name(&base, true);
            warnings.push(format!(
                "il preset «{}» era stato modificato: ora è la mappa personale «{name}»",
                self.profiles[i].name
            ));
            if self.active_id == self.profiles[i].id {
                self.active_id = id.clone();
            }
            self.profiles.push(RulesProfile {
                id,
                name,
                preset: None,
                rules,
            });
        }
        for kind in PresetKind::ALL {
            if seen.contains(&kind) {
                continue;
            }
            let id = if self.profiles.iter().any(|p| p.id == kind.id()) {
                self.new_id()
            } else {
                kind.id().to_owned()
            };
            self.profiles.push(RulesProfile {
                id,
                name: kind.name(false).to_owned(),
                preset: Some(kind.clone()),
                rules: kind.rules(engine),
            });
        }
        // Nomi dei preset riservati; nomi ripetuti tra mappe personali resi unici.
        for i in 0..self.profiles.len() {
            if self.profiles[i].preset.is_some() {
                continue;
            }
            let name = self.profiles[i].name.clone();
            let clash = self.profiles.iter().enumerate().any(|(j, p)| {
                j != i
                    && p.name.trim().eq_ignore_ascii_case(name.trim())
                    && (p.preset.is_some() || j < i)
            });
            if clash {
                let fresh = self.personal_name(&name, false);
                warnings.push(format!(
                    "nome «{name}» già usato: la mappa personale si chiama «{fresh}»"
                ));
                self.profiles[i].name = fresh;
            }
        }
        warnings
    }

    /// Oltre [`MAX_PROFILES`] si tolgono le ultime mappe personali non attive.
    fn fit(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        while self.profiles.len() > MAX_PROFILES {
            let Some(i) = self
                .profiles
                .iter()
                .rposition(|p| p.preset.is_none() && p.id != self.active_id)
            else {
                break;
            };
            let dropped = self.profiles.remove(i);
            warnings.push(format!("troppe mappe: tolta «{}»", dropped.name));
        }
        warnings
    }

    /// Copia le regole attive nella mappa attiva.
    pub fn store_active(&mut self, engine: &EngineRules) {
        let id = self.active_id.clone();
        if let Some(p) = self.profiles.iter_mut().find(|p| p.id == id) {
            p.rules = engine.clone();
        }
    }

    /// Controlla che `rules` (nuove regole attive) non modifichi un preset attivo: per un
    /// preset cambiano solo le parti comuni (motore acceso, osservazione, conflitti ignorati).
    /// Le mappe devono essere già normalizzate.
    pub fn check_editable(&self, rules: &EngineRules) -> Result<(), String> {
        match self.active() {
            Some(p) if p.preset.is_some() && !same_settings(&p.rules, rules, true) => {
                Err(read_only(&p.name))
            }
            _ => Ok(()),
        }
    }

    /// Applica `cmd` partendo dalle regole attive `current`; restituisce le nuove regole attive
    /// (con le parti comuni di `current`). Errore = mappe invariate.
    pub fn apply(
        &mut self,
        cmd: &ProfileCommand,
        current: &EngineRules,
    ) -> Result<EngineRules, String> {
        let mut next = self.clone();
        next.store_active(current);
        match cmd {
            ProfileCommand::Create { name } => {
                let name = next.free_name(name, None)?;
                next.room()?;
                let mut rules = EngineRules {
                    order: Some(current.configured_order()),
                    primary: None,
                    reserves: Vec::new(),
                    excluded: current.excluded.clone(),
                    ..EngineRules::default()
                };
                if rules.order.as_ref().is_some_and(Vec::is_empty) {
                    rules.order = None;
                }
                rules.sync_order();
                carry_globals(&mut rules, current);
                next.push_personal(name, rules);
            }
            ProfileCommand::Rename { id, name } => {
                let i = next.personal(id)?;
                let name = next.free_name(name, Some(i))?;
                next.profiles[i].name = name;
            }
            ProfileCommand::Duplicate { id, name } => {
                let i = next.index(id)?;
                next.room()?;
                let name = match name {
                    Some(n) => next.free_name(n, None)?,
                    None => next.copy_name(&next.profiles[i].name),
                };
                let rules = next.profiles[i].rules.clone();
                next.push_personal(name, rules);
            }
            ProfileCommand::CloneAs { id, name } => {
                let i = next.index(id)?;
                let name = next.free_name(name, None)?;
                next.room()?;
                let mut rules = next.profiles[i].rules_from(current);
                carry_globals(&mut rules, current);
                next.active_id = next.push_personal(name, rules);
            }
            ProfileCommand::Delete { id } => {
                let i = next.personal(id)?;
                if next.active_id == *id {
                    return Err(
                        "la mappa attiva non si elimina: scegli prima un'altra mappa".to_owned(),
                    );
                }
                next.profiles.remove(i);
            }
            ProfileCommand::Select { id } => {
                let i = next.index(id)?;
                next.profiles[i].rules = next.profiles[i].rules_from(current);
                next.active_id = id.clone();
            }
        }
        let i = next.index(&next.active_id.clone())?;
        let mut rules = next.profiles[i].rules.clone();
        carry_globals(&mut rules, current);
        rules.sync_order();
        next.profiles[i].rules = rules.clone();
        *self = next;
        Ok(rules)
    }

    /// Indice di una mappa personale; un preset dà l'errore di sola lettura.
    fn personal(&self, id: &str) -> Result<usize, String> {
        let i = self.index(id)?;
        match &self.profiles[i].preset {
            Some(_) => Err(read_only(&self.profiles[i].name)),
            None => Ok(i),
        }
    }

    fn room(&self) -> Result<(), String> {
        if self.profiles.len() >= MAX_PROFILES {
            return Err(format!("al massimo {MAX_PROFILES} mappe, preset compresi"));
        }
        Ok(())
    }

    /// Aggiunge una mappa personale; restituisce l'id.
    fn push_personal(&mut self, name: String, rules: EngineRules) -> String {
        let id = self.new_id();
        self.profiles.push(RulesProfile {
            id: id.clone(),
            name,
            preset: None,
            rules,
        });
        id
    }

    fn new_id(&self) -> String {
        (1..)
            .map(|n| format!("map-{n}"))
            .find(|id| self.profiles.iter().all(|p| p.id != *id))
            .unwrap_or_default()
    }

    /// Nome valido e non usato da altre mappe (senza distinzione di maiuscole).
    fn free_name(&self, name: &str, except: Option<usize>) -> Result<String, String> {
        let name = name_ok(name)?;
        let taken = self
            .profiles
            .iter()
            .enumerate()
            .any(|(i, p)| Some(i) != except && p.name.trim().eq_ignore_ascii_case(&name));
        if taken {
            return Err("esiste già una mappa con questo nome".to_owned());
        }
        Ok(name)
    }

    /// Primo nome libero tra `base` (se `plain`), «<base> (personale)», «<base> (personale 2)»…
    fn personal_name(&self, base: &str, plain: bool) -> String {
        self.suffixed(base, PERSONAL_SUFFIX, plain)
    }

    fn copy_name(&self, base: &str) -> String {
        self.suffixed(base, "copia", false)
    }

    fn suffixed(&self, base: &str, suffix: &str, plain: bool) -> String {
        let full = base.trim();
        let base: String = full
            .chars()
            .take(MAX_PROFILE_NAME_LEN - suffix.chars().count() - 8)
            .collect();
        (0..)
            .map(|n| match n {
                0 => full.to_owned(),
                1 => format!("{base} ({suffix})"),
                n => format!("{base} ({suffix} {n})"),
            })
            .skip(usize::from(!plain))
            .find(|c| self.free_name(c, None).is_ok())
            .unwrap_or_default()
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn name_ok(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_PROFILE_NAME_LEN
        || name.chars().any(char::is_control)
    {
        return Err(format!(
            "nome della mappa vuoto, oltre {MAX_PROFILE_NAME_LEN} caratteri o con caratteri di controllo"
        ));
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_like() -> EngineRules {
        EngineRules {
            enabled: true,
            primary: Some(6),
            reserves: vec![1, 3],
            ignored_conflicts: vec!["task:cswap-guard".to_owned()],
            ..EngineRules::default()
        }
    }

    #[test]
    fn seeding_and_presets() {
        let p = EngineProfiles::seeded(&user_like());
        let ids: Vec<&str> = p.profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            ["personal", "priority-reserves", "eco", "sport", "safe"]
        );
        assert_eq!(p.active().unwrap().name, "Personale");
        assert_eq!(p.active().unwrap().rules.order, Some(vec![6, 1, 3]));
        let priority = &p.profiles[1].rules;
        let expected = EngineRules {
            order: Some(vec![6, 1, 3]),
            return_hysteresis: 45.0,
            ..user_like()
        };
        assert_eq!(
            priority, &expected,
            "«Principale con riserve» = la lista attuale"
        );
        assert_eq!(p.profiles[1].name, "Principale con riserve");
        let eco = &p.profiles[2].rules;
        assert_eq!(eco.reserve_criterion, ReserveCriterion::ResetSoonest);
        assert_eq!(eco.five_hour_threshold, 90.0);
        assert_eq!(eco.configured_order(), vec![6, 1, 3]);
        assert_eq!(
            p.profiles[3].rules.reserve_criterion,
            ReserveCriterion::Lowest5h
        );
        assert_eq!(p.profiles[3].rules.five_hour_threshold, 98.0);
        assert_eq!(
            p.profiles[4].rules.reserve_criterion,
            ReserveCriterion::PaceSafe
        );
        let empty = EngineProfiles::seeded(&EngineRules::default());
        assert_eq!(empty.profiles[2].rules.order, None);
        assert!(empty.profiles[2].rules.validate_for_save().is_empty());
        // Già normalizzate: nessun avviso, niente cambia.
        let mut again = p.clone();
        let mut synced = user_like();
        synced.sync_order();
        assert!(again.normalize(&synced).is_empty());
        assert_eq!(again, p);
    }

    #[test]
    fn presets_are_read_only_and_clone_into_personal_maps() {
        let cur = user_like();
        let mut p = EngineProfiles::seeded(&cur);
        let r = p
            .apply(&ProfileCommand::Select { id: "eco".into() }, &cur)
            .unwrap();
        assert_eq!(p.active_id, "eco");
        assert!(r.enabled, "il cambio di mappa non spegne il motore");
        assert_eq!(r.ignored_conflicts, cur.ignored_conflicts);
        assert_eq!(r.reserve_criterion, ReserveCriterion::ResetSoonest);

        // Preset: niente rinomina né eliminazione, regole modificabili solo nelle parti comuni.
        let before = p.clone();
        for cmd in [
            ProfileCommand::Rename {
                id: "eco".into(),
                name: "Mia".into(),
            },
            ProfileCommand::Delete { id: "sport".into() },
        ] {
            let err = p.apply(&cmd, &r).unwrap_err();
            assert!(
                err.contains("sola lettura") && err.contains("clonalo"),
                "{err}"
            );
        }
        assert_eq!(p, before, "errore = mappe invariate");
        let mut edited = r.clone();
        edited.five_hour_threshold = 70.0;
        assert!(p.check_editable(&edited).unwrap_err().contains("«Eco»"));
        let mut listed = r.clone();
        listed.order = Some(vec![1, 6]);
        assert!(
            p.check_editable(&listed).is_err(),
            "anche la lista è del preset"
        );
        let mut globals = r.clone();
        globals.enabled = false;
        globals.dry_run = true;
        globals.ignored_conflicts.clear();
        p.check_editable(&globals).unwrap();

        // Clona: mappa personale con gli stessi valori, attiva.
        let clone = ProfileCommand::CloneAs {
            id: "eco".into(),
            name: " Mia eco ".into(),
        };
        let r2 = p.apply(&clone, &r).unwrap();
        assert_eq!(r2, r, "stessi valori");
        let mine = p.active().unwrap();
        assert_eq!((mine.id.as_str(), mine.name.as_str()), ("map-1", "Mia eco"));
        assert_eq!(mine.preset, None);
        p.check_editable(&edited).unwrap();
        for bad in ["", "  ", "eco", "MIA ECO"] {
            let cmd = ProfileCommand::CloneAs {
                id: "eco".into(),
                name: bad.into(),
            };
            assert!(p.apply(&cmd, &r2).is_err(), "nome «{bad}»");
        }

        // Duplica (anche un preset): copia personale, non selezionata.
        let dup = |name: Option<&str>| ProfileCommand::Duplicate {
            id: "sport".into(),
            name: name.map(str::to_owned),
        };
        p.apply(&dup(None), &r2).unwrap();
        p.apply(&dup(None), &r2).unwrap();
        p.apply(&dup(Some("Corsa")), &r2).unwrap();
        let names: Vec<&str> = p.profiles[6..].iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Sport (copia)", "Sport (copia 2)", "Corsa"]);
        assert!(p.profiles[6..].iter().all(|m| m.preset.is_none()));
        assert_eq!(p.active_id, "map-1");
        let rename = ProfileCommand::Rename {
            id: "map-2".into(),
            name: "Veloce".into(),
        };
        p.apply(&rename, &r2).unwrap();

        // La mappa attiva non si elimina; le personali sì, tutte: i preset restano.
        let err = p
            .apply(&ProfileCommand::Delete { id: "map-1".into() }, &r2)
            .unwrap_err();
        assert!(err.contains("scegli prima un'altra mappa"), "{err}");
        let r3 = p
            .apply(&ProfileCommand::Select { id: "safe".into() }, &r2)
            .unwrap();
        let personal: Vec<String> = p
            .profiles
            .iter()
            .filter(|m| m.preset.is_none())
            .map(|m| m.id.clone())
            .collect();
        for id in personal {
            p.apply(&ProfileCommand::Delete { id }, &r3).unwrap();
        }
        assert_eq!(p.profiles.len(), PresetKind::ALL.len());
        assert_eq!(p.active_id, "safe");
    }

    #[test]
    fn a_preset_takes_the_list_of_the_map_it_is_chosen_from() {
        let cur = user_like();
        let mut p = EngineProfiles::seeded(&cur);
        let mut mine = cur.clone();
        mine.order = Some(vec![2, 5, 7]);
        mine.excluded = vec![7];
        mine.sync_order();
        p.store_active(&mine);
        let r = p
            .apply(&ProfileCommand::Select { id: "sport".into() }, &mine)
            .unwrap();
        assert_eq!(r.configured_order(), vec![2, 5]);
        assert_eq!(r.excluded, vec![7]);
        assert_eq!(r.five_hour_threshold, 98.0);
        // Il clone di un preset non attivo ha i suoi valori sulla lista attiva.
        let clone = ProfileCommand::CloneAs {
            id: "eco".into(),
            name: "E".into(),
        };
        let r = p.apply(&clone, &r).unwrap();
        assert_eq!(
            (r.configured_order(), r.reserve_criterion.clone()),
            (vec![2, 5], ReserveCriterion::ResetSoonest)
        );
        // Tornando a «Personale» la sua lista resta la sua.
        let r = p
            .apply(
                &ProfileCommand::Select {
                    id: "personal".into(),
                },
                &r,
            )
            .unwrap();
        assert_eq!(r.configured_order(), vec![2, 5, 7]);
    }

    #[test]
    fn normalize_repairs_and_engine_wins() {
        let cur = user_like();
        let mut p = EngineProfiles::default();
        assert!(p.normalize(&cur).is_empty());
        assert_eq!(p.profiles.len(), 5, "migrazione: Personale e preset");
        let mut changed = cur.clone();
        changed.five_hour_threshold = 80.0;
        changed.sync_order();
        p.profiles.push(p.profiles[0].clone()); // id ripetuto
        let w = p.normalize(&changed);
        assert_eq!(w, ["mappe non valide tolte"]);
        assert_eq!(p.active().unwrap().rules, changed, "engine prevale");
        // Mappa attiva sparita: le regole attive vanno in «Recuperata», la prima resta com'è.
        let first = p.profiles[0].clone();
        p.active_id = "gone".into();
        assert_eq!(
            p.normalize(&changed),
            ["mappa attiva inesistente: creata «Recuperata» dalle regole attive"]
        );
        assert_eq!(p.profiles[0], first);
        let rec = p.active().unwrap();
        assert_eq!((rec.name.as_str(), &rec.rules), (RECOVERED_NAME, &changed));
        p.active_id = "gone-again".into();
        p.normalize(&changed);
        assert_eq!(p.active().unwrap().name, "Recuperata 2");
        // Al massimo delle mappe si toglie l'ultima personale non attiva per fare posto.
        while p.profiles.len() < MAX_PROFILES {
            p.profiles.push(RulesProfile {
                id: format!("x-{}", p.profiles.len()),
                name: format!("X {}", p.profiles.len()),
                preset: None,
                rules: cur.clone(),
            });
        }
        p.active_id = "gone".into();
        let w = p.normalize(&changed);
        assert_eq!(w.len(), 2, "{w:?}");
        assert_eq!(p.profiles.len(), MAX_PROFILES);
        assert_eq!(p.active().unwrap().rules, changed);
        assert_eq!(p.profiles.iter().filter(|m| m.preset.is_some()).count(), 4);
        let err = p
            .apply(&ProfileCommand::Create { name: "Y".into() }, &changed)
            .unwrap_err();
        assert!(err.contains("al massimo"), "{err}");
    }

    /// Config della v0.1.0: preset modificati, rinominati, duplicati o mancanti.
    #[test]
    fn v010_presets_migrate_on_read_idempotently() {
        let text = r#"{"activeId":"eco","profiles":[
            {"id":"personal","name":"Personale","rules":{"order":[2,5]}},
            {"id":"eco","name":"Eco","preset":"eco",
             "rules":{"order":[6,1],"reserveCriterion":"reset-soonest","fiveHourThreshold":70}},
            {"id":"sport","name":"Corsa","preset":"sport",
             "rules":{"order":[6],"reserveCriterion":"lowest5h","fiveHourThreshold":98}},
            {"id":"safe","name":"Sicura","preset":"safe",
             "rules":{"order":[3,4],"reserveCriterion":"pace-safe",
                      "scoreWeights":{"fiveHour":1,"sevenDay":0,"reset":0,"pace":0}}},
            {"id":"map-1","name":"Eco (copia)","preset":"eco","rules":{"order":[9]}},
            {"id":"map-2","name":"sport","rules":{"order":[1]}}]}"#;
        let mut p: EngineProfiles = serde_json::from_str(text).unwrap();
        let mut engine = p.profiles[1].rules.clone();
        engine.sync_order();
        let w = p.normalize(&engine);
        assert_eq!(w.len(), 3, "{w:?}");

        let by_name =
            |p: &EngineProfiles, n: &str| p.profiles.iter().find(|m| m.name == n).cloned().unwrap();
        // Eco modificato: le regole dell'utente in «Eco (personale)», attiva; il preset torna.
        let mine = by_name(&p, "Eco (personale)");
        assert_eq!(
            (mine.preset.clone(), mine.rules.five_hour_threshold),
            (None, 70.0)
        );
        assert_eq!(p.active_id, mine.id, "la mappa attiva segue la personale");
        assert_eq!(mine.rules, engine);
        let eco = by_name(&p, "Eco");
        assert_eq!(
            (eco.id.as_str(), eco.rules.five_hour_threshold),
            ("eco", 90.0)
        );
        assert_eq!(
            eco.rules.configured_order(),
            vec![6, 1],
            "lista del preset tenuta"
        );
        // Sport solo rinominato: «Corsa» resta come mappa personale, il preset torna «Sport»,
        // e la mappa personale «sport» prende il suffisso.
        assert_eq!(by_name(&p, "Corsa").preset, None);
        assert_eq!(by_name(&p, "Sport").preset, Some(PresetKind::Sport));
        assert_eq!(by_name(&p, "sport (personale)").id, "map-2");
        // Pesi di un criterio che non li usa: non contano come modifica.
        assert_eq!(by_name(&p, "Sicura").rules.score_weights, None);
        assert!(p.profiles.iter().all(|m| m.name != "Sicura (personale)"));
        // Copia di un preset: personale. Preset mancante: aggiunto con la lista attiva.
        assert_eq!(by_name(&p, "Eco (copia)").preset, None);
        let pr = by_name(&p, "Principale con riserve");
        assert_eq!(
            (pr.id.as_str(), pr.rules.configured_order()),
            ("priority-reserves", vec![6, 1])
        );
        assert_eq!(by_name(&p, "Personale").preset, None);
        assert_eq!(p.profiles.iter().filter(|m| m.preset.is_some()).count(), 4);

        // Idempotente, anche dopo un giro nel file.
        let once = p.clone();
        let engine = p.active().unwrap().rules.clone();
        assert!(p.normalize(&engine).is_empty());
        assert_eq!(p, once);
        let mut again: EngineProfiles =
            serde_json::from_value(serde_json::to_value(&p).unwrap()).unwrap();
        assert!(again.normalize(&engine).is_empty());
        assert_eq!(again, once);
    }

    #[test]
    fn engine_changed_outside_on_an_active_preset_becomes_personal() {
        let cur = user_like();
        let mut p = EngineProfiles::seeded(&cur);
        p.apply(&ProfileCommand::Select { id: "safe".into() }, &cur)
            .unwrap();
        let mut outside = p.active().unwrap().rules.clone();
        outside.min_switch_gap_secs = 60;
        let w = p.normalize(&outside);
        assert_eq!(w.len(), 1, "{w:?}");
        let active = p.active().unwrap();
        assert_eq!(
            (active.name.as_str(), &active.rules),
            ("Sicura (personale)", &outside)
        );
        let safe = p.profiles.iter().find(|m| m.id == "safe").unwrap();
        assert_eq!(safe.rules.min_switch_gap_secs, 300);
    }

    #[test]
    fn priority_preset_keeps_excluded() {
        let base = EngineRules {
            excluded: vec![1, 9],
            ..user_like()
        };
        let r = PresetKind::PriorityReserves.rules(&base);
        assert_eq!(r.excluded, vec![1, 9]);
        assert_eq!(r.configured_order(), vec![6, 3]);
        assert_eq!((r.primary, r.reserves.clone()), (Some(6), vec![3]));
        assert!(r.validate().is_empty());
    }

    #[test]
    fn unknown_preset_and_criterion_survive_a_round_trip() {
        let text = r#"{"activeId":"m","profiles":[{"id":"m","name":"Futura","preset":"turbo",
            "rules":{"order":[6,1],"reserveCriterion":"quantum-pick"}}]}"#;
        let mut p: EngineProfiles = serde_json::from_str(text).unwrap();
        let m = &p.profiles[0];
        assert_eq!(m.preset, Some(PresetKind::Unknown("turbo".into())));
        assert_eq!(
            m.rules.reserve_criterion,
            ReserveCriterion::Unknown("quantum-pick".into())
        );
        let engine = m.rules.clone();
        assert!(p.normalize(&engine).is_empty());
        assert_eq!(p.profiles.len(), 5, "preset noti aggiunti");
        let rename = ProfileCommand::Rename {
            id: "m".into(),
            name: "X".into(),
        };
        let err = p.apply(&rename, &engine).unwrap_err();
        assert!(err.contains("sola lettura"), "{err}");
        let mut edited = engine.clone();
        edited.five_hour_threshold = 50.0;
        assert!(p.check_editable(&edited).is_err());
        let saved = serde_json::to_value(&p).unwrap();
        assert_eq!(saved["profiles"][0]["preset"], "turbo");
        assert_eq!(
            saved["profiles"][0]["rules"]["reserveCriterion"],
            "quantum-pick"
        );
        let again: EngineProfiles = serde_json::from_value(saved).unwrap();
        assert_eq!(again, p);
        assert_eq!(
            serde_json::to_value(PresetKind::PriorityReserves).unwrap(),
            "priorityReserves"
        );
        // Il clone di un preset sconosciuto copia i suoi valori.
        let clone = ProfileCommand::CloneAs {
            id: "m".into(),
            name: "Mia".into(),
        };
        let r = p.apply(&clone, &engine).unwrap();
        assert_eq!(
            r.reserve_criterion,
            ReserveCriterion::Unknown("quantum-pick".into())
        );
    }

    #[test]
    fn old_guard_preset_becomes_priority_reserves() {
        let text = r#"{"activeId":"personal","profiles":[
            {"id":"personal","name":"Personale","rules":{"order":[2,5]}},
            {"id":"cswap-guard","name":"Come cswap-guard","preset":"cswapGuard",
             "rules":{"order":[6,1,3],"fiveHourThreshold":80}},
            {"id":"g2","name":"Mia","preset":"cswapGuard","rules":{"order":[6]}}]}"#;
        let mut p: EngineProfiles = serde_json::from_str(text).unwrap();
        let engine = p.profiles[0].rules.clone();
        let w = p.normalize(&engine);
        assert_eq!(w.len(), 1, "{w:?}");
        let old = &p.profiles[1];
        assert_eq!(old.preset, Some(PresetKind::PriorityReserves));
        assert_eq!(old.name, "Principale con riserve");
        assert_eq!(old.id, "cswap-guard", "l'id resta");
        assert_eq!(old.rules.five_hour_threshold, 95.0, "valori del codice");
        let mine = p
            .profiles
            .iter()
            .find(|m| m.name == "Principale con riserve (personale)");
        assert_eq!(
            mine.map(|m| (m.rules.configured_order(), m.rules.five_hour_threshold)),
            Some((vec![6, 1, 3], 80.0)),
            "regole dell'utente in una mappa personale"
        );
        assert_eq!(
            (p.profiles[2].name.as_str(), &p.profiles[2].preset),
            ("Mia", &None)
        );
        let saved = serde_json::to_value(&p).unwrap();
        assert_eq!(saved["profiles"][1]["preset"], "priorityReserves");
        assert_eq!(
            PresetKind::PriorityReserves.name(true),
            "Primary with reserves"
        );
    }

    #[test]
    fn clone_command_json() {
        let cmd: ProfileCommand =
            serde_json::from_str(r#"{"op":"clone","id":"eco","name":"Mia"}"#).unwrap();
        assert_eq!(
            cmd,
            ProfileCommand::CloneAs {
                id: "eco".into(),
                name: "Mia".into()
            }
        );
        let restore = r#"{"op":"restore","id":"eco"}"#;
        assert!(serde_json::from_str::<ProfileCommand>(restore).is_err());
    }
}
