//! Riepilogo per la barra di stato delle estensioni (VS Code, JetBrains): `swapdeck-cli ide
//! status`. Dallo stato letto (lista di cswap, motore) dà dati già pronti da mostrare e i livelli
//! di colore; i testi restano agli host, che li traducono.
//!
//! Il caricamento (nessuno stato ancora letto) non passa di qui: è dell'host, che non ha ancora
//! niente da chiedere.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cswap::{Account, CswapError, ListResponse, UsageWindow};
use crate::timeutil::parse_iso;

/// Soglie di default dei colori, allineate a quelle delle notifiche (80% e 95%).
pub const DEFAULT_WARN: f64 = 80.0;
pub const DEFAULT_DANGER: f64 = 95.0;
/// Lunghezza massima dell'email accorciata nel tooltip.
pub const SHORT_EMAIL_MAX: usize = 24;

/// Livello di colore della barra di stato.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS,
)]
#[serde(rename_all = "camelCase")]
pub enum IdeLevel {
    /// Nessun dato da colorare.
    #[default]
    Unknown,
    Ok,
    Warn,
    Danger,
    /// Lettura fallita (cswap assente o in errore).
    Error,
}

/// Soglie dei colori: dalla più bassa (avviso) alla più alta (pericolo).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ColorThresholds {
    pub warn: f64,
    pub danger: f64,
}

impl Default for ColorThresholds {
    fn default() -> Self {
        Self {
            warn: DEFAULT_WARN,
            danger: DEFAULT_DANGER,
        }
    }
}

impl ColorThresholds {
    /// Dalle soglie delle notifiche: la più bassa avvisa, la più alta è critica. Si ignorano i
    /// valori fuori da (0, 100] o non finiti; senza valori validi valgono i default.
    pub fn from_values(values: &[f64]) -> Self {
        let valid: Vec<f64> = values
            .iter()
            .copied()
            .filter(|v| v.is_finite() && *v > 0.0 && *v <= 100.0)
            .collect();
        if valid.is_empty() {
            return Self::default();
        }
        Self {
            warn: valid.iter().copied().fold(f64::INFINITY, f64::min),
            danger: valid.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        }
    }

    /// Livello di una percentuale di consumo (`unknown` se manca).
    pub fn level(&self, pct: Option<f64>) -> IdeLevel {
        match pct {
            Some(p) if p.is_finite() && p >= self.danger => IdeLevel::Danger,
            Some(p) if p.is_finite() && p >= self.warn => IdeLevel::Warn,
            Some(p) if p.is_finite() => IdeLevel::Ok,
            _ => IdeLevel::Unknown,
        }
    }
}

/// Percentuale limitata a 0..100 e arrotondata all'intero, come si mostra.
pub fn display_pct(pct: f64) -> u32 {
    if !pct.is_finite() {
        return 0;
    }
    // Limitata a 0..100: la conversione non tronca.
    pct.clamp(0.0, 100.0).round() as u32
}

/// Accorcia un'email tenendo il dominio: `lungonomeutente@example.com` → `lungonome…@example.com`.
/// Senza `@` tronca il testo. Conta i caratteri, non i byte.
pub fn short_email(email: &str, max: usize) -> String {
    let chars: Vec<char> = email.chars().collect();
    if chars.len() <= max {
        return email.to_owned();
    }
    let take = |v: &[char], n: usize| v.iter().take(n).collect::<String>();
    let Some(at) = chars.iter().rposition(|c| *c == '@').filter(|at| *at > 0) else {
        return format!("{}…", take(&chars, max.saturating_sub(1).max(1)));
    };
    let (local, domain) = chars.split_at(at);
    let room = max as isize - domain.len() as isize - 1;
    if room >= 3 {
        return format!(
            "{}…{}",
            take(local, room as usize),
            domain.iter().collect::<String>()
        );
    }
    format!(
        "{}…{}…",
        take(local, 3),
        take(domain, max.saturating_sub(4).max(1))
    )
}

/// Chi tiene il lock del motore rispetto all'host che chiede.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum EngineHolder {
    /// Il motore avviato da questo host (il PID del lock è il suo).
    Here,
    /// Un'altra istanza: altra finestra, altra IDE o l'app.
    Other,
    /// Nessun motore vivo.
    Off,
}

/// Il motore com'è nello stato letto (`EngineStatus` della CLI).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EngineFacts {
    pub running: bool,
    /// `engine.enabled` delle regole: cambi automatici accesi.
    pub enabled: bool,
    pub dry_run: bool,
    pub lock_pid: Option<u32>,
    pub active_map: Option<String>,
}

/// Riga del motore nel tooltip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IdeEngine {
    pub holder: EngineHolder,
    /// PID del titolare del lock, se noto (`null` con `off`).
    pub pid: Option<u32>,
    /// Cambi automatici accesi (`engine.enabled`).
    pub switching: bool,
    /// Solo osservazione.
    pub dry_run: bool,
    /// Nome della mappa attiva.
    pub active_map: Option<String>,
}

impl IdeEngine {
    /// Il titolare è il motore di questo host se il PID del lock è `own_pid`. Il testo della
    /// barra è uguale in tutte le finestre; cambia solo la riga del motore.
    pub fn new(facts: &EngineFacts, own_pid: Option<u32>) -> Self {
        let (holder, pid) = if !facts.running {
            (EngineHolder::Off, None)
        } else if own_pid.is_some() && facts.lock_pid == own_pid {
            (EngineHolder::Here, facts.lock_pid)
        } else {
            (EngineHolder::Other, facts.lock_pid)
        };
        Self {
            holder,
            pid,
            switching: facts.enabled,
            dry_run: facts.dry_run,
            active_map: facts.active_map.clone(),
        }
    }
}

/// Account attivo: nome da mostrare (alias, altrimenti numero) ed email accorciata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IdeAccount {
    pub number: u32,
    /// Alias non vuoto; senza, l'host mostra il numero (`#6`).
    pub alias: Option<String>,
    pub short_email: String,
}

/// Una finestra di consumo (5 ore o 7 giorni) come si mostra.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IdeWindow {
    /// Percentuale arrotondata e limitata a 0..100; `null` se manca.
    pub pct: Option<u32>,
    pub level: IdeLevel,
    /// Tempo al reset come lo scrive cswap (`1h 44m`).
    pub countdown: Option<String>,
    pub resets_at: Option<String>,
    /// Secondi al reset da adesso (mai negativi), se `resets_at` è valido.
    #[ts(type = "number | null")]
    pub resets_in_secs: Option<i64>,
    /// Consumo atteso al ritmo lineare (solo 7 giorni), arrotondato come `pct`.
    pub expected_pct: Option<u32>,
}

impl IdeWindow {
    fn new(window: Option<&UsageWindow>, thresholds: &ColorThresholds, now: i64) -> Self {
        let Some(w) = window else {
            return Self::default();
        };
        Self {
            pct: Some(display_pct(w.pct)),
            level: thresholds.level(Some(w.pct)),
            countdown: w.countdown.clone(),
            resets_at: w.resets_at.clone(),
            resets_in_secs: w
                .resets_at
                .as_deref()
                .and_then(parse_iso)
                .map(|t| (t - now).max(0)),
            expected_pct: w.expected_pct.map(display_pct),
        }
    }
}

/// Cosa mostra la barra.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum IdeContent {
    /// cswap non disponibile o lettura fallita; `error` assente = errore sconosciuto.
    Error { error: Option<CswapError> },
    /// Nessun account attivo.
    NoAccount,
    Account {
        account: IdeAccount,
        five_hour: IdeWindow,
        seven_day: IdeWindow,
        /// Percentuali dall'ultimo dato noto (consumo attuale non disponibile).
        last_known: bool,
    },
}

/// `swapdeck-cli ide status`: tutto ciò che serve per testo, colore e tooltip della barra.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IdeStatus {
    /// Colore: il peggiore tra 5h e 7d; `error` se la lettura è fallita; `unknown` senza dati.
    pub level: IdeLevel,
    pub content: IdeContent,
    pub engine: IdeEngine,
    /// Istante della lettura, in ms Unix.
    #[ts(type = "number")]
    pub updated_at_ms: u64,
    /// Soglie usate per i livelli.
    pub thresholds: ColorThresholds,
}

/// Stato letto da riassumere (`CliState` della CLI).
#[derive(Debug, Clone, Copy)]
pub struct StatusInput<'a> {
    pub list: Option<&'a ListResponse>,
    pub error: Option<&'a CswapError>,
    pub updated_at_ms: u64,
    pub engine: &'a EngineFacts,
}

/// Il consumo da mostrare: quello attuale, altrimenti l'ultimo dato noto (`true`).
fn shown_usage(account: &Account) -> Option<(&crate::cswap::Usage, bool)> {
    account
        .usage
        .as_ref()
        .map(|u| (u, false))
        .or_else(|| account.last_good_usage.as_ref().map(|u| (u, true)))
}

/// Riepilogo della barra di stato. `own_engine_pid` è il PID del motore avviato dall'host che
/// chiede (se ce l'ha); `now` in secondi Unix.
pub fn status(
    input: StatusInput<'_>,
    thresholds: ColorThresholds,
    own_engine_pid: Option<u32>,
    now: i64,
) -> IdeStatus {
    let engine = IdeEngine::new(input.engine, own_engine_pid);
    let (level, content) = match input.list {
        None => (
            IdeLevel::Error,
            IdeContent::Error {
                error: input.error.cloned(),
            },
        ),
        Some(list) => match list.accounts.iter().find(|a| a.active) {
            None => (IdeLevel::Unknown, IdeContent::NoAccount),
            Some(active) => {
                let shown = shown_usage(active);
                let usage = shown.map(|(u, _)| u);
                let five_hour =
                    IdeWindow::new(usage.and_then(|u| u.five_hour.as_ref()), &thresholds, now);
                let seven_day =
                    IdeWindow::new(usage.and_then(|u| u.seven_day.as_ref()), &thresholds, now);
                let level = five_hour.level.max(seven_day.level);
                let account = IdeAccount {
                    number: active.number,
                    alias: active.alias.clone().filter(|a| !a.is_empty()),
                    short_email: short_email(&active.email, SHORT_EMAIL_MAX),
                };
                (
                    level,
                    IdeContent::Account {
                        account,
                        five_hour,
                        seven_day,
                        last_known: shown.is_some_and(|(_, last)| last),
                    },
                )
            }
        },
    };
    IdeStatus {
        level,
        content,
        engine,
        updated_at_ms: input.updated_at_ms,
        thresholds,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cswap::{CswapErrorKind, Usage};

    const NOW: i64 = 1_760_000_000;

    fn fixture() -> ListResponse {
        serde_json::from_str(include_str!("../../../fixtures/cswap/list.json")).unwrap()
    }

    fn with_active_usage(mut list: ListResponse, five: f64, seven: f64) -> ListResponse {
        let active = list.accounts.iter_mut().find(|a| a.active).unwrap();
        active.usage = Some(Usage {
            five_hour: Some(UsageWindow {
                pct: five,
                ..UsageWindow::default()
            }),
            seven_day: Some(UsageWindow {
                pct: seven,
                ..UsageWindow::default()
            }),
            scoped: None,
            spend: None,
        });
        list
    }

    fn summary(list: Option<&ListResponse>, engine: &EngineFacts, own: Option<u32>) -> IdeStatus {
        let input = StatusInput {
            list,
            error: None,
            updated_at_ms: 42,
            engine,
        };
        status(input, ColorThresholds::default(), own, NOW)
    }

    fn off() -> EngineFacts {
        EngineFacts::default()
    }

    #[test]
    fn active_account_with_rounded_percentages_and_tooltip_data() {
        let list = fixture();
        let s = summary(Some(&list), &off(), None);
        assert_eq!(s.level, IdeLevel::Ok);
        assert_eq!(s.updated_at_ms, 42);
        let IdeContent::Account {
            account,
            five_hour,
            seven_day,
            last_known,
        } = s.content
        else {
            panic!("account atteso: {:?}", s.content);
        };
        assert_eq!(account.number, 6);
        assert_eq!(account.alias.as_deref(), Some("alias6"));
        assert_eq!(account.short_email, "user6@example.com");
        assert!(!last_known);
        assert_eq!(five_hour.pct, Some(66));
        assert_eq!(five_hour.countdown.as_deref(), Some("1h 44m"));
        assert_eq!(five_hour.expected_pct, None);
        assert_eq!(seven_day.pct, Some(77));
        assert_eq!(seven_day.expected_pct, Some(26));
        assert!(seven_day.resets_in_secs.is_some_and(|s| s > 0));
        assert_eq!(s.engine.holder, EngineHolder::Off);
        assert!(!s.engine.switching);
    }

    #[test]
    fn json_shape_for_the_hosts() {
        let list = fixture();
        let v = serde_json::to_value(summary(Some(&list), &off(), None)).unwrap();
        assert_eq!(v["level"], "ok");
        assert_eq!(v["content"]["kind"], "account");
        assert_eq!(v["content"]["fiveHour"]["pct"], 66);
        assert_eq!(v["content"]["sevenDay"]["expectedPct"], 26);
        assert_eq!(v["content"]["lastKnown"], false);
        assert_eq!(v["content"]["account"]["shortEmail"], "user6@example.com");
        assert_eq!(v["engine"]["holder"], "off");
        assert_eq!(v["updatedAtMs"], 42);
        assert_eq!(v["thresholds"]["warn"], 80.0);
        let none = serde_json::to_value(IdeContent::NoAccount).unwrap();
        assert_eq!(none, serde_json::json!({"kind": "noAccount"}));
    }

    #[test]
    fn level_is_the_worse_of_five_hour_and_seven_day() {
        for (five, seven, level) in [
            (10.0, 85.0, IdeLevel::Warn),
            (96.0, 10.0, IdeLevel::Danger),
            (85.0, 96.0, IdeLevel::Danger),
            (79.9, 0.0, IdeLevel::Ok),
            (80.0, 0.0, IdeLevel::Warn),
            (95.0, 0.0, IdeLevel::Danger),
        ] {
            let list = with_active_usage(fixture(), five, seven);
            assert_eq!(
                summary(Some(&list), &off(), None).level,
                level,
                "{five}/{seven}"
            );
        }
    }

    #[test]
    fn level_uses_raw_percentages_and_display_rounds() {
        // 79.6 si mostra 80 ma resta sotto la soglia di avviso, come prima in VS Code.
        let list = with_active_usage(fixture(), 79.6, 120.0);
        let s = summary(Some(&list), &off(), None);
        let IdeContent::Account {
            five_hour,
            seven_day,
            ..
        } = &s.content
        else {
            panic!()
        };
        assert_eq!((five_hour.pct, five_hour.level), (Some(80), IdeLevel::Ok));
        assert_eq!(
            (seven_day.pct, seven_day.level),
            (Some(100), IdeLevel::Danger)
        );
        assert_eq!(display_pct(-3.0), 0);
        assert_eq!(display_pct(0.5), 1);
        assert_eq!(display_pct(f64::NAN), 0);
    }

    #[test]
    fn last_known_data_and_number_without_alias() {
        let mut list = fixture();
        let active = list.accounts.iter_mut().find(|a| a.active).unwrap();
        active.last_good_usage = active.usage.take();
        active.alias = Some(String::new());
        let s = summary(Some(&list), &off(), None);
        let IdeContent::Account {
            account,
            five_hour,
            last_known,
            ..
        } = s.content
        else {
            panic!()
        };
        assert!(last_known);
        assert_eq!(account.alias, None, "alias vuoto = numero");
        assert_eq!(five_hour.pct, Some(66));
    }

    #[test]
    fn no_usage_at_all_is_unknown() {
        let mut list = fixture();
        let active = list.accounts.iter_mut().find(|a| a.active).unwrap();
        active.usage = None;
        active.last_good_usage = None;
        let s = summary(Some(&list), &off(), None);
        assert_eq!(s.level, IdeLevel::Unknown);
        let IdeContent::Account {
            five_hour,
            seven_day,
            last_known,
            ..
        } = s.content
        else {
            panic!()
        };
        assert!(!last_known);
        assert_eq!(five_hour, IdeWindow::default());
        assert_eq!(seven_day.pct, None);
    }

    #[test]
    fn error_and_no_active_account() {
        let error = CswapError::new(CswapErrorKind::NotInstalled, "cswap non trovato");
        let engine = EngineFacts {
            running: true,
            lock_pid: Some(42),
            ..off()
        };
        let input = StatusInput {
            list: None,
            error: Some(&error),
            updated_at_ms: 1,
            engine: &engine,
        };
        let s = status(input, ColorThresholds::default(), None, NOW);
        assert_eq!(s.level, IdeLevel::Error);
        assert_eq!(
            s.content,
            IdeContent::Error {
                error: Some(error.clone())
            }
        );
        assert_eq!(
            (s.engine.holder, s.engine.pid),
            (EngineHolder::Other, Some(42))
        );

        let mut none = fixture();
        for a in &mut none.accounts {
            a.active = false;
        }
        let s = summary(Some(&none), &off(), None);
        assert_eq!(
            (s.level, s.content),
            (IdeLevel::Unknown, IdeContent::NoAccount)
        );
    }

    #[test]
    fn engine_holder_by_lock_pid() {
        let list = fixture();
        let running = EngineFacts {
            running: true,
            enabled: true,
            dry_run: false,
            lock_pid: Some(4242),
            active_map: Some("Eco".into()),
        };
        let here = summary(Some(&list), &running, Some(4242));
        assert_eq!(
            (here.engine.holder, here.engine.pid),
            (EngineHolder::Here, Some(4242))
        );
        assert!(here.engine.switching);
        assert_eq!(here.engine.active_map.as_deref(), Some("Eco"));
        // Processo figlio vivo ma lock di un'altra finestra: questa osserva.
        let other = summary(Some(&list), &running, Some(99));
        assert_eq!(
            (other.engine.holder, other.engine.pid),
            (EngineHolder::Other, Some(4242))
        );
        assert_eq!(
            summary(Some(&list), &running, None).engine.holder,
            EngineHolder::Other
        );
        let unknown_pid = EngineFacts {
            lock_pid: None,
            ..running.clone()
        };
        let s = summary(Some(&list), &unknown_pid, None);
        assert_eq!((s.engine.holder, s.engine.pid), (EngineHolder::Other, None));
        let stopped = EngineFacts {
            running: false,
            ..running.clone()
        };
        let s = summary(Some(&list), &stopped, Some(4242));
        assert_eq!((s.engine.holder, s.engine.pid), (EngineHolder::Off, None));
        // Il resto è uguale in ogni finestra.
        assert_eq!(here.content, other.content);
        assert_eq!(here.level, other.level);
    }

    #[test]
    fn color_thresholds_from_notification_thresholds() {
        let t = |v: &[f64]| {
            let c = ColorThresholds::from_values(v);
            (c.warn, c.danger)
        };
        assert_eq!(t(&[95.0, 70.0, 85.0]), (70.0, 95.0));
        assert_eq!(t(&[90.0]), (90.0, 90.0));
        assert_eq!(t(&[]), (80.0, 95.0));
        assert_eq!(t(&[0.0, 150.0, f64::NAN, -5.0]), (80.0, 95.0));
        assert_eq!(t(&[100.0, 0.5]), (0.5, 100.0));
        let single = ColorThresholds::from_values(&[90.0]);
        assert_eq!(single.level(Some(90.0)), IdeLevel::Danger);
        assert_eq!(single.level(Some(89.9)), IdeLevel::Ok);
        assert_eq!(single.level(None), IdeLevel::Unknown);
    }

    #[test]
    fn short_email_keeps_the_domain() {
        assert_eq!(short_email("a@b.it", 24), "a@b.it");
        assert_eq!(
            short_email("lungonomeutente.davvero@example.com", 24),
            "lungonomeut…@example.com"
        );
        assert_eq!(short_email("senzachiocciolaetroppolungo", 10), "senzachio…");
        assert_eq!(
            short_email("nome@dominiomoltolungoperstare.example.com", 24),
            "nom…@dominiomoltolungope…"
        );
        assert_eq!(
            short_email("èèèèèèèèèè@example.com", 20),
            "èèèèèèè…@example.com"
        );
    }
}
