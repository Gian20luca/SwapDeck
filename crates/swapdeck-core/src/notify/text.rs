//! Testi delle notifiche in italiano e in inglese (niente email: solo alias o `#N`).

use time::OffsetDateTime;

use super::LocalizedText;
use crate::engine::{SuspendReason, SwitchCause};

pub(super) type Texts = (LocalizedText, LocalizedText);

fn lt(it: impl Into<String>, en: impl Into<String>) -> LocalizedText {
    LocalizedText {
        it: it.into(),
        en: en.into(),
    }
}

/// Finestra di consumo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Win {
    FiveHour,
    SevenDay,
    Model(String),
}

impl Win {
    pub(super) fn id(&self) -> String {
        match self {
            Self::FiveHour => "5h".into(),
            Self::SevenDay => "7d".into(),
            Self::Model(n) => format!("m-{n}"),
        }
    }

    /// Durata della finestra in secondi (le finestre per modello sono settimanali).
    pub(super) fn len_secs(&self) -> i64 {
        match self {
            Self::FiveHour => 5 * 3600,
            _ => 7 * 86_400,
        }
    }

    fn it(&self) -> String {
        match self {
            Self::FiveHour => "5 ore".into(),
            Self::SevenDay => "7 giorni".into(),
            Self::Model(n) => format!("modello {n}"),
        }
    }

    fn en(&self) -> String {
        match self {
            Self::FiveHour => "5h".into(),
            Self::SevenDay => "7d".into(),
            Self::Model(n) => format!("{n} model"),
        }
    }
}

/// Numero senza decimali inutili (80, 82.5).
pub(super) fn num(v: f64) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// Ora locale `HH:MM`, con `GG/MM` davanti se non è oggi.
pub(super) fn clock(ts: i64, now: i64, offset: i32) -> String {
    let local = |t: i64| OffsetDateTime::from_unix_timestamp(t + i64::from(offset)).ok();
    let (Some(t), Some(n)) = (local(ts), local(now)) else {
        return "?".into();
    };
    let hm = format!("{:02}:{:02}", t.hour(), t.minute());
    if t.date() == n.date() {
        hm
    } else {
        format!("{:02}/{:02} {hm}", t.day(), u8::from(t.month()))
    }
}

fn cause(c: Option<SwitchCause>) -> (&'static str, &'static str) {
    match c {
        Some(SwitchCause::PrimaryAtLimit) => {
            ("prima della lista al limite", "first account at limit")
        }
        Some(SwitchCause::ReserveAtLimit) => ("riserva al limite", "reserve at limit"),
        Some(SwitchCause::ActiveNeedsRelogin) => ("login da rifare", "login required"),
        Some(SwitchCause::PrimaryReset) => ("reset della prima della lista", "first account reset"),
        Some(SwitchCause::ReservesAtLimit) => ("riserve al limite", "reserves at limit"),
        Some(SwitchCause::EarlierAvailable) => (
            "un'utenza prima nella lista è di nuovo libera",
            "an earlier account in the list is free again",
        ),
        None => ("cambio manuale o esterno", "manual or external switch"),
    }
}

pub(super) fn preview(
    active: &str,
    target: Option<&str>,
    win: &Win,
    (pct, threshold): (f64, f64),
    eta_min: Option<i64>,
) -> Texts {
    let (tit, ten) = target.map_or(("un'altra utenza", "another account"), |t| (t, t));
    let body = match eta_min {
        Some(m) => lt(
            format!(
                "Tra circa {m} min passo a {tit}: {active} è al {}% su {} (soglia {}%).",
                num(pct),
                win.it(),
                num(threshold)
            ),
            format!(
                "In about {m} min switching to {ten}: {active} is at {}% of {} (threshold {}%).",
                num(pct),
                win.en(),
                num(threshold)
            ),
        ),
        None => lt(
            format!(
                "{active} è a {} punti dalla soglia {} ({}%): poi passo a {tit}.",
                num(threshold - pct),
                win.it(),
                num(threshold)
            ),
            format!(
                "{active} is {} points from the {} threshold ({}%): then switching to {ten}.",
                num(threshold - pct),
                win.en(),
                num(threshold)
            ),
        ),
    };
    (
        lt("Cambio di utenza in arrivo", "Account switch coming up"),
        body,
    )
}

pub(super) fn switched(
    from: Option<&str>,
    to: &str,
    c: Option<SwitchCause>,
    profile: Option<&str>,
) -> Texts {
    let (cit, cen) = cause(c);
    let (fit, fen) = from.map_or_else(
        || (String::new(), String::new()),
        |f| (format!("Da {f}: "), format!("From {f}: ")),
    );
    let (pit, pen) = profile.map_or_else(
        || (String::new(), String::new()),
        |p| (format!(" Mappa: {p}."), format!(" Map: {p}.")),
    );
    (
        lt(
            format!("Utenza attiva: {to}"),
            format!("Active account: {to}"),
        ),
        lt(format!("{fit}{cit}.{pit}"), format!("{fen}{cen}.{pen}")),
    )
}

pub(super) fn returned(to: &str, profile: Option<&str>) -> Texts {
    let (pit, pen) = profile.map_or_else(
        || (String::new(), String::new()),
        |p| (format!(" Mappa: {p}."), format!(" Map: {p}.")),
    );
    (
        lt(format!("Di nuovo su {to}"), format!("Back to {to}")),
        lt(
            format!("Reset avvenuto: si torna alla prima della lista.{pit}"),
            format!("Reset done: back to the first account in the list.{pen}"),
        ),
    )
}

pub(super) fn no_reserve(nearest: Option<(&str, String)>) -> Texts {
    let body = match nearest {
        Some((name, at)) => lt(
            format!("Tutte le utenze sono al limite. Primo reset: {name} alle {at}."),
            format!("All accounts are at their limit. Next reset: {name} at {at}."),
        ),
        None => lt(
            "Tutte le utenze sono al limite e nessun reset è noto.",
            "All accounts are at their limit and no reset time is known.",
        ),
    };
    (
        lt("Nessuna riserva disponibile", "No reserve available"),
        body,
    )
}

pub(super) fn threshold(name: &str, win: &Win, t: f64, pct: f64) -> Texts {
    (
        lt(
            format!("{name}: {}% su {}", num(pct), win.it()),
            format!("{name}: {}% of {}", num(pct), win.en()),
        ),
        lt(
            format!("Superata la soglia del {}%.", num(t)),
            format!("Crossed the {}% threshold.", num(t)),
        ),
    )
}

pub(super) fn exhaustion(name: &str, win: &Win, at: &str, reset: &str) -> Texts {
    (
        lt(
            format!("{name}: esaurimento previsto"),
            format!("{name}: projected to run out"),
        ),
        lt(
            format!(
                "Al ritmo attuale {} finisce alle {at}, prima del reset ({reset}).",
                win.it()
            ),
            format!(
                "At the current pace {} runs out at {at}, before the reset ({reset}).",
                win.en()
            ),
        ),
    )
}

pub(super) fn reset_soon(name: &str, win: &Win, minutes: i64, at: &str) -> Texts {
    (
        lt(
            format!("{name}: reset tra {minutes} min"),
            format!("{name}: reset in {minutes} min"),
        ),
        lt(
            format!("La finestra di {} riparte alle {at}.", win.it()),
            format!("The {} window resets at {at}.", win.en()),
        ),
    )
}

pub(super) fn reset_done(name: &str, win: &Win) -> Texts {
    (
        lt(
            format!("{name}: reset avvenuto"),
            format!("{name}: reset done"),
        ),
        lt(
            format!("La finestra di {} è ripartita da zero.", win.it()),
            format!("The {} window started over.", win.en()),
        ),
    )
}

pub(super) fn engine_suspended(reason: SuspendReason, detail: &[String]) -> Texts {
    let (it, en) = match reason {
        SuspendReason::Conflict => ("un altro motore è attivo", "another engine is running"),
        SuspendReason::InvalidRules => ("regole non valide", "invalid rules"),
        SuspendReason::NoActiveAccount => {
            ("nessuna utenza attiva gestita", "no managed active account")
        }
        SuspendReason::NoViableTarget => ("nessuna utenza disponibile", "no account available"),
        SuspendReason::Disabled => ("motore spento", "engine off"),
    };
    let extra = if detail.is_empty() {
        String::new()
    } else {
        format!(" ({})", detail.join(", "))
    };
    (
        lt("Motore sospeso", "Engine paused"),
        lt(
            format!("Motivo: {it}{extra}."),
            format!("Reason: {en}{extra}."),
        ),
    )
}

pub(super) fn engine_resumed() -> Texts {
    (
        lt("Motore ripreso", "Engine resumed"),
        lt(
            "Il motore è tornato a controllare le utenze.",
            "The engine is managing the accounts again.",
        ),
    )
}

pub(super) fn relogin(name: &str) -> Texts {
    (
        lt(
            format!("{name}: login da rifare"),
            format!("{name}: login required"),
        ),
        lt(
            "Rifai /login in Claude Code e poi cswap add.",
            "Run /login in Claude Code, then cswap add.",
        ),
    )
}

pub(super) fn login_expiring(name: &str, left_secs: i64, at: &str) -> Texts {
    if left_secs <= 0 {
        return relogin(name);
    }
    let days = left_secs / 86_400;
    let (it, en) = if days >= 1 {
        (format!("tra {days} g"), format!("in {days} d"))
    } else {
        let h = (left_secs / 3600).max(1);
        (format!("tra {h} h"), format!("in {h} h"))
    };
    (
        lt(
            format!("{name}: login in scadenza {it}"),
            format!("{name}: login expires {en}"),
        ),
        lt(
            format!("Il login scade il {at}: rifallo prima per non restare senza utenza."),
            format!("The login expires on {at}: renew it to keep the account usable."),
        ),
    )
}

pub(super) fn stale(name: &str, minutes: i64) -> Texts {
    (
        lt(
            format!("{name}: dati non aggiornati"),
            format!("{name}: data not updated"),
        ),
        lt(
            format!("Nessun consumo aggiornato da {minutes} min."),
            format!("No fresh usage for {minutes} min."),
        ),
    )
}

pub(super) fn update(installed: Option<&str>, latest: &str) -> Texts {
    let (it, en) = installed.map_or_else(
        || (String::new(), String::new()),
        |i| (format!(" (installata {i})"), format!(" (installed {i})")),
    );
    (
        lt(
            "Aggiornamento di cswap disponibile",
            "cswap update available",
        ),
        lt(
            format!("È uscita la versione {latest}{it}."),
            format!("Version {latest} is out{en}."),
        ),
    )
}

/// Riepilogo: righe `(nome, punti 5h, punti 7d)` e numero di cambi.
pub(super) fn summary(rows: &[(String, f64, f64)], switches: usize) -> Texts {
    let (mut it, mut en) = (Vec::new(), Vec::new());
    for (name, five, seven) in rows {
        it.push(format!("{name} 5h +{}, 7d +{}", num(*five), num(*seven)));
        en.push(format!("{name} 5h +{}, 7d +{}", num(*five), num(*seven)));
    }
    if it.is_empty() {
        it.push("nessun consumo".into());
        en.push("no usage".into());
    }
    (
        lt("Riepilogo di oggi", "Today's summary"),
        lt(
            format!("{}. Cambi: {switches}.", it.join("; ")),
            format!("{}. Switches: {switches}.", en.join("; ")),
        ),
    )
}
