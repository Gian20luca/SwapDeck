//! Testi lato Rust (menu, tooltip, notifiche) in italiano e in inglese.

use swapdeck_core::cswap::{CswapError, ListResponse};

use crate::notify::{Notice, Window, display_name};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    It,
    En,
}

pub struct Labels {
    pub open: &'static str,
    pub mini: &'static str,
    pub refresh: &'static str,
    pub quit: &'static str,
    pub no_accounts: &'static str,
}

pub fn labels(lang: Lang) -> Labels {
    match lang {
        Lang::It => Labels {
            open: "Apri",
            mini: "Mini-widget",
            refresh: "Aggiorna",
            quit: "Esci",
            no_accounts: "Nessun account",
        },
        Lang::En => Labels {
            open: "Open",
            mini: "Mini widget",
            refresh: "Refresh",
            quit: "Quit",
            no_accounts: "No accounts",
        },
    }
}

fn pct(v: Option<f64>) -> String {
    match v {
        Some(p) if p.is_finite() => format!("{}%", p.round() as i64),
        _ => "–".into(),
    }
}

/// % delle 5 ore dell'account attivo (dato fresco, altrimenti l'ultimo buono).
pub fn active_five_hour(list: &ListResponse) -> Option<f64> {
    let active = list.accounts.iter().find(|a| a.active)?;
    let usage = active.usage.as_ref().or(active.last_good_usage.as_ref())?;
    usage.five_hour.as_ref().map(|w| w.pct)
}

/// Tooltip dell'icona: alias, 5h e 7d dell'account attivo, oppure l'errore.
pub fn tooltip(lang: Lang, list: Option<&ListResponse>, error: Option<&CswapError>) -> String {
    let mut text = String::from("SwapDeck");
    match list.and_then(|l| l.accounts.iter().find(|a| a.active)) {
        Some(a) => {
            let usage = a.usage.as_ref().or(a.last_good_usage.as_ref());
            let five = usage.and_then(|u| u.five_hour.as_ref()).map(|w| w.pct);
            let seven = usage.and_then(|u| u.seven_day.as_ref()).map(|w| w.pct);
            text.push_str(&format!(
                "\n{}\n5h {} · 7d {}",
                display_name(a),
                pct(five),
                pct(seven)
            ));
        }
        None if list.is_some() => {
            text.push('\n');
            text.push_str(labels(lang).no_accounts);
        }
        None => {}
    }
    if let Some(e) = error {
        let prefix = match lang {
            Lang::It => "Errore",
            Lang::En => "Error",
        };
        let msg: String = e.message.chars().take(60).collect();
        text.push_str(&format!("\n{prefix}: {msg}"));
    }
    text
}

/// Voce del menu per un account: «alias (n) · 5h 42%».
pub fn account_label(account: &swapdeck_core::cswap::Account) -> String {
    let five = account
        .usage
        .as_ref()
        .or(account.last_good_usage.as_ref())
        .and_then(|u| u.five_hour.as_ref())
        .map(|w| w.pct);
    let name = display_name(account);
    let name = if name.starts_with('#') {
        name
    } else {
        format!("{name} ({})", account.number)
    };
    format!("{name} · 5h {}", pct(five))
}

/// Titolo e testo di una notifica.
pub fn notice(lang: Lang, n: &Notice) -> (String, String) {
    match (lang, n) {
        (
            Lang::It,
            Notice::Threshold {
                name,
                window,
                threshold,
                pct: p,
                ..
            },
        ) => (
            format!("{name}: {} sulle {}", pct(Some(*p)), window_it(*window)),
            format!("Soglia superata: {}.", pct(Some(*threshold))),
        ),
        (
            Lang::En,
            Notice::Threshold {
                name,
                window,
                threshold,
                pct: p,
                ..
            },
        ) => (
            format!(
                "{name}: {} of the {} limit",
                pct(Some(*p)),
                window_en(*window)
            ),
            format!("Crossed the {} threshold.", pct(Some(*threshold))),
        ),
        (Lang::It, Notice::Switched { from, name, .. }) => (
            format!("Account attivo: {name}"),
            match from {
                Some(f) => format!("Cambio dall'account {f}."),
                None => "Account attivato.".into(),
            },
        ),
        (Lang::En, Notice::Switched { from, name, .. }) => (
            format!("Active account: {name}"),
            match from {
                Some(f) => format!("Switched from account {f}."),
                None => "Account activated.".into(),
            },
        ),
    }
}

fn window_it(w: Window) -> &'static str {
    match w {
        Window::FiveHour => "5 ore",
        Window::SevenDay => "7 giorni",
    }
}

fn window_en(w: Window) -> &'static str {
    match w {
        Window::FiveHour => "5-hour",
        Window::SevenDay => "7-day",
    }
}

/// Testo dell'errore di un cambio dal menu.
pub fn switch_failed(lang: Lang, err: &CswapError) -> (String, String) {
    let title = match lang {
        Lang::It => "Cambio di account non riuscito",
        Lang::En => "Account switch failed",
    };
    (title.into(), err.message.clone())
}

/// `cswap auto` avviato dall'app è terminato da solo.
pub fn auto_ended(lang: Lang, code: Option<i32>, err: Option<&CswapError>) -> (String, String) {
    let title = match (lang, err.is_some()) {
        (Lang::It, false) => "Cambio automatico terminato",
        (Lang::It, true) => "Cambio automatico fermato da un errore",
        (Lang::En, false) => "Automatic switching ended",
        (Lang::En, true) => "Automatic switching stopped by an error",
    };
    let body = match (err, code) {
        (Some(e), _) => e.message.clone(),
        (None, Some(c)) => match lang {
            Lang::It => format!("cswap auto è uscito con codice {c}."),
            Lang::En => format!("cswap auto exited with code {c}."),
        },
        (None, None) => String::new(),
    };
    (title.into(), body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::fixture_list;
    use swapdeck_core::cswap::CswapErrorKind;

    #[test]
    fn tooltip_shows_active_account_and_error() {
        let list = fixture_list();
        let t = tooltip(Lang::It, Some(&list), None);
        assert_eq!(t, "SwapDeck\nalias6\n5h 66% · 7d 77%");
        assert!(!t.contains('@'));
        let err = CswapError::new(CswapErrorKind::Timeout, "tempo scaduto");
        let t = tooltip(Lang::En, None, Some(&err));
        assert_eq!(t, "SwapDeck\nError: tempo scaduto");
        assert_eq!(active_five_hour(&list), Some(66.0));
    }

    #[test]
    fn labels_and_notices() {
        let list = fixture_list();
        assert_eq!(account_label(&list.accounts[4]), "alias6 (6) · 5h 66%");
        let mut no_data = list.accounts[1].clone();
        no_data.usage = None;
        no_data.last_good_usage = None;
        assert_eq!(account_label(&no_data), "alias2 (2) · 5h –");
        let n = Notice::Threshold {
            account: 6,
            name: "alias6".into(),
            window: Window::FiveHour,
            threshold: 80.0,
            pct: 81.4,
        };
        assert_eq!(
            notice(Lang::It, &n),
            (
                "alias6: 81% sulle 5 ore".into(),
                "Soglia superata: 80%.".into()
            )
        );
        let s = Notice::Switched {
            from: Some(6),
            to: 3,
            name: "alias3".into(),
        };
        assert_eq!(notice(Lang::En, &s).0, "Active account: alias3");
    }

    #[test]
    fn auto_ended_texts() {
        let err = CswapError::new(CswapErrorKind::Failed, "nessun account");
        let (t, b) = auto_ended(Lang::It, Some(1), Some(&err));
        assert_eq!(t, "Cambio automatico fermato da un errore");
        assert_eq!(b, "nessun account");
        let (t, b) = auto_ended(Lang::En, Some(0), None);
        assert_eq!(t, "Automatic switching ended");
        assert!(b.contains("code 0"));
    }
}
