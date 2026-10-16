//! Mappe (profili con nome): ognuna è un [`EngineRules`] con id e nome. La config tiene in
//! `engine` le regole della mappa attiva (il motore le rilegge a ogni giro, quindi il cambio
//! di mappa vale senza riavvio) e in `engineProfiles` tutte le mappe. La scelta della mappa è
//! solo manuale.
//!
//! Motore acceso, osservazione e conflitti ignorati sono di tutte le mappe: cambiare mappa
//! non accende né spegne il motore.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{EngineRules, ReserveCriterion};

/// Mappe al massimo, e lunghezza massima di un nome.
pub const MAX_PROFILES: usize = 32;
pub const MAX_PROFILE_NAME_LEN: usize = 60;
/// Id della mappa creata dalle regole esistenti alla prima lettura.
pub const PERSONAL_ID: &str = "personal";

/// Mappe predefinite, ripristinabili.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum PresetKind {
    /// Principale 6, poi 1 e 3; settimanale più bassa, 5h 95, 7d 98, ritorno dopo il reset.
    /// Esempio: i numeri che non esistono in cswap si saltano.
    CswapGuard,
    /// Reset settimanale più vicino, 5h 90.
    Eco,
    /// 5h più libera, 5h 98.
    Sport,
    /// Ritmo sicuro.
    Safe,
}

impl PresetKind {
    pub const ALL: [Self; 4] = [Self::CswapGuard, Self::Eco, Self::Sport, Self::Safe];

    fn id(self) -> &'static str {
        match self {
            Self::CswapGuard => "cswap-guard",
            Self::Eco => "eco",
            Self::Sport => "sport",
            Self::Safe => "safe",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::CswapGuard => "Come cswap-guard",
            Self::Eco => "Eco",
            Self::Sport => "Sport",
            Self::Safe => "Sicura",
        }
    }

    /// Regole del preset. Fuori da «Come cswap-guard» lista ed esclusi restano quelli di
    /// `base`; le parti comuni a tutte le mappe vengono sempre da `base`.
    pub fn rules(self, base: &EngineRules) -> EngineRules {
        let mut r = EngineRules {
            enabled: base.enabled,
            dry_run: base.dry_run,
            ignored_conflicts: base.ignored_conflicts.clone(),
            order: Some(base.configured_order()),
            primary: None,
            reserves: Vec::new(),
            excluded: base.excluded.clone(),
            ..EngineRules::default()
        };
        match self {
            Self::CswapGuard => {
                r.order = Some(vec![6, 1, 3]);
                r.excluded = Vec::new();
            }
            Self::Eco => {
                r.reserve_criterion = ReserveCriterion::ResetSoonest;
                r.five_hour_threshold = 90.0;
            }
            Self::Sport => {
                r.reserve_criterion = ReserveCriterion::Lowest5h;
                r.five_hour_threshold = 98.0;
            }
            Self::Safe => r.reserve_criterion = ReserveCriterion::PaceSafe,
        }
        if r.order.as_ref().is_some_and(Vec::is_empty) {
            r.order = None;
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
    /// Preset di origine: «Ripristina» torna alle sue regole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preset: Option<PresetKind>,
    #[serde(default)]
    pub rules: EngineRules,
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
    /// Nuova mappa con le regole predefinite e la lista della mappa attiva (non selezionata).
    Create {
        name: String,
    },
    Rename {
        id: String,
        name: String,
    },
    /// Copia (nome assente = «<nome> (copia)»).
    Duplicate {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        name: Option<String>,
    },
    /// Elimina; mai l'ultima. Se era attiva diventa attiva la prima rimasta.
    Delete {
        id: String,
    },
    /// Torna alle regole del preset di origine.
    Restore {
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
            name: k.name().to_owned(),
            preset: Some(*k),
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
    /// sulla copia della mappa attiva. Restituisce gli avvisi.
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
            warnings.push("mappa attiva inesistente: si usa la prima".to_owned());
            self.active_id = self.profiles[0].id.clone();
        }
        for p in &mut self.profiles {
            p.rules.sync_order();
        }
        self.store_active(engine);
        warnings
    }

    /// Copia le regole attive nella mappa attiva.
    pub fn store_active(&mut self, engine: &EngineRules) {
        let id = self.active_id.clone();
        if let Some(p) = self.profiles.iter_mut().find(|p| p.id == id) {
            p.rules = engine.clone();
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
                if next.profiles.len() >= MAX_PROFILES {
                    return Err(format!("al massimo {MAX_PROFILES} mappe"));
                }
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
                let id = next.new_id();
                next.profiles.push(RulesProfile {
                    id,
                    name,
                    preset: None,
                    rules,
                });
            }
            ProfileCommand::Rename { id, name } => {
                let i = next.index(id)?;
                let name = next.free_name(name, Some(i))?;
                next.profiles[i].name = name;
            }
            ProfileCommand::Duplicate { id, name } => {
                let i = next.index(id)?;
                if next.profiles.len() >= MAX_PROFILES {
                    return Err(format!("al massimo {MAX_PROFILES} mappe"));
                }
                let name = match name {
                    Some(n) => next.free_name(n, None)?,
                    None => next.copy_name(&next.profiles[i].name),
                };
                let mut copy = next.profiles[i].clone();
                copy.id = next.new_id();
                copy.name = name;
                next.profiles.push(copy);
            }
            ProfileCommand::Delete { id } => {
                let i = next.index(id)?;
                if next.profiles.len() == 1 {
                    return Err("l'ultima mappa non si può eliminare".to_owned());
                }
                next.profiles.remove(i);
                if next.active_id == *id {
                    next.active_id = next.profiles[0].id.clone();
                }
            }
            ProfileCommand::Restore { id } => {
                let i = next.index(id)?;
                let kind = next.profiles[i]
                    .preset
                    .ok_or_else(|| "la mappa non viene da un preset".to_owned())?;
                next.profiles[i].rules = kind.rules(&next.profiles[i].rules);
            }
            ProfileCommand::Select { id } => {
                next.index(id)?;
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

    fn copy_name(&self, base: &str) -> String {
        let base: String = base
            .trim()
            .chars()
            .take(MAX_PROFILE_NAME_LEN - 12)
            .collect();
        (1..)
            .map(|n| {
                if n == 1 {
                    format!("{base} (copia)")
                } else {
                    format!("{base} (copia {n})")
                }
            })
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

    fn guard_like() -> EngineRules {
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
        let p = EngineProfiles::seeded(&guard_like());
        let ids: Vec<&str> = p.profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["personal", "cswap-guard", "eco", "sport", "safe"]);
        assert_eq!(p.active().unwrap().name, "Personale");
        assert_eq!(p.active().unwrap().rules.order, Some(vec![6, 1, 3]));
        let guard = &p.profiles[1].rules;
        let expected = EngineRules {
            order: Some(vec![6, 1, 3]),
            ..guard_like()
        };
        assert_eq!(guard, &expected, "«Come cswap-guard» = la regola attuale");
        let eco = &p.profiles[2].rules;
        assert_eq!(eco.reserve_criterion, ReserveCriterion::ResetSoonest);
        assert_eq!(eco.five_hour_threshold, 90.0);
        assert_eq!(
            eco.configured_order(),
            vec![6, 1, 3],
            "lista della mappa personale"
        );
        assert_eq!(
            p.profiles[3].rules.reserve_criterion,
            ReserveCriterion::Lowest5h
        );
        assert_eq!(p.profiles[3].rules.five_hour_threshold, 98.0);
        assert_eq!(
            p.profiles[4].rules.reserve_criterion,
            ReserveCriterion::PaceSafe
        );
        // Senza regole: i preset diversi da cswap-guard non hanno lista.
        let empty = EngineProfiles::seeded(&EngineRules::default());
        assert_eq!(empty.profiles[2].rules.order, None);
        assert!(empty.profiles[2].rules.validate_for_save().is_empty());
    }

    #[test]
    fn crud_select_and_restore() {
        let cur = guard_like();
        let mut p = EngineProfiles::seeded(&cur);
        let r = p
            .apply(&ProfileCommand::Select { id: "eco".into() }, &cur)
            .unwrap();
        assert_eq!(p.active_id, "eco");
        assert!(r.enabled, "il cambio di mappa non spegne il motore");
        assert_eq!(r.ignored_conflicts, cur.ignored_conflicts);
        assert_eq!(r.reserve_criterion, ReserveCriterion::ResetSoonest);

        // Modifica della mappa attiva, poi ripristino del preset.
        let mut edited = r.clone();
        edited.five_hour_threshold = 70.0;
        let r = p
            .apply(&ProfileCommand::Restore { id: "eco".into() }, &edited)
            .unwrap();
        assert_eq!(r.five_hour_threshold, 90.0);
        assert!(
            p.apply(
                &ProfileCommand::Restore {
                    id: "personal".into()
                },
                &r
            )
            .is_err()
        );

        p.apply(
            &ProfileCommand::Duplicate {
                id: "eco".into(),
                name: None,
            },
            &r,
        )
        .unwrap();
        p.apply(
            &ProfileCommand::Duplicate {
                id: "eco".into(),
                name: None,
            },
            &r,
        )
        .unwrap();
        let names: Vec<&str> = p.profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(&names[5..], ["Eco (copia)", "Eco (copia 2)"]);
        assert_eq!(p.profiles[5].id, "map-1");
        assert_eq!(p.profiles[5].preset, Some(PresetKind::Eco));

        p.apply(
            &ProfileCommand::Create {
                name: " Notte ".into(),
            },
            &r,
        )
        .unwrap();
        let notte = p.profiles.last().unwrap();
        assert_eq!((notte.id.as_str(), notte.name.as_str()), ("map-3", "Notte"));
        assert_eq!(notte.rules.configured_order(), vec![6, 1, 3]);
        assert!(
            p.apply(&ProfileCommand::Create { name: "eco".into() }, &r)
                .is_err()
        );
        assert!(
            p.apply(&ProfileCommand::Create { name: "  ".into() }, &r)
                .is_err()
        );
        p.apply(
            &ProfileCommand::Rename {
                id: "map-3".into(),
                name: "Sera".into(),
            },
            &r,
        )
        .unwrap();
        assert!(
            p.apply(
                &ProfileCommand::Rename {
                    id: "nope".into(),
                    name: "X".into()
                },
                &r
            )
            .is_err()
        );

        // Eliminare l'attiva rende attiva la prima; l'ultima resta.
        let r = p
            .apply(&ProfileCommand::Delete { id: "eco".into() }, &r)
            .unwrap();
        assert_eq!(p.active_id, "personal");
        assert_eq!(r.reserve_criterion, ReserveCriterion::LowestWeekly);
        let ids: Vec<String> = p.profiles.iter().map(|p| p.id.clone()).collect();
        for id in &ids[1..] {
            p.apply(&ProfileCommand::Delete { id: id.clone() }, &r)
                .unwrap();
        }
        let before = p.clone();
        assert!(
            p.apply(
                &ProfileCommand::Delete {
                    id: "personal".into()
                },
                &r
            )
            .is_err()
        );
        assert_eq!(p, before, "errore = mappe invariate");
    }

    #[test]
    fn normalize_repairs_and_engine_wins() {
        let cur = guard_like();
        let mut p = EngineProfiles::default();
        assert!(p.normalize(&cur).is_empty());
        assert_eq!(p.profiles.len(), 5, "migrazione: Personale e preset");
        let mut changed = cur.clone();
        changed.five_hour_threshold = 80.0;
        p.active_id = "sport".into();
        p.profiles.push(p.profiles[0].clone()); // id ripetuto
        let w = p.normalize(&changed);
        assert_eq!(w, ["mappe non valide tolte"]);
        assert_eq!(p.active().unwrap().rules, changed, "engine prevale");
        p.active_id = "gone".into();
        assert_eq!(p.normalize(&changed).len(), 1);
        assert_eq!(p.active_id, "personal");
    }
}
