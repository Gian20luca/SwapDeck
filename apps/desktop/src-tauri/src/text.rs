//! Testi lato Rust (menu, tooltip, notifiche) in italiano e in inglese.

use swapdeck_core::cswap::{CswapError, ListResponse};

use crate::notify::display_name;

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
    pub map: &'static str,
}

pub fn labels(lang: Lang) -> Labels {
    match lang {
        Lang::It => Labels {
            open: "Apri",
            mini: "Mini-widget",
            refresh: "Aggiorna",
            quit: "Esci",
            no_accounts: "Nessun account",
            map: "Mappa",
        },
        Lang::En => Labels {
            open: "Open",
            mini: "Mini widget",
            refresh: "Refresh",
            quit: "Quit",
            no_accounts: "No accounts",
            map: "Map",
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

/// Testo dell'errore di un cambio dal menu.
pub fn switch_failed(lang: Lang, err: &CswapError) -> (String, String) {
    let title = match lang {
        Lang::It => "Cambio di account non riuscito",
        Lang::En => "Account switch failed",
    };
    (title.into(), err.message.clone())
}

pub fn map_failed(lang: Lang, err: &CswapError) -> (String, String) {
    let title = match lang {
        Lang::It => "Cambio di mappa non riuscito",
        Lang::En => "Map change failed",
    };
    (title.into(), err.message.clone())
}

/// Aggiornamento scaricato e verificato: si installa all'uscita dall'app.
pub fn update_ready(lang: Lang, version: &str) -> (String, String) {
    match lang {
        Lang::It => (
            format!("SwapDeck {version} pronto"),
            "L'aggiornamento verrà installato all'uscita dall'app.".into(),
        ),
        Lang::En => (
            format!("SwapDeck {version} ready"),
            "The update will be installed when you quit the app.".into(),
        ),
    }
}

/// Riga del tooltip con il motore sospeso (conflitto o regole da sistemare).
pub fn engine_paused(lang: Lang, detail: &[String]) -> String {
    let head = match lang {
        Lang::It => "Motore sospeso",
        Lang::En => "Engine suspended",
    };
    if detail.is_empty() {
        head.to_owned()
    } else {
        format!("{head}: {}", detail.join(", "))
    }
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
    fn labels() {
        let list = fixture_list();
        assert_eq!(account_label(&list.accounts[4]), "alias6 (6) · 5h 66%");
        let mut no_data = list.accounts[1].clone();
        no_data.usage = None;
        no_data.last_good_usage = None;
        assert_eq!(account_label(&no_data), "alias2 (2) · 5h –");
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
        assert_eq!(engine_paused(Lang::It, &[]), "Motore sospeso");
        assert_eq!(
            engine_paused(Lang::En, &["cswap-guard".into()]),
            "Engine suspended: cswap-guard"
        );
    }
}
