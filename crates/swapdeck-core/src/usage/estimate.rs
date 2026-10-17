//! Stima del consumo degli altri dispositivi (metodo in `docs/CHI-CONSUMA.md`).
//!
//! 1. **Intervalli.** Per ogni utenza e finestra (5h, 7d) i campioni dello storico con lo stesso
//!    `resetsAt` (stessa finestra del server) si uniscono in intervalli finché la percentuale sale
//!    di almeno [`MIN_POINTS_5H`] o [`MIN_POINTS_7D`] punti (le percentuali sono intere). Per ogni
//!    intervallo: aumento sul server `Δp` e token equivalenti locali `U` attribuiti all'utenza, per
//!    modello (serie al minuto dell'indice). Intervalli con molto consumo di utenza sconosciuta
//!    si scartano.
//! 2. **Calibrazione.** Rapporto `r = U / Δp` (token equivalenti per punto). Dove consuma anche un
//!    altro dispositivo `Δp` è più alto e `r` più basso: si parte dall'80° percentile e si tiene
//!    il gruppo alto con una mediana iterata (`r` entro `(1 ± TOL) · k`), poi
//!    `k = ΣU / ΣΔp` sul gruppo tenuto. Un `k` per modello dove il modello fa almeno l'80% del
//!    consumo dell'intervallo, altrimenti quello dell'utenza (tutti i modelli insieme). Servono
//!    almeno [`MIN_SAMPLES`] intervalli.
//! 3. **Stima.** Nella finestra corrente: punti locali `Σ U_m / k_m`, altri dispositivi
//!    `max(0, p_server − punti locali)`. Fiducia dai campioni e dalla dispersione (MAD/mediana);
//!    bassa se lo storico non copre bene la finestra.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::attribution::Timeline;
use super::index::UsageIndex;
use super::{BreakdownWindow, TokenCounts};
use crate::cswap::UsageStatus;
use crate::history::UsageSample;
use crate::timeutil::parse_iso;

/// Aumento minimo di un intervallo di calibrazione (punti), finestra di 5 ore.
pub const MIN_POINTS_5H: f64 = 3.0;
/// Aumento minimo di un intervallo di calibrazione (punti), finestra di 7 giorni.
pub const MIN_POINTS_7D: f64 = 2.0;
/// Intervalli coerenti minimi per una calibrazione.
pub const MIN_SAMPLES: usize = 3;
/// Tolleranza della selezione degli intervalli «solo locali».
const TOL: f64 = 0.4;
/// Punto di partenza della selezione (quantile dei rapporti).
const START_QUANTILE: f64 = 0.8;
/// Quota minima di un modello in un intervallo per calibrarlo da solo.
const MODEL_SHARE: f64 = 0.8;
/// Intervalli minimi per usare il `k` di un modello invece di quello dell'utenza.
pub const MIN_MODEL_SAMPLES: usize = 5;
/// Quota di consumo locale stimata con il `k` dell'utenza (modelli senza calibrazione propria)
/// oltre la quale la fiducia scende di un grado.
const FALLBACK_SHARE: f64 = 0.1;
/// Quota massima di consumo di utenza sconosciuta in un intervallo di calibrazione.
const MAX_UNKNOWN_SHARE: f64 = 0.1;

const FIVE_HOURS: i64 = 5 * 3600;
const SEVEN_DAYS: i64 = 7 * 86_400;

/// Grado di fiducia della stima.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// Stato della stima.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum EstimateStatus {
    /// Stima calcolata.
    Available,
    /// Campioni insufficienti: «stima non ancora disponibile».
    NotCalibrated,
    /// Nessun campione della finestra corrente.
    NoData,
}

/// Quota stimata degli altri dispositivi per un'utenza e una finestra del server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct OtherDevicesEstimate {
    pub account: u32,
    pub window: BreakdownWindow,
    pub status: EstimateStatus,
    /// Inizio della finestra del server considerata (secondi Unix).
    #[ts(type = "number | null")]
    pub from: Option<i64>,
    /// Ora dell'ultima lettura della percentuale.
    #[ts(type = "number | null")]
    pub to: Option<i64>,
    /// Percentuale sul server all'ultima lettura.
    pub server_pct: Option<f64>,
    /// Punti spiegati dal consumo di questo PC.
    pub local_pct: Option<f64>,
    /// Punti non spiegati (altri dispositivi), mai negativi.
    pub other_pct: Option<f64>,
    /// Token equivalenti corrispondenti a `other_pct`.
    pub other_equivalent: Option<f64>,
    /// Token equivalenti per punto (calibrazione dell'utenza, tutti i modelli).
    pub tokens_per_point: Option<f64>,
    pub confidence: Option<Confidence>,
    /// Intervalli usati per la calibrazione.
    pub samples: u32,
    /// Lo storico non copre bene la finestra (consumo di utenza sconosciuta).
    pub history_gaps: bool,
}

/// Calibrazione di un'utenza in una finestra.
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    /// Token equivalenti per punto, tutti i modelli.
    pub pooled: f64,
    /// Per modello (indice dei modelli dell'indice).
    pub per_model: HashMap<u32, f64>,
    pub samples: usize,
    pub confidence: Confidence,
}

/// Intervallo di osservazione.
#[derive(Debug, Clone, PartialEq)]
pub struct Interval {
    pub from: i64,
    pub to: i64,
    pub points: f64,
    /// Token equivalenti locali per modello.
    pub local: HashMap<u32, f64>,
    pub unknown: f64,
}

impl Interval {
    fn local_total(&self) -> f64 {
        self.local.values().sum()
    }
}

/// Punto della serie: istante della lettura, percentuale, reset.
#[derive(Debug, Clone, Copy)]
struct Point {
    t: i64,
    pct: f64,
    resets: i64,
}

fn points(samples: &[&UsageSample], window: BreakdownWindow) -> Vec<Point> {
    let mut out: Vec<Point> = samples
        .iter()
        .filter(|s| s.status == UsageStatus::Ok)
        .filter_map(|s| {
            let (pct, resets) = match window {
                BreakdownWindow::FiveHour => (s.five_hour_pct?, s.five_hour_resets_at.as_deref()?),
                _ => (s.seven_day_pct?, s.seven_day_resets_at.as_deref()?),
            };
            let t = s.fetched_at.as_deref().and_then(parse_iso).unwrap_or(s.ts);
            Some(Point {
                t,
                pct,
                resets: parse_iso(resets)?,
            })
        })
        .collect();
    out.sort_by_key(|p| p.t);
    out.dedup_by_key(|p| p.t);
    out
}

/// Stessa finestra del server (il reset può oscillare di qualche secondo tra letture).
fn same_window(a: i64, b: i64) -> bool {
    (a - b).abs() <= 300
}

/// Serie al minuto di un'utenza: (minuto, modello) → token equivalenti.
pub struct MinuteSeries {
    /// In ordine di minuto: (minuto, modello, equivalenti, di utenza sconosciuta).
    rows: Vec<(i64, u32, f64, bool)>,
}

impl MinuteSeries {
    pub fn for_account(index: &UsageIndex, account: u32) -> Self {
        let mut rows: Vec<(i64, u32, f64, bool)> = index
            .minutes
            .iter()
            .filter(|((_, a, _), _)| a.is_none() || *a == Some(account))
            .map(|(&(t, a, m), c)| (t, m, c.equivalent(), a.is_none()))
            .collect();
        rows.sort_by(|x, y| x.0.cmp(&y.0).then(x.1.cmp(&y.1)));
        Self { rows }
    }

    /// Consumo in `[from, to)`: per modello e sconosciuto.
    pub fn sum(&self, from: i64, to: i64) -> (HashMap<u32, f64>, f64) {
        let start = self.rows.partition_point(|r| r.0 < from);
        let mut local: HashMap<u32, f64> = HashMap::new();
        let mut unknown = 0.0;
        for &(t, m, units, unk) in &self.rows[start..] {
            if t >= to {
                break;
            }
            if unk {
                unknown += units;
            } else {
                *local.entry(m).or_default() += units;
            }
        }
        (local, unknown)
    }
}

/// Intervalli di una finestra per un'utenza.
pub fn intervals(
    account: u32,
    samples: &[&UsageSample],
    window: BreakdownWindow,
    series: &MinuteSeries,
    timeline: &Timeline,
) -> Vec<Interval> {
    let (min_points, max_span) = match window {
        BreakdownWindow::FiveHour => (MIN_POINTS_5H, 3 * 3600),
        _ => (MIN_POINTS_7D, 24 * 3600),
    };
    let pts = points(samples, window);
    let mut out = Vec::new();
    let mut anchor: Option<Point> = None;
    for p in pts {
        let Some(a) = anchor else {
            anchor = Some(p);
            continue;
        };
        let delta = p.pct - a.pct;
        if !same_window(a.resets, p.resets) || delta < 0.0 || p.t - a.t > max_span {
            anchor = Some(p);
            continue;
        }
        // Utenza attiva per tutto l'intervallo più un giro di margine per lato (un cambio fatto
        // fuori da SwapDeck si vede solo al giro dopo): altrimenti il consumo locale è incerto.
        let margin = timeline.period_at(p.t);
        if delta >= min_points && !timeline.active_throughout(account, a.t - margin, p.t + margin) {
            anchor = Some(p);
            continue;
        }
        if delta >= min_points {
            let (local, unknown) = series.sum(a.t, p.t);
            out.push(Interval {
                from: a.t,
                to: p.t,
                points: delta,
                local,
                unknown,
            });
            anchor = Some(p);
        }
    }
    out
}

fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n == 0 {
        0.0
    } else if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// Gruppo «solo locale» di rapporti (U, Δp): k, campioni, dispersione.
fn robust_ratio(pairs: &[(f64, f64)]) -> Option<(f64, usize, f64)> {
    let pairs: Vec<(f64, f64)> = pairs
        .iter()
        .copied()
        .filter(|&(u, p)| u > 0.0 && p > 0.0)
        .collect();
    if pairs.len() < MIN_SAMPLES {
        return None;
    }
    // Si parte dall'alto (80° percentile): il consumo altrui abbassa solo i rapporti.
    let mut sorted: Vec<f64> = pairs.iter().map(|(u, p)| u / p).collect();
    sorted.sort_by(f64::total_cmp);
    let mut k = sorted[((sorted.len() - 1) as f64 * START_QUANTILE).round() as usize];
    let mut kept: Vec<(f64, f64)> = pairs.clone();
    for _ in 0..8 {
        let next: Vec<(f64, f64)> = pairs
            .iter()
            .copied()
            .filter(|&(u, p)| {
                let r = u / p;
                r >= k * (1.0 - TOL) && r <= k * (1.0 + TOL)
            })
            .collect();
        if next.len() < MIN_SAMPLES {
            break;
        }
        let nk = median(&mut next.iter().map(|(u, p)| u / p).collect::<Vec<_>>());
        let stable = next == kept && (nk - k).abs() <= k * 1e-9;
        kept = next;
        k = nk;
        if stable {
            break;
        }
    }
    if kept.len() < MIN_SAMPLES {
        return None;
    }
    let (su, sp) = kept
        .iter()
        .fold((0.0, 0.0), |(a, b), &(u, p)| (a + u, b + p));
    let mut ratios: Vec<f64> = kept.iter().map(|(u, p)| u / p).collect();
    let med = median(&mut ratios);
    let mut dev: Vec<f64> = ratios.iter().map(|r| (r - med).abs()).collect();
    let disp = if med > 0.0 {
        median(&mut dev) / med
    } else {
        1.0
    };
    Some((su / sp, kept.len(), disp))
}

fn confidence(samples: usize, disp: f64) -> Confidence {
    if samples >= 8 && disp <= 0.2 {
        Confidence::High
    } else if samples >= 5 && disp <= 0.35 {
        Confidence::Medium
    } else {
        Confidence::Low
    }
}

/// Calibrazione dagli intervalli; `None` con dati insufficienti.
pub fn calibrate(intervals: &[Interval]) -> Option<Calibration> {
    let usable: Vec<&Interval> = intervals
        .iter()
        .filter(|i| {
            let local = i.local_total();
            i.unknown <= MAX_UNKNOWN_SHARE * (local + i.unknown)
        })
        .collect();
    let pooled_pairs: Vec<(f64, f64)> =
        usable.iter().map(|i| (i.local_total(), i.points)).collect();
    let (pooled, samples, disp) = robust_ratio(&pooled_pairs)?;
    let mut by_model: BTreeMap<u32, Vec<(f64, f64)>> = BTreeMap::new();
    for i in &usable {
        let total = i.local_total();
        for (&m, &u) in &i.local {
            if total > 0.0 && u >= MODEL_SHARE * total {
                by_model.entry(m).or_default().push((u, i.points));
            }
        }
    }
    let per_model = by_model
        .into_iter()
        .filter_map(|(m, pairs)| match robust_ratio(&pairs) {
            Some((k, n, _)) if n >= MIN_MODEL_SAMPLES => Some((m, k)),
            _ => None,
        })
        .collect();
    Some(Calibration {
        pooled,
        per_model,
        samples,
        confidence: confidence(samples, disp),
    })
}

/// Stima della finestra corrente di un'utenza.
pub fn estimate(
    account: u32,
    window: BreakdownWindow,
    samples: &[&UsageSample],
    series: &MinuteSeries,
    timeline: &Timeline,
    now: i64,
) -> OtherDevicesEstimate {
    let mut out = OtherDevicesEstimate {
        account,
        window,
        status: EstimateStatus::NoData,
        from: None,
        to: None,
        server_pct: None,
        local_pct: None,
        other_pct: None,
        other_equivalent: None,
        tokens_per_point: None,
        confidence: None,
        samples: 0,
        history_gaps: false,
    };
    let length = if window == BreakdownWindow::FiveHour {
        FIVE_HOURS
    } else {
        SEVEN_DAYS
    };
    let pts = points(samples, window);
    let Some(last) = pts.last().copied().filter(|p| p.resets > now) else {
        return out;
    };
    // Inizio: reset meno la durata, ma non prima del reset della finestra precedente.
    let previous_reset = pts
        .iter()
        .rev()
        .find(|p| !same_window(p.resets, last.resets) && p.resets <= last.resets)
        .map(|p| p.resets);
    let from = (last.resets - length).max(previous_reset.unwrap_or(i64::MIN));
    out.from = Some(from);
    out.to = Some(last.t);
    out.server_pct = Some(last.pct);
    let cal = calibrate(&intervals(account, samples, window, series, timeline));
    let Some(cal) = cal else {
        out.status = EstimateStatus::NotCalibrated;
        return out;
    };
    let (local, unknown) = series.sum(from, last.t);
    let local_pct: f64 = local
        .iter()
        .map(|(m, u)| u / cal.per_model.get(m).copied().unwrap_or(cal.pooled))
        .sum();
    let known: f64 = local.values().sum();
    let fallback: f64 = local
        .iter()
        .filter(|(m, _)| !cal.per_model.contains_key(m))
        .map(|(_, u)| u)
        .sum();
    out.history_gaps = unknown > MAX_UNKNOWN_SHARE * (known + unknown);
    let other = (last.pct - local_pct).max(0.0);
    out.status = EstimateStatus::Available;
    out.local_pct = Some(round2(local_pct));
    out.other_pct = Some(round2(other));
    out.other_equivalent = Some((other * cal.pooled).round());
    out.tokens_per_point = Some(cal.pooled.round());
    out.samples = cal.samples as u32;
    out.confidence = Some(if out.history_gaps {
        Confidence::Low
    } else if fallback > FALLBACK_SHARE * known {
        lower(cal.confidence)
    } else {
        cal.confidence
    });
    out
}

fn lower(c: Confidence) -> Confidence {
    match c {
        Confidence::High => Confidence::Medium,
        _ => Confidence::Low,
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Token equivalenti (pesi in [`super::WEIGHTS`]).
impl TokenCounts {
    pub fn equivalent(&self) -> f64 {
        let w = super::WEIGHTS;
        self.input as f64 * w[0]
            + self.output as f64 * w[1]
            + self.cache_creation as f64 * w[2]
            + self.cache_read as f64 * w[3]
    }
}
