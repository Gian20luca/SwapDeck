//! Logica delle notifiche (F2): soglie della config sull'account attivo, una sola volta per finestra
//! di reset e per soglia, più un avviso a ogni cambio di account. Funzione pura: chi la usa mostra
//! le [`Notice`] restituite.

use std::collections::HashMap;

use swapdeck_core::cswap::{Account, ListResponse, UsageWindow};

/// Soglie di default (piano, F2).
pub const DEFAULT_THRESHOLDS: [f64; 2] = [80.0, 95.0];
/// Calo della % che indica una finestra nuova anche senza `resetsAt`.
const RESET_DROP_PCT: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Window {
    FiveHour,
    SevenDay,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    Threshold {
        account: u32,
        name: String,
        window: Window,
        threshold: f64,
        pct: f64,
    },
    Switched {
        from: Option<u32>,
        to: u32,
        name: String,
    },
}

#[derive(Debug, Clone)]
struct Mark {
    reset_key: Option<String>,
    last_pct: f64,
    /// Soglia più alta già notificata in questa finestra.
    notified: f64,
}

/// Nome da mostrare: alias, altrimenti il numero (niente email nelle notifiche di sistema).
pub fn display_name(account: &Account) -> String {
    match account.alias.as_deref().map(str::trim) {
        Some(a) if !a.is_empty() => a.to_owned(),
        _ => format!("#{}", account.number),
    }
}

/// Chiave della finestra di reset al minuto (ignora le frazioni di secondo che cambiano tra letture).
fn reset_key(window: &UsageWindow) -> Option<String> {
    window
        .resets_at
        .as_deref()
        .map(|s| s.chars().take(16).collect())
}

#[derive(Debug, Default)]
pub struct Notifier {
    thresholds: Vec<f64>,
    marks: HashMap<(u32, Window), Mark>,
    /// `None` finché non è arrivata la prima lista.
    last_active: Option<Option<u32>>,
}

impl Notifier {
    pub fn new(thresholds: &[f64]) -> Self {
        let mut n = Self::default();
        n.set_thresholds(thresholds);
        n
    }

    pub fn set_thresholds(&mut self, thresholds: &[f64]) {
        let mut t: Vec<f64> = thresholds
            .iter()
            .copied()
            .filter(|t| t.is_finite() && *t > 0.0 && *t <= 100.0)
            .collect();
        t.sort_by(f64::total_cmp);
        t.dedup();
        self.thresholds = t;
    }

    /// Confronta la nuova lista con le precedenti e restituisce le notifiche da mostrare.
    pub fn observe(&mut self, list: &ListResponse) -> Vec<Notice> {
        let mut out = Vec::new();
        let active = list.active_account_number;
        if let Some(prev) = self.last_active
            && prev != active
            && let Some(to) = active
        {
            let name = list
                .accounts
                .iter()
                .find(|a| a.number == to)
                .map(display_name)
                .unwrap_or_else(|| format!("#{to}"));
            out.push(Notice::Switched {
                from: prev,
                to,
                name,
            });
        }
        self.last_active = Some(active);

        let Some(account) = list
            .accounts
            .iter()
            .find(|a| Some(a.number) == active || (active.is_none() && a.active))
        else {
            return out;
        };
        // Solo dati freschi: lastGoodUsage può essere vecchio di ore.
        let Some(usage) = &account.usage else {
            return out;
        };
        for (window, w) in [
            (Window::FiveHour, usage.five_hour.as_ref()),
            (Window::SevenDay, usage.seven_day.as_ref()),
        ] {
            if let Some(w) = w
                && let Some(n) = self.check(account, window, w)
            {
                out.push(n);
            }
        }
        out
    }

    fn check(&mut self, account: &Account, window: Window, w: &UsageWindow) -> Option<Notice> {
        let pct = w.pct;
        if !pct.is_finite() {
            return None;
        }
        let key = reset_key(w);
        let mark = self
            .marks
            .entry((account.number, window))
            .or_insert_with(|| Mark {
                reset_key: key.clone(),
                last_pct: pct,
                notified: f64::NEG_INFINITY,
            });
        let new_window = matches!((&mark.reset_key, &key), (Some(a), Some(b)) if a != b)
            || pct + RESET_DROP_PCT < mark.last_pct;
        if new_window {
            mark.notified = f64::NEG_INFINITY;
        }
        if key.is_some() {
            mark.reset_key = key;
        }
        mark.last_pct = pct;
        let crossed = self
            .thresholds
            .iter()
            .copied()
            .filter(|t| *t <= pct)
            .fold(f64::NEG_INFINITY, f64::max);
        if crossed > mark.notified {
            mark.notified = crossed;
            return Some(Notice::Threshold {
                account: account.number,
                name: display_name(account),
                window,
                threshold: crossed,
                pct,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::fixture_list;

    /// Lista con l'account attivo 6 ai valori dati.
    fn list_with(active: u32, five: f64, resets: &str) -> ListResponse {
        let mut list = fixture_list();
        list.active_account_number = Some(active);
        for a in &mut list.accounts {
            a.active = a.number == active;
            if a.number == active {
                let usage = a.usage.get_or_insert_with(Default::default);
                let w = usage.five_hour.get_or_insert_with(Default::default);
                w.pct = five;
                w.resets_at = Some(resets.to_owned());
                usage.seven_day = None;
            }
        }
        list
    }

    fn thresholds(notices: &[Notice]) -> Vec<f64> {
        notices
            .iter()
            .filter_map(|n| match n {
                Notice::Threshold { threshold, .. } => Some(*threshold),
                Notice::Switched { .. } => None,
            })
            .collect()
    }

    const R1: &str = "2026-10-15T22:20:00.105830+00:00";
    const R1_JITTER: &str = "2026-10-15T22:20:00.987000+00:00";
    const R2: &str = "2026-10-16T03:20:00.105830+00:00";

    #[test]
    fn each_threshold_once_per_window() {
        let mut n = Notifier::new(&DEFAULT_THRESHOLDS);
        assert!(n.observe(&list_with(6, 50.0, R1)).is_empty());
        assert_eq!(thresholds(&n.observe(&list_with(6, 81.0, R1))), [80.0]);
        assert!(n.observe(&list_with(6, 85.0, R1_JITTER)).is_empty());
        assert!(n.observe(&list_with(6, 82.0, R1)).is_empty());
        assert_eq!(thresholds(&n.observe(&list_with(6, 95.5, R1))), [95.0]);
        assert!(n.observe(&list_with(6, 99.0, R1)).is_empty());
        // Nuova finestra: si riparte.
        assert!(n.observe(&list_with(6, 2.0, R2)).is_empty());
        assert_eq!(thresholds(&n.observe(&list_with(6, 80.0, R2))), [80.0]);
    }

    #[test]
    fn jump_over_two_thresholds_notifies_only_the_highest() {
        let mut n = Notifier::new(&DEFAULT_THRESHOLDS);
        let first = n.observe(&list_with(6, 96.0, R1));
        assert_eq!(thresholds(&first), [95.0]);
        assert!(n.observe(&list_with(6, 97.0, R1)).is_empty());
    }

    #[test]
    fn drop_without_reset_time_rearms() {
        let mut n = Notifier::new(&[90.0]);
        let mut list = list_with(6, 91.0, R1);
        let strip = |l: &mut ListResponse| {
            for a in &mut l.accounts {
                if let Some(w) = a.usage.as_mut().and_then(|u| u.five_hour.as_mut()) {
                    w.resets_at = None;
                }
            }
        };
        strip(&mut list);
        assert_eq!(thresholds(&n.observe(&list)), [90.0]);
        let mut low = list_with(6, 3.0, R1);
        strip(&mut low);
        assert!(n.observe(&low).is_empty());
        assert_eq!(thresholds(&n.observe(&list)), [90.0]);
    }

    #[test]
    fn switch_is_notified_except_on_first_list() {
        let mut n = Notifier::new(&[]);
        assert!(n.observe(&list_with(6, 10.0, R1)).is_empty());
        let notices = n.observe(&list_with(3, 10.0, R1));
        assert_eq!(
            notices,
            [Notice::Switched {
                from: Some(6),
                to: 3,
                name: "alias3".into()
            }]
        );
        assert!(n.observe(&list_with(3, 10.0, R1)).is_empty());
    }

    #[test]
    fn thresholds_are_sanitized() {
        let n = Notifier::new(&[95.0, f64::NAN, 80.0, 80.0, -1.0, 150.0]);
        assert_eq!(n.thresholds, [80.0, 95.0]);
    }

    #[test]
    fn display_name_never_uses_email() {
        let list = fixture_list();
        let mut a = list.accounts[0].clone();
        assert_eq!(display_name(&a), "alias1");
        a.alias = Some("  ".into());
        assert_eq!(display_name(&a), "#1");
    }
}
