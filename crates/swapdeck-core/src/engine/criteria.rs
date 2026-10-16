//! Criteri di scelta della riserva. Ogni criterio ha una chiave per candidata (`None` = dati
//! che servono mancanti: candidata esclusa) e un confronto; a parità vince la prima nella
//! lista (`min_by` tiene il primo minimo e le candidate arrivano in ordine). Se il criterio
//! non sceglie nessuna candidata si ripiega sulla settimanale più bassa e poi sull'ordine.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::explain::{duration, pct};
use super::{Account, Ctx, ReserveCriterion, UsageWindow, effective_pct, explain, reset_passed};
use crate::timeutil::parse_iso;

const WEEK_SECS: f64 = 7.0 * 86_400.0;

/// Pesi di [`ReserveCriterion::BalancedScore`]. Ogni componente vale 0–1 e il punteggio è la
/// media pesata: margine 5h e 7d rispetto alle soglie, vicinanza del reset 7d (1 = adesso,
/// 0 = tra una settimana), ritmo (`0,5 + (expectedPct - pct) / 100`). Un fattore senza dati si
/// toglie e i pesi degli altri si rinormalizzano; un peso a 0 toglie la componente.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ScoreWeights {
    pub five_hour: f64,
    pub seven_day: f64,
    pub reset: f64,
    pub pace: f64,
}

impl Default for ScoreWeights {
    /// 5h 0,4 (la finestra che fa cambiare più spesso), 7d 0,3, reset 0,15, ritmo 0,15.
    fn default() -> Self {
        Self {
            five_hour: 0.4,
            seven_day: 0.3,
            reset: 0.15,
            pace: 0.15,
        }
    }
}

impl ScoreWeights {
    /// Pesi finiti tra 0 e 100, almeno uno positivo.
    pub fn is_valid(&self) -> bool {
        let all = [self.five_hour, self.seven_day, self.reset, self.pace];
        all.iter()
            .all(|w| w.is_finite() && (0.0..=100.0).contains(w))
            && all.iter().any(|w| *w > 0.0)
    }
}

/// Prima candidata con la chiave minore secondo `cmp`; candidate senza chiave escluse.
fn best_by<'a, K>(
    cands: &[(usize, &'a Account)],
    key: impl Fn(&Account) -> Option<K>,
    cmp: impl Fn(&K, &K) -> Ordering,
) -> Option<(&'a Account, K)> {
    cands
        .iter()
        .filter_map(|(_, a)| key(a).map(|k| (*a, k)))
        .min_by(|(_, x), (_, y)| cmp(x, y))
}

/// Finestra 7d con reset futuro noto (i campi di ritmo valgono solo prima del reset).
fn live_week(a: &Account, now: i64) -> Option<&UsageWindow> {
    a.usage
        .as_ref()?
        .seven_day
        .as_ref()
        .filter(|w| !reset_passed(w, now))
}

fn reset_at(w: &UsageWindow, now: i64) -> Option<i64> {
    w.resets_at
        .as_deref()
        .and_then(parse_iso)
        .filter(|r| *r > now)
}

fn win5(a: &Account) -> Option<&UsageWindow> {
    a.usage.as_ref().and_then(|u| u.five_hour.as_ref())
}

fn win7(a: &Account) -> Option<&UsageWindow> {
    a.usage.as_ref().and_then(|u| u.seven_day.as_ref())
}

/// Riserva scelta tra le candidate (già sotto soglia, in ordine di lista) e il suo motivo.
/// Se il criterio non ne sceglie nessuna (dati mancanti) ripiega sulla settimanale più bassa
/// tra quelle con la 7d nota e poi sulla prima della lista, e lo dice nel motivo.
pub(super) fn choose(ctx: &Ctx<'_>, cands: &[(usize, &Account)]) -> Option<(u32, String)> {
    let &(first_pos, first) = cands.first()?;
    if let Some(pick) = by_criterion(ctx, cands) {
        return Some(pick);
    }
    let missing = missing_data(ctx);
    let now = ctx.now;
    let key =
        |a: &Account| win7(a).map(|w| (effective_pct(Some(w), now), effective_pct(win5(a), now)));
    let asc2 = |x: &(f64, f64), y: &(f64, f64)| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1));
    Some(match best_by(cands, key, asc2) {
        Some((a, k)) => (
            a.number,
            format!("{missing}: scelgo la settimanale più bassa ({}%)", pct(k.0)),
        ),
        None => (
            first.number,
            format!("{missing}: scelgo la {}", explain::order_why(first_pos)),
        ),
    })
}

/// Dato che manca al criterio, per il motivo del ripiego.
fn missing_data(ctx: &Ctx<'_>) -> String {
    match &ctx.rules.reserve_criterion {
        ReserveCriterion::ResetSoonest => "reset settimanale non disponibile".to_owned(),
        ReserveCriterion::PaceSafe => "ritmo non disponibile".to_owned(),
        ReserveCriterion::Lowest5h => "5h non disponibile".to_owned(),
        ReserveCriterion::EvenSpread => "settimanale non disponibile".to_owned(),
        ReserveCriterion::BalancedScore => "dati del punteggio non disponibili".to_owned(),
        ReserveCriterion::ModelHeadroom => match ctx.rules.model() {
            None => "modello non indicato".to_owned(),
            Some(m)
                if super::scoped_names(ctx.list)
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(m)) =>
            {
                format!("{m} non disponibile sulle riserve libere")
            }
            Some(m) => format!("modello {m} sconosciuto: nessuna utenza lo riporta"),
        },
        ReserveCriterion::Unknown(_) => "criterio sconosciuto".to_owned(),
        ReserveCriterion::LowestWeekly | ReserveCriterion::Best | ReserveCriterion::FixedOrder => {
            "dati non disponibili".to_owned()
        }
    }
}

/// Scelta del criterio; `None` se nessuna candidata ha i dati che servono.
fn by_criterion(ctx: &Ctx<'_>, cands: &[(usize, &Account)]) -> Option<(u32, String)> {
    let now = ctx.now;
    let rules = ctx.rules;
    let (t5, t7) = (rules.five_hour_threshold, rules.seven_day_limit());
    let p5 = |a: &Account| effective_pct(win5(a), now);
    let p7 = |a: &Account| effective_pct(win7(a), now);
    let asc2 = |x: &(f64, f64), y: &(f64, f64)| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1));
    match rules.reserve_criterion {
        ReserveCriterion::FixedOrder => cands
            .first()
            .map(|(j, a)| (a.number, explain::order_why(*j))),
        ReserveCriterion::LowestWeekly => best_by(cands, |a| Some((p7(a), p5(a))), asc2)
            .map(|(a, k)| (a.number, format!("settimanale più bassa ({}%)", pct(k.0)))),
        ReserveCriterion::Best => {
            let margin = |a: &Account| (t5 - p5(a)).min(t7 - p7(a));
            best_by(cands, |a| Some(margin(a)), |x, y| y.total_cmp(x)).map(|(a, m)| {
                (
                    a.number,
                    format!("più margine sulle soglie ({} punti)", pct(m)),
                )
            })
        }
        ReserveCriterion::ResetSoonest => best_by(
            cands,
            |a| live_week(a, now).and_then(|w| Some((reset_at(w, now)?, w.pct))),
            |x, y| x.0.cmp(&y.0).then(x.1.total_cmp(&y.1)),
        )
        .map(|(a, (r, _))| {
            (
                a.number,
                format!("reset settimanale più vicino (tra {})", duration(r - now)),
            )
        }),
        ReserveCriterion::PaceSafe => {
            // (arriva al reset, margine sul ritmo, esaurimento previsto se non ci arriva)
            let key = |a: &Account| {
                let w = live_week(a, now)?;
                let lasts = w.will_last_to_reset?;
                let margin = w.expected_pct? - w.pct;
                let exhaust = if lasts {
                    0
                } else {
                    parse_iso(w.projected_exhaustion_at.as_deref()?)?
                };
                Some((lasts, margin, exhaust))
            };
            let cmp = |x: &(bool, f64, i64), y: &(bool, f64, i64)| {
                y.0.cmp(&x.0).then(y.2.cmp(&x.2)).then(y.1.total_cmp(&x.1))
            };
            best_by(cands, key, cmp).map(|(a, (lasts, m, _))| {
                let why = if lasts {
                    format!(
                        "arriva al reset settimanale al ritmo attuale (margine {} punti)",
                        pct(m)
                    )
                } else {
                    "nessuna arriva al reset al ritmo attuale: si esaurisce per ultima".to_owned()
                };
                (a.number, why)
            })
        }
        ReserveCriterion::Lowest5h => best_by(cands, |a| win5(a).map(|_| (p5(a), p7(a))), asc2)
            .map(|(a, k)| (a.number, format!("5h più libera ({}%)", pct(k.0)))),
        ReserveCriterion::BalancedScore => best_by(cands, |a| score(ctx, a), |x, y| y.total_cmp(x))
            .map(|(a, s)| (a.number, format!("punteggio più alto ({s:.2})"))),
        ReserveCriterion::EvenSpread => {
            let levels: Vec<f64> = ctx
                .order
                .iter()
                .filter_map(|n| ctx.account(*n))
                .filter(|a| super::fresh(a, rules, now) && win7(a).is_some())
                .map(p7)
                .collect();
            let mean = levels.iter().sum::<f64>() / levels.len().max(1) as f64;
            best_by(cands, |a| win7(a).map(|_| (p7(a), p5(a))), asc2).map(|(a, k)| {
                (
                    a.number,
                    format!(
                        "più sotto la media settimanale ({}% contro {}%)",
                        pct(k.0),
                        pct(mean)
                    ),
                )
            })
        }
        ReserveCriterion::ModelHeadroom => {
            let model = rules.model()?;
            let used = |a: &Account| {
                let w = a.usage.as_ref()?.scoped.as_ref()?.iter().find(|w| {
                    w.name
                        .as_deref()
                        .is_some_and(|n| n.trim().eq_ignore_ascii_case(model))
                })?;
                Some((effective_pct(Some(w), now), p7(a)))
            };
            best_by(cands, used, asc2).map(|(a, k)| {
                (
                    a.number,
                    format!("più margine su {model} ({}% usato)", pct(k.0)),
                )
            })
        }
        ReserveCriterion::Unknown(_) => None,
    }
}

/// Punteggio di [`ReserveCriterion::BalancedScore`] (0–1): media pesata dei soli fattori con
/// i dati disponibili (pesi rinormalizzati); `None` se non ce n'è nessuno con peso positivo.
fn score(ctx: &Ctx<'_>, a: &Account) -> Option<f64> {
    let now = ctx.now;
    let rules = ctx.rules;
    let w = rules.weights();
    let u = a.usage.as_ref()?;
    let margin = |w: Option<&UsageWindow>, t: f64| w.map(|w| (t - effective_pct(Some(w), now)) / t);
    let week = live_week(a, now);
    let factors = [
        (
            w.five_hour,
            margin(u.five_hour.as_ref(), rules.five_hour_threshold),
        ),
        (
            w.seven_day,
            margin(u.seven_day.as_ref(), rules.seven_day_limit()),
        ),
        (
            w.reset,
            week.and_then(|wk| reset_at(wk, now))
                .map(|r| 1.0 - (r - now) as f64 / WEEK_SECS),
        ),
        (
            w.pace,
            week.and_then(|wk| Some(0.5 + (wk.expected_pct? - wk.pct) / 100.0)),
        ),
    ];
    let (sum, total) = factors
        .iter()
        .filter(|(weight, _)| *weight > 0.0)
        .filter_map(|(weight, value)| value.map(|v| (*weight, v)))
        .fold((0.0, 0.0), |(s, t), (weight, v)| {
            (s + weight * v.clamp(0.0, 1.0), t + weight)
        });
    (total > 0.0).then(|| sum / total)
}
