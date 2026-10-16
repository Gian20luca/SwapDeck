//! Test del modulo notify: tabelle per ogni avviso (quando scatta, quando no, quante volte,
//! di nuovo dopo il reset), non disturbare, attiva o tutte, riepilogo, memoria su file e
//! mittente unico.

use serde_json::json;

use super::*;
use crate::cswap::{ListResponse, UsageStatus};
use crate::engine::{IdleReason, SuspendReason};
use crate::history::{SwitchRecord, UsageSample};
use crate::setup::UpdateInfo;
use crate::timeutil::format_iso;

/// 2027-01-15T08:00:00Z.
const NOW: i64 = 1_800_000_000;
const MIN: i64 = 60;
const R5: i64 = NOW + 3600;
const R7: i64 = NOW + 3 * 86_400;

fn account(n: u32, p5: f64, p7: f64) -> Account {
    serde_json::from_value(json!({
        "number": n,
        "email": format!("a{n}@example.test"),
        "organizationName": "",
        "organizationUuid": "",
        "isOrganization": false,
        "active": false,
        "usageStatus": "ok",
        "alias": format!("acc{n}"),
        "usageAgeSeconds": 30.0,
        "usage": {
            "fiveHour": { "pct": p5, "resetsAt": format_iso(R5) },
            "sevenDay": { "pct": p7, "resetsAt": format_iso(R7) }
        }
    }))
    .unwrap()
}

fn list(active: Option<u32>, mut accounts: Vec<Account>) -> ListResponse {
    for a in &mut accounts {
        a.active = Some(a.number) == active;
    }
    serde_json::from_value(json!({
        "schemaVersion": 1,
        "activeAccountNumber": active,
        "accounts": accounts,
    }))
    .unwrap()
}

fn two(p5_active: f64, p5_other: f64) -> ListResponse {
    list(
        Some(1),
        vec![account(1, p5_active, 10.0), account(2, p5_other, 10.0)],
    )
}

fn engine_rules() -> EngineRules {
    EngineRules {
        enabled: true,
        primary: Some(1),
        five_hour_threshold: 90.0,
        seven_day_threshold: Some(95.0),
        ..EngineRules::default()
    }
}

/// Regole con solo gli avvisi indicati accesi (gli altri spenti), per tabelle mirate.
fn only(kinds: &[&str]) -> NotificationConfig {
    let mut r = NotificationConfig::default();
    let on = |k: &str| kinds.contains(&k);
    r.switch_preview.enabled = on("preview");
    r.switched.enabled = on("switched");
    r.returned_to_primary.enabled = on("returned");
    r.no_reserve.enabled = on("noReserve");
    r.percent.enabled = on("percent");
    r.exhaustion.enabled = on("exhaustion");
    r.reset_soon.enabled = on("resetSoon");
    r.reset_done.enabled = on("resetDone");
    r.model_limit.enabled = on("model");
    r.engine_state.enabled = on("engine");
    r.login.enabled = on("login");
    r.stale.enabled = on("stale");
    r.updates.enabled = on("updates");
    r
}

#[derive(Default)]
struct H {
    rules: NotificationConfig,
    mem: NotifyMemory,
}

impl H {
    fn new(rules: NotificationConfig) -> Self {
        Self {
            rules,
            mem: NotifyMemory::default(),
        }
    }

    fn input<'a>(&self, list: Option<&'a ListResponse>) -> NotifyInput<'a> {
        NotifyInput {
            list,
            ..NotifyInput::default()
        }
    }

    fn run(&mut self, list: &ListResponse, now: i64) -> Vec<Notification> {
        let input = self.input(Some(list));
        evaluate(&input, &self.rules, &mut self.mem, now)
    }

    fn with(&mut self, input: &NotifyInput<'_>, now: i64) -> Vec<Notification> {
        evaluate(input, &self.rules, &mut self.mem, now)
    }
}

fn keys(v: &[Notification]) -> Vec<String> {
    v.iter().map(|n| n.key.clone()).collect()
}

fn kinds(v: &[Notification]) -> Vec<NotificationKind> {
    v.iter().map(|n| n.kind).collect()
}

// ---- soglie percentuali ------------------------------------------------------------------

#[test]
fn percent_thresholds_table() {
    // (5h attiva, avvisi attesi) in sequenza sulla stessa finestra.
    let steps: &[(f64, &[&str])] = &[
        (50.0, &[]),
        (81.0, &["80"]),
        (85.0, &[]),
        (82.0, &[]),
        (96.0, &["95"]),
        (99.0, &[]),
    ];
    let mut h = H::new(only(&["percent"]));
    for (i, (pct, expect)) in steps.iter().enumerate() {
        let out = h.run(&two(*pct, 0.0), NOW + i as i64 * MIN);
        let got: Vec<String> = out
            .iter()
            .map(|n| n.key.split(':').nth(3).unwrap().to_owned())
            .collect();
        assert_eq!(got, *expect, "passo {i} ({pct}%)");
    }
}

#[test]
fn jump_over_thresholds_notifies_only_the_highest_once() {
    let mut h = H::new(only(&["percent"]));
    let out = h.run(&two(97.0, 0.0), NOW);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Warning);
    assert!(out[0].key.contains(":95:"));
    assert!(h.run(&two(98.0, 0.0), NOW + MIN).is_empty());
}

#[test]
fn new_window_rearms_thresholds() {
    let mut h = H::new(only(&["percent"]));
    assert_eq!(h.run(&two(81.0, 0.0), NOW).len(), 1);
    // Reset: nuovo resetsAt.
    let mut l = two(85.0, 0.0);
    l.accounts[0]
        .usage
        .as_mut()
        .unwrap()
        .five_hour
        .as_mut()
        .unwrap()
        .resets_at = Some(format_iso(R5 + 5 * 3600));
    assert_eq!(h.run(&l, R5 + MIN).len(), 1);
    assert!(h.run(&l, R5 + 2 * MIN).is_empty());
}

#[test]
fn drop_without_reset_time_rearms() {
    let mut h = H::new(only(&["percent"]));
    let strip = |mut l: ListResponse| {
        for a in &mut l.accounts {
            a.usage
                .as_mut()
                .unwrap()
                .five_hour
                .as_mut()
                .unwrap()
                .resets_at = None;
        }
        l
    };
    assert_eq!(h.run(&strip(two(91.0, 0.0)), NOW).len(), 1);
    assert!(h.run(&strip(two(3.0, 0.0)), NOW + MIN).is_empty());
    assert_eq!(h.run(&strip(two(91.0, 0.0)), NOW + 2 * MIN).len(), 1);
}

#[test]
fn seven_day_has_its_own_thresholds() {
    let mut rules = only(&["percent"]);
    rules.percent.five_hour = vec![50.0];
    rules.percent.seven_day = vec![30.0];
    let mut h = H::new(rules);
    let l = list(Some(1), vec![account(1, 40.0, 35.0)]);
    let out = h.run(&l, NOW);
    assert_eq!(
        keys(&out),
        [format!("pct:1:7d:30:{}", &format_iso(R7)[..16])]
    );
}

#[test]
fn scope_active_or_all() {
    let mut h = H::new(only(&["percent"]));
    assert!(h.run(&two(10.0, 90.0), NOW).is_empty(), "solo l'attiva");
    let mut rules = only(&["percent"]);
    rules.percent.scope = NotifyScope::All;
    let mut h = H::new(rules);
    let out = h.run(&two(10.0, 90.0), NOW);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].account, Some(2));
}

#[test]
fn disabled_rule_is_silent() {
    let mut h = H::new(only(&[]));
    assert!(h.run(&two(99.0, 99.0), NOW).is_empty());
}

// ---- limite per modello e esaurimento ------------------------------------------------------

fn with_model(pct: f64, exhaustion: Option<i64>) -> ListResponse {
    let mut a = account(1, 10.0, 10.0);
    let mut w: crate::cswap::UsageWindow = serde_json::from_value(json!({
        "name": "Fable", "pct": pct, "resetsAt": format_iso(R7)
    }))
    .unwrap();
    w.projected_exhaustion_at = exhaustion.map(format_iso);
    a.usage.as_mut().unwrap().scoped = Some(vec![w]);
    list(Some(1), vec![a])
}

#[test]
fn model_limit_table() {
    let mut rules = only(&["model"]);
    rules.model_limit.thresholds = vec![70.0, 90.0];
    let mut h = H::new(rules);
    let cases: &[(f64, usize)] = &[(60.0, 0), (71.0, 1), (75.0, 0), (91.0, 1), (95.0, 0)];
    for (i, (pct, n)) in cases.iter().enumerate() {
        let out = h.run(&with_model(*pct, None), NOW + i as i64 * MIN);
        assert_eq!(out.len(), *n, "{pct}%");
        assert!(out.iter().all(|x| x.kind == NotificationKind::ModelLimit));
        assert!(out.iter().all(|x| x.title.it.contains("modello Fable")));
    }
}

#[test]
fn exhaustion_once_per_window_and_only_before_reset() {
    let mut h = H::new(only(&["exhaustion"]));
    // Esaurimento dopo il reset: niente.
    assert!(h.run(&with_model(50.0, Some(R7 + 10)), NOW).is_empty());
    let mut l = list(Some(1), vec![account(1, 10.0, 60.0)]);
    l.accounts[0]
        .usage
        .as_mut()
        .unwrap()
        .seven_day
        .as_mut()
        .unwrap()
        .projected_exhaustion_at = Some(format_iso(NOW + 86_400));
    let out = h.run(&l, NOW);
    assert_eq!(kinds(&out), [NotificationKind::Exhaustion]);
    assert!(h.run(&l, NOW + MIN).is_empty());
}

// ---- reset imminente e avvenuto -------------------------------------------------------------

#[test]
fn reset_soon_and_done() {
    let mut rules = only(&["resetSoon", "resetDone"]);
    rules.reset_soon.minutes_before = 15;
    let mut h = H::new(rules);
    let l = two(40.0, 0.0);
    assert!(h.run(&l, R5 - 20 * MIN).is_empty());
    let out = h.run(&l, R5 - 10 * MIN);
    assert_eq!(kinds(&out), [NotificationKind::ResetSoon]);
    assert!(out[0].title.it.contains("tra 10 min"));
    assert!(h.run(&l, R5 - 5 * MIN).is_empty());
    // Dopo il reset (cswap mostra già la finestra nuova): reset avvenuto una volta.
    let mut next = two(1.0, 0.0);
    next.accounts[0]
        .usage
        .as_mut()
        .unwrap()
        .five_hour
        .as_mut()
        .unwrap()
        .resets_at = Some(format_iso(R5 + 5 * 3600));
    let out = h.run(&next, R5 + MIN);
    assert_eq!(kinds(&out), [NotificationKind::ResetDone]);
    assert!(h.run(&next, R5 + 2 * MIN).is_empty());
}

#[test]
fn reset_done_after_long_pause_is_skipped() {
    let mut h = H::new(only(&["resetDone"]));
    h.run(&two(40.0, 0.0), NOW);
    assert!(h.run(&two(40.0, 0.0), R5 + 2 * 3600).is_empty());
}

// ---- cambi e ritorno -----------------------------------------------------------------------

#[test]
fn switch_table() {
    let mut h = H::new(only(&["switched", "returned"]));
    let rules = engine_rules();
    // Prima lista: base, nessun avviso.
    assert!(h.run(&two(10.0, 0.0), NOW).is_empty());
    // Cambio esterno.
    let l2 = list(Some(2), vec![account(1, 10.0, 0.0), account(2, 0.0, 0.0)]);
    let out = h.run(&l2, NOW + MIN);
    assert_eq!(kinds(&out), [NotificationKind::Switched]);
    assert!(out[0].body.it.contains("manuale o esterno"));
    assert!(out[0].title.it.ends_with("acc2"));
    assert!(h.run(&l2, NOW + 2 * MIN).is_empty());
    // Cambio del motore con motivo e mappa (lista ancora quella di prima del cambio).
    let decision = Decision::ReturnToPrimary {
        from: 2,
        to: 1,
        cause: SwitchCause::PrimaryReset,
    };
    let ctx = EngineContext {
        rules: &rules,
        decision: Some(&decision),
        switched: Some(SwitchDone {
            from: Some(2),
            to: 1,
            cause: SwitchCause::PrimaryReset,
        }),
        preview: None,
        profile: Some("Eco"),
    };
    let input = NotifyInput {
        list: Some(&l2),
        engine: Some(ctx),
        ..NotifyInput::default()
    };
    let out = h.with(&input, NOW + 3 * MIN);
    assert_eq!(kinds(&out), [NotificationKind::ReturnedToPrimary]);
    assert!(out[0].body.it.contains("Mappa: Eco"));
    // La lista dopo il cambio non ripete l'avviso.
    assert!(h.run(&two(10.0, 0.0), NOW + 4 * MIN).is_empty());
    // Motore: prima della lista al limite.
    let ctx = EngineContext {
        switched: Some(SwitchDone {
            from: Some(1),
            to: 2,
            cause: SwitchCause::PrimaryAtLimit,
        }),
        profile: None,
        ..ctx
    };
    let l1 = two(10.0, 0.0);
    let input = NotifyInput {
        list: Some(&l1),
        engine: Some(ctx),
        ..NotifyInput::default()
    };
    let out = h.with(&input, NOW + 5 * MIN);
    assert_eq!(kinds(&out), [NotificationKind::Switched]);
    assert!(out[0].body.en.contains("first account at limit"));
}

#[test]
fn switch_seen_after_long_pause_is_not_notified() {
    let mut h = H::new(only(&["switched"]));
    h.run(&two(10.0, 0.0), NOW);
    let l2 = list(Some(2), vec![account(1, 10.0, 0.0), account(2, 0.0, 0.0)]);
    assert!(h.run(&l2, NOW + 2 * 3600).is_empty());
}

// ---- motore: sospeso, ripreso, nessuna riserva, preavviso -----------------------------------

fn engine_input<'a>(
    l: &'a ListResponse,
    rules: &'a EngineRules,
    d: &'a Decision,
    preview: Option<&'a SwitchPreview>,
) -> NotifyInput<'a> {
    NotifyInput {
        list: Some(l),
        engine: Some(EngineContext {
            rules,
            decision: Some(d),
            switched: None,
            preview,
            profile: None,
        }),
        ..NotifyInput::default()
    }
}

fn susp(reason: SuspendReason) -> Decision {
    Decision::Suspended {
        reason,
        detail: Vec::new(),
    }
}

fn idle(reason: IdleReason) -> Decision {
    Decision::NoAction { reason }
}

#[test]
fn engine_suspended_resumed_and_no_reserve() {
    let mut h = H::new(only(&["engine", "noReserve"]));
    let rules = engine_rules();
    let l = two(10.0, 0.0);
    let ok = idle(IdleReason::BelowThreshold);
    let conflict = Decision::Suspended {
        reason: SuspendReason::Conflict,
        detail: vec!["cswap-guard".into()],
    };
    let none = susp(SuspendReason::NoViableTarget);
    let off = susp(SuspendReason::Disabled);
    let table: &[(&Decision, &[NotificationKind])] = &[
        (&ok, &[]),
        (&conflict, &[NotificationKind::EngineSuspended]),
        (&conflict, &[]),
        (&ok, &[NotificationKind::EngineResumed]),
        (&none, &[NotificationKind::NoReserve]),
        (&none, &[]),
        (&ok, &[NotificationKind::EngineResumed]),
        (&none, &[NotificationKind::NoReserve]),
        (&off, &[]),
        (&ok, &[]),
    ];
    for (i, (d, expect)) in table.iter().enumerate() {
        let out = h.with(&engine_input(&l, &rules, d, None), NOW + i as i64 * MIN);
        assert_eq!(kinds(&out), *expect, "passo {i}");
        if let Some(n) = out.first() {
            if n.kind == NotificationKind::NoReserve {
                assert_eq!(n.severity, Severity::Critical);
                assert!(n.body.it.contains("Primo reset: acc1"), "{}", n.body.it);
            }
            if n.kind == NotificationKind::EngineSuspended {
                assert!(n.body.it.contains("cswap-guard"));
            }
        }
    }
}

#[test]
fn switch_preview_table() {
    let mut rules = only(&["preview"]);
    rules.switch_preview.points_before = 5.0;
    rules.switch_preview.minutes_before = 10;
    let er = engine_rules(); // soglia 5h 90
    let ok = idle(IdleReason::BelowThreshold);
    // 5h a 86%: 4 punti dalla soglia → preavviso per punti.
    let mut h = H::new(rules.clone());
    let l = two(86.0, 0.0);
    let out = h.with(&engine_input(&l, &er, &ok, None), NOW);
    assert_eq!(kinds(&out), [NotificationKind::SwitchPreview]);
    assert!(
        out[0].body.it.contains("un'altra utenza"),
        "{}",
        out[0].body.it
    );
    // Una volta per finestra.
    assert!(
        h.with(&engine_input(&l, &er, &ok, None), NOW + MIN)
            .is_empty()
    );

    // Ritmo: finestra a metà (2,5 h su 5) all'80%: 32%/h, 10 punti in circa 19 min.
    let mut rules_t = rules.clone();
    rules_t.switch_preview.points_before = 0.0;
    rules_t.switch_preview.minutes_before = 20;
    let mut h = H::new(rules_t);
    let mut l = two(80.0, 0.0);
    l.accounts[0]
        .usage
        .as_mut()
        .unwrap()
        .five_hour
        .as_mut()
        .unwrap()
        .resets_at = Some(format_iso(NOW + 9000));
    let p = SwitchPreview {
        target: Some(2),
        reason: None,
    };
    let out = h.with(&engine_input(&l, &er, &ok, Some(&p)), NOW);
    assert_eq!(kinds(&out), [NotificationKind::SwitchPreview]);
    assert!(
        out[0].body.it.starts_with("Tra circa 19 min passo a acc2"),
        "{}",
        out[0].body.it
    );

    // Lontano dalla soglia, motore spento, in osservazione o già in cambio: niente.
    let far = two(50.0, 0.0);
    let mut off = er.clone();
    off.enabled = false;
    let mut dry = er.clone();
    dry.dry_run = true;
    let switching = Decision::Switch {
        from: 1,
        to: 2,
        cause: SwitchCause::PrimaryAtLimit,
    };
    for (i, (l, r, d)) in [
        (&far, &er, &ok),
        (&two(86.0, 0.0), &off, &ok),
        (&two(86.0, 0.0), &dry, &ok),
        (&two(86.0, 0.0), &er, &switching),
    ]
    .into_iter()
    .enumerate()
    {
        let mut h = H::new(rules.clone());
        assert!(
            h.with(&engine_input(l, r, d, None), NOW).is_empty(),
            "caso {i}"
        );
    }
}

// ---- login, dati vecchi, aggiornamenti -----------------------------------------------------

#[test]
fn login_table() {
    let mut h = H::new(only(&["login"]));
    let mut l = two(10.0, 0.0);
    l.accounts[1].login_expires_at = Some(format_iso(NOW + 5 * 86_400));
    assert!(h.run(&l, NOW).is_empty(), "oltre i 3 giorni");
    l.accounts[1].login_expires_at = Some(format_iso(NOW + 2 * 86_400));
    let out = h.run(&l, NOW + MIN);
    assert_eq!(kinds(&out), [NotificationKind::Login]);
    assert_eq!(out[0].severity, Severity::Critical);
    assert_eq!(out[0].account, Some(2), "default: tutte le utenze");
    assert!(h.run(&l, NOW + 2 * MIN).is_empty());
    // Login da rifare: una volta finché dura, di nuovo se torna.
    l.accounts[0].usage_status = UsageStatus::ReloginRequired;
    l.accounts[0].usage = None;
    assert_eq!(h.run(&l, NOW + 3 * MIN).len(), 1);
    assert!(h.run(&l, NOW + 4 * MIN).is_empty());
    l.accounts[0].usage_status = UsageStatus::Ok;
    assert!(h.run(&l, NOW + 5 * MIN).is_empty());
    l.accounts[0].usage_status = UsageStatus::ReloginRequired;
    assert_eq!(h.run(&l, NOW + 6 * MIN).len(), 1);
}

#[test]
fn stale_data_after_minutes_and_on_failed_reads() {
    let mut h = H::new(only(&["stale"]));
    h.run(&two(10.0, 0.0), NOW);
    let none = NotifyInput::default();
    assert!(h.with(&none, NOW + 10 * MIN).is_empty());
    let out = h.with(&none, NOW + 16 * MIN);
    assert_eq!(kinds(&out), [NotificationKind::Stale]);
    assert_eq!(out[0].account, Some(1), "solo l'attiva");
    assert!(h.with(&none, NOW + 20 * MIN).is_empty());
    // Dati di nuovo freschi: la condizione finisce e si riarma.
    h.run(&two(10.0, 0.0), NOW + 21 * MIN);
    assert_eq!(h.with(&none, NOW + 40 * MIN).len(), 1);
}

#[test]
fn update_once_per_version_and_not_when_dismissed() {
    let mut h = H::new(only(&["updates"]));
    let mut u = UpdateInfo {
        installed: Some("0.27.2".into()),
        latest: "0.28.0".into(),
        update_available: true,
        dismissed: false,
        checked_at: 0,
    };
    let l = two(10.0, 0.0);
    let run = |u: &UpdateInfo, h: &mut H| {
        h.with(
            &NotifyInput {
                list: Some(&l),
                update: Some(u),
                ..NotifyInput::default()
            },
            NOW,
        )
    };
    assert_eq!(kinds(&run(&u, &mut h)), [NotificationKind::Update]);
    assert!(run(&u, &mut h).is_empty());
    u.latest = "0.29.0".into();
    u.dismissed = true;
    assert!(run(&u, &mut h).is_empty());
}

// ---- non disturbare ------------------------------------------------------------------------

#[test]
fn quiet_hours_drop_all_but_critical() {
    let mut rules = only(&["percent", "noReserve"]);
    rules.quiet_hours = QuietHours {
        enabled: true,
        start: "22:00".into(),
        end: "09:00".into(),
        allow_critical: true,
    };
    let er = engine_rules();
    let none = susp(SuspendReason::NoViableTarget);
    let l = two(99.0, 0.0);
    // NOW = 08:00 UTC, offset 0: dentro la fascia.
    let mut h = H::new(rules.clone());
    let out = h.with(&engine_input(&l, &er, &none, None), NOW);
    assert_eq!(kinds(&out), [NotificationKind::NoReserve]);
    // Il non critico è stato segnato: alla fine della fascia non arriva.
    assert!(
        h.with(&engine_input(&l, &er, &none, None), NOW + 2 * 3600)
            .is_empty()
    );
    // Senza eccezione per i critici: niente.
    rules.quiet_hours.allow_critical = false;
    let mut h = H::new(rules.clone());
    assert!(h.with(&engine_input(&l, &er, &none, None), NOW).is_empty());
    // Fuori dalla fascia (ora locale 10:00 con offset +2 h): tutto.
    rules.quiet_hours.allow_critical = true;
    let mut h = H::new(rules);
    let mut input = engine_input(&l, &er, &none, None);
    input.local_offset_secs = 7200;
    assert_eq!(h.with(&input, NOW).len(), 2);
}

#[test]
fn quiet_hours_same_day_range() {
    let mut rules = only(&["percent"]);
    rules.quiet_hours.enabled = true;
    rules.quiet_hours.start = "07:30".into();
    rules.quiet_hours.end = "08:30".into();
    let mut h = H::new(rules);
    assert!(h.run(&two(99.0, 0.0), NOW).is_empty());
}

// ---- riepilogo -----------------------------------------------------------------------------

fn sample(ts: i64, account: u32, p5: f64, p7: f64) -> HistoryRecord {
    HistoryRecord::Sample(UsageSample {
        ts,
        account,
        active: account == 1,
        status: UsageStatus::Ok,
        five_hour_pct: Some(p5),
        seven_day_pct: Some(p7),
        five_hour_resets_at: None,
        seven_day_resets_at: None,
        seven_day_expected_pct: None,
        fetched_at: None,
    })
}

#[test]
fn daily_summary_from_history() {
    let mut rules = only(&[]);
    rules.daily_summary.enabled = true;
    rules.daily_summary.time = "07:00".into();
    let day = NOW - 8 * 3600; // mezzanotte UTC
    let history = vec![
        sample(day + 3600, 1, 10.0, 20.0),
        sample(day + 7200, 1, 60.0, 25.0),
        // Reset della 5h: conta il valore nuovo.
        sample(day + 3 * 3600, 1, 5.0, 26.0),
        sample(day + 3600, 2, 0.0, 5.0),
        sample(day + 7200, 2, 0.0, 5.0),
        HistoryRecord::Switch(SwitchRecord {
            ts: day + 7300,
            from: Some(1),
            to: 2,
            cause: SwitchCause::PrimaryAtLimit,
            switched: true,
            message: String::new(),
        }),
        // Del giorno prima: escluso.
        sample(day - 60, 1, 0.0, 0.0),
    ];
    let mut h = H::new(rules.clone());
    let l = two(10.0, 0.0);
    assert_eq!(
        summary_state(&rules, &h.mem, NOW, 0),
        SummaryState::Due { from: day, to: NOW }
    );
    let input = NotifyInput {
        list: Some(&l),
        history: Some(&history),
        ..NotifyInput::default()
    };
    let out = h.with(&input, NOW);
    assert_eq!(kinds(&out), [NotificationKind::DailySummary]);
    assert_eq!(out[0].body.it, "acc1 5h +55, 7d +6. Cambi: 1.");
    assert!(h.with(&input, NOW + MIN).is_empty(), "una volta al giorno");
    assert_eq!(
        summary_state(&rules, &h.mem, NOW + MIN, 0),
        SummaryState::NotDue
    );
    // Prima dell'ora: non dovuto. Troppo dopo: saltato.
    let fresh = NotifyMemory::default();
    assert_eq!(
        summary_state(&rules, &fresh, day + 6 * 3600, 0),
        SummaryState::NotDue
    );
    assert_eq!(
        summary_state(&rules, &fresh, day + 12 * 3600, 0),
        SummaryState::Missed
    );
}

// ---- memoria su file, config, mittente unico ------------------------------------------------

#[test]
fn restart_does_not_repeat_within_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let rules = only(&["percent"]);
    let mut first = Notifier::new(Some(dir.path()));
    let mut never = || -> Option<String> { panic!("versione chiesta senza cache") };
    let l = two(85.0, 0.0);
    assert_eq!(first.tick(Some(&l), None, &rules, NOW, &mut never).len(), 1);
    drop(first);
    let mut second = Notifier::new(Some(dir.path()));
    assert!(
        second
            .tick(Some(&l), None, &rules, NOW + MIN, &mut never)
            .is_empty()
    );
    // Memoria rovinata: si riparte senza errori.
    std::fs::write(dir.path().join(MEMORY_FILE), b"{not json").unwrap();
    let mut third = Notifier::new(Some(dir.path()));
    assert_eq!(
        third
            .tick(Some(&l), None, &rules, NOW + 2 * MIN, &mut never)
            .len(),
        1
    );
}

#[test]
fn installed_version_is_read_only_with_a_cached_latest() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(crate::setup::update::CACHE_FILE),
        br#"{"checkedAt":1,"latest":"0.99.0"}"#,
    )
    .unwrap();
    let rules = only(&["updates"]);
    let mut calls = 0;
    let mut installed = || {
        calls += 1;
        Some("0.27.2".to_owned())
    };
    let mut n = Notifier::new(Some(dir.path()));
    let out = n.tick(None, None, &rules, NOW, &mut installed);
    assert_eq!(kinds(&out), [NotificationKind::Update]);
    assert!(
        n.tick(None, None, &rules, NOW + MIN, &mut installed)
            .is_empty()
    );
    assert_eq!(calls, 1, "versione installata tenuta per 6 ore");
}

#[test]
fn config_migrates_old_thresholds_and_keeps_legacy_copy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, br#"{"notifications":{"thresholds":[70,90]}}"#).unwrap();
    let loaded = crate::config::load(&path).unwrap();
    let n = &loaded.config.notifications;
    assert_eq!(n.percent.five_hour, [70.0, 90.0]);
    assert_eq!(n.percent.seven_day, [70.0, 90.0]);
    assert!(loaded.warnings.iter().any(|w| w.contains("migrate")));
    assert_eq!(n.switch_preview, SwitchPreviewRule::default());
    // Salvata: `thresholds` resta per le versioni precedenti, `percent` decide.
    let mut c = loaded.config.clone();
    c.patch_notifications(&json!({"percent": {"fiveHour": [60]}, "quietHours": {"enabled": true}}))
        .unwrap();
    crate::config::save(&path, &c).unwrap();
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(raw["notifications"]["thresholds"], json!([60.0]));
    assert_eq!(
        raw["notifications"]["percent"]["sevenDay"],
        json!([70.0, 90.0])
    );
    let again = crate::config::load(&path).unwrap().config;
    assert!(again.notifications.quiet_hours.enabled);
    assert_eq!(again.preferences().notification_thresholds, [60.0]);
    // Preferenze della versione 1: valgono per 5h e 7d.
    let mut c = again;
    c.apply_preferences(&crate::config::PreferencesPatch {
        notification_thresholds: Some(vec![50.0, 75.0]),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(c.notifications.percent.seven_day, [50.0, 75.0]);
}

#[test]
fn config_unknown_keys_survive_and_invalid_values_are_rejected() {
    let mut c = crate::config::AppConfig::default();
    c.patch_notifications(&json!({"futureRule": {"x": 1}}))
        .unwrap();
    let v = serde_json::to_value(&c).unwrap();
    assert_eq!(v["notifications"]["futureRule"]["x"], json!(1));
    for bad in [
        json!({"percent": {"fiveHour": [90, 80]}}),
        json!({"quietHours": {"start": "25:00"}}),
        json!({"login": {"daysBefore": 0}}),
        json!({"switchPreview": {"pointsBefore": -1}}),
        json!({"stale": {"minutes": 0}}),
        json!({"modelLimit": {"thresholds": [0]}}),
        json!([1]),
    ] {
        let before = c.clone();
        assert!(c.patch_notifications(&bad).is_err(), "{bad}");
        assert_eq!(c, before);
    }
    // Valori scritti a mano fuori dai limiti: riportati in lettura.
    let mut n: NotificationConfig = serde_json::from_value(json!({
        "percent": {"fiveHour": [95, 80, 80, 200]},
        "stale": {"minutes": 99999},
        "dailySummary": {"time": "xx"}
    }))
    .unwrap();
    let w = n.normalize();
    assert_eq!(n.percent.five_hour, [80.0, 95.0]);
    assert_eq!(n.thresholds, [80.0, 95.0]);
    assert_eq!(n.stale.minutes, 1440);
    assert_eq!(n.daily_summary.time, "20:00");
    assert_eq!(w.len(), 3);
    assert!(n.problems().is_empty());
}

#[test]
fn single_sender_poll() {
    let dir = tempfile::tempdir().unwrap();
    let rules = only(&["percent"]);
    let l = two(85.0, 0.0);
    let mut none = || None;
    let now = crate::timeutil::now_unix();
    // Nessun motore: manda chi chiede, una volta sola.
    let p = poll(
        dir.path(),
        Some(&l),
        &rules,
        crate::config::Language::It,
        now,
        &mut none,
    )
    .unwrap();
    assert_eq!(p.sender, NotifySender::Caller);
    assert_eq!(p.notifications.len(), 1);
    let p = poll(
        dir.path(),
        Some(&l),
        &rules,
        crate::config::Language::It,
        now,
        &mut none,
    )
    .unwrap();
    assert!(p.notifications.is_empty());
    // Un'altra richiesta in corso: niente.
    let guard = PollGuard::acquire(dir.path()).unwrap().unwrap();
    let busy = two(99.0, 0.0);
    let p = poll(
        dir.path(),
        Some(&busy),
        &rules,
        crate::config::Language::En,
        now,
        &mut none,
    )
    .unwrap();
    assert_eq!(p.sender, NotifySender::Busy);
    drop(guard);
    assert!(!dir.path().join(POLL_LOCK_FILE).exists());
    // Motore vivo (lock di questo processo): manda lui.
    let crate::lock::Acquire::Acquired(lock) =
        crate::lock::try_acquire(dir.path(), 60, now).unwrap()
    else {
        panic!("lock non preso");
    };
    let p = poll(
        dir.path(),
        Some(&busy),
        &rules,
        crate::config::Language::It,
        now,
        &mut none,
    )
    .unwrap();
    assert_eq!(p.sender, NotifySender::Engine);
    assert!(p.notifications.is_empty());
    lock.release();
    let p = poll(
        dir.path(),
        Some(&busy),
        &rules,
        crate::config::Language::It,
        now,
        &mut none,
    )
    .unwrap();
    assert_eq!(p.sender, NotifySender::Caller);
    assert_eq!(p.notifications.len(), 1, "95% nella stessa finestra");
}

#[test]
fn texts_never_contain_emails() {
    let mut h = H::new(NotificationConfig {
        percent: PercentRule {
            scope: NotifyScope::All,
            ..PercentRule::default()
        },
        ..NotificationConfig::default()
    });
    let mut l = two(99.0, 99.0);
    for a in &mut l.accounts {
        a.alias = None;
    }
    let out = h.run(&l, NOW);
    assert!(!out.is_empty());
    for n in out {
        let all = format!("{}{}{}{}", n.title.it, n.title.en, n.body.it, n.body.en);
        assert!(!all.contains('@'), "{all}");
    }
}

#[test]
fn headless_event_shape() {
    let n = Notification {
        kind: NotificationKind::Stale,
        severity: Severity::Warning,
        key: "stale:1".into(),
        account: Some(1),
        title: LocalizedText {
            it: "a".into(),
            en: "b".into(),
        },
        body: LocalizedText {
            it: "c".into(),
            en: "d".into(),
        },
    };
    let v = serde_json::to_value(HeadlessEvent::Notification { notification: n }).unwrap();
    assert_eq!(v["event"], "notification");
    assert_eq!(v["notification"]["kind"], "stale");
    assert_eq!(v["notification"]["title"]["en"], "b");
}

#[test]
fn earlier_available_to_the_first_is_a_return() {
    let rules = engine_rules(); // prima della lista: 1
    let l = two(10.0, 0.0);
    for (to, expect) in [
        (1, NotificationKind::ReturnedToPrimary),
        (2, NotificationKind::Switched),
    ] {
        let mut h = H::new(only(&["switched", "returned"]));
        let d = Decision::Switch {
            from: 3,
            to,
            cause: SwitchCause::EarlierAvailable,
        };
        let input = NotifyInput {
            list: Some(&l),
            engine: Some(EngineContext {
                rules: &rules,
                decision: Some(&d),
                switched: Some(SwitchDone {
                    from: Some(3),
                    to,
                    cause: SwitchCause::EarlierAvailable,
                }),
                preview: None,
                profile: None,
            }),
            ..NotifyInput::default()
        };
        assert_eq!(kinds(&h.with(&input, NOW)), [expect]);
    }
}
