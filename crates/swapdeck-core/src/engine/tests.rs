//! Scenari del motore: regola attuale dell'utente (`cswap-guard`) e casi limite.

use super::*;
use crate::conflicts::{Conflict, ConflictKind};
use crate::timeutil::format_iso;

const NOW: i64 = 1_800_000_000;
const H5_RESET: i64 = NOW + 3600;

fn window(pct: f64, reset: i64) -> UsageWindow {
    UsageWindow {
        pct,
        resets_at: Some(format_iso(reset)),
        ..Default::default()
    }
}

fn usage(p5: f64, p7: f64) -> Usage {
    Usage {
        five_hour: Some(window(p5, H5_RESET)),
        seven_day: Some(window(p7, NOW + 3 * 86_400)),
        ..Default::default()
    }
}

pub(crate) fn acc(number: u32, p5: f64, p7: f64) -> Account {
    Account {
        number,
        email: format!("a{number}@example.test"),
        organization_name: String::new(),
        organization_uuid: String::new(),
        is_organization: false,
        active: false,
        usage_status: UsageStatus::Ok,
        usage: Some(usage(p5, p7)),
        alias: None,
        disabled: None,
        login_expires_at: None,
        usage_fetched_at: None,
        usage_age_seconds: Some(30.0),
        last_good_usage: None,
        last_good_fetched_at: None,
        last_good_age_seconds: None,
        usage_error: None,
        usage_retry_at: None,
    }
}

fn stale(mut a: Account, age: f64) -> Account {
    a.usage_age_seconds = Some(age);
    a
}

fn status(mut a: Account, s: UsageStatus) -> Account {
    a.usage_status = s;
    if s != UsageStatus::Ok {
        a.last_good_usage = a.usage.take();
        a.usage_age_seconds = None;
    }
    a
}

fn disabled(mut a: Account) -> Account {
    a.disabled = Some(true);
    a
}

pub(crate) fn list(active: Option<u32>, mut accounts: Vec<Account>) -> ListResponse {
    for a in &mut accounts {
        a.active = Some(a.number) == active;
    }
    ListResponse {
        schema_version: 1,
        active_account_number: active,
        accounts,
        duplicate_account_warnings: None,
        lockstep_usage_warnings: None,
        unclaimed_credentials: None,
    }
}

/// Regola attuale: principale 6, riserve [1, 3], settimanale più bassa, soglia 95%.
pub(crate) fn user_rules() -> EngineRules {
    EngineRules {
        enabled: true,
        primary: Some(6),
        reserves: vec![1, 3],
        ..EngineRules::default()
    }
}

fn guard_task() -> Conflict {
    Conflict {
        id: "task:cswap-guard".to_owned(),
        kind: ConflictKind::ScheduledTask,
        label: "\\cswap-guard".to_owned(),
        certain: true,
    }
}

fn sw(from: u32, to: u32, cause: SwitchCause) -> Decision {
    Decision::Switch { from, to, cause }
}

fn back(from: u32, cause: SwitchCause) -> Decision {
    Decision::ReturnToPrimary { from, to: 6, cause }
}

fn idle(reason: IdleReason) -> Decision {
    Decision::NoAction { reason }
}

fn susp(reason: SuspendReason) -> Decision {
    Decision::Suspended {
        reason,
        detail: Vec::new(),
    }
}

struct Case {
    name: &'static str,
    active: Option<u32>,
    accounts: Vec<Account>,
    mem: EngineMemory,
    expect: Decision,
}

fn case(name: &'static str, active: u32, accounts: Vec<Account>, expect: Decision) -> Case {
    with_mem(name, active, accounts, EngineMemory::default(), expect)
}

fn with_mem(
    name: &'static str,
    active: u32,
    accounts: Vec<Account>,
    mem: EngineMemory,
    expect: Decision,
) -> Case {
    Case {
        name,
        active: Some(active),
        accounts,
        mem,
        expect,
    }
}

fn run_cases(rules: &EngineRules, cases: Vec<Case>) {
    for c in cases {
        let l = list(c.active, c.accounts);
        let obs = Observation {
            list: &l,
            conflicts: &[],
        };
        assert_eq!(
            decide(obs, rules, &c.mem, NOW),
            c.expect,
            "scenario: {}",
            c.name
        );
    }
}

#[test]
#[rustfmt::skip]
fn user_rule_scenarios() {
    use IdleReason::*;
    use SwitchCause::*;
    let passed = EngineMemory {
        last_switch_at: Some(NOW - 6 * 3600),
        left_primary_at: Some(NOW - 6 * 3600),
        primary_reset_at: Some(NOW - 60),
        primary_left_windows: Some(vec![LimitWindow::FiveHour]),
    };
    let future = EngineMemory { primary_reset_at: Some(NOW + 600), ..passed.clone() };
    let seven_day = EngineMemory { primary_left_windows: Some(vec![LimitWindow::SevenDay]), ..passed.clone() };
    let mut reset_window = acc(6, 97.0, 30.0);
    reset_window.usage.as_mut().unwrap().five_hour = Some(window(97.0, NOW - 1));
    let mut unknown = acc(1, 0.0, 0.0);
    unknown.usage = None;
    let cases = vec![
        case("principale sotto soglia", 6, vec![acc(6, 50.0, 30.0), acc(1, 0.0, 40.0), acc(3, 0.0, 20.0)], idle(BelowThreshold)),
        case("principale al 95%: settimanale più bassa", 6, vec![acc(6, 95.0, 30.0), acc(1, 0.0, 40.0), acc(3, 0.0, 20.0)], sw(6, 3, PrimaryAtLimit)),
        case("riserva scelta al limite: l'altra", 6, vec![acc(6, 96.0, 30.0), acc(1, 10.0, 40.0), acc(3, 97.0, 20.0)], sw(6, 1, PrimaryAtLimit)),
        case("settimana del principale al 98%", 6, vec![acc(6, 10.0, 98.0), acc(1, 0.0, 40.0), acc(3, 0.0, 20.0)], sw(6, 3, PrimaryAtLimit)),
        case("tutti al limite", 6, vec![acc(6, 99.0, 30.0), acc(1, 95.0, 40.0), acc(3, 10.0, 99.0)], susp(SuspendReason::NoViableTarget)),
        case("parità sulla settimanale: decide la 5h", 6, vec![acc(6, 99.0, 30.0), acc(1, 5.0, 20.0), acc(3, 1.0, 20.0)], sw(6, 3, PrimaryAtLimit)),
        case("finestra già resettata", 6, vec![reset_window], idle(BelowThreshold)),
        case("su riserva, principale al limite", 3, vec![acc(6, 96.0, 30.0), acc(1, 0.0, 40.0), acc(3, 20.0, 20.0)], idle(OnReserve)),
        case("riserva al limite: altra riserva", 3, vec![acc(6, 96.0, 30.0), acc(1, 0.0, 40.0), acc(3, 95.0, 20.0)], sw(3, 1, ReserveAtLimit)),
        case("riserve al limite, principale sotto soglia", 3, vec![acc(6, 90.0, 30.0), acc(1, 99.0, 40.0), acc(3, 95.0, 20.0)], back(3, ReservesAtLimit)),
        case("riserve e principale al limite", 3, vec![acc(6, 96.0, 30.0), acc(1, 99.0, 40.0), acc(3, 95.0, 20.0)], susp(SuspendReason::NoViableTarget)),
        case("429: principale vecchio ma al limite", 6, vec![stale(acc(6, 97.0, 30.0), 4000.0), acc(1, 0.0, 40.0), acc(3, 0.0, 20.0)], sw(6, 3, PrimaryAtLimit)),
        case("429: riserva con dato vecchio saltata", 6, vec![acc(6, 97.0, 30.0), acc(1, 0.0, 40.0), stale(acc(3, 0.0, 20.0), 4000.0)], sw(6, 1, PrimaryAtLimit)),
        case("429: riserva unavailable saltata", 6, vec![acc(6, 97.0, 30.0), acc(1, 0.0, 40.0), status(acc(3, 0.0, 20.0), UsageStatus::Unavailable)], sw(6, 1, PrimaryAtLimit)),
        case("429: principale sotto soglia ma vecchio", 6, vec![stale(acc(6, 50.0, 30.0), 4000.0), acc(1, 0.0, 40.0)], idle(StaleUsage)),
        case("429: lastGood del principale al limite", 6, vec![status(acc(6, 97.0, 30.0), UsageStatus::Unavailable), acc(1, 0.0, 40.0)], sw(6, 1, PrimaryAtLimit)),
        case("riserva con consumi sconosciuti", 6, vec![acc(6, 97.0, 30.0), unknown, acc(3, 0.0, 50.0)], sw(6, 3, PrimaryAtLimit)),
        case("principale in re-login", 6, vec![status(acc(6, 10.0, 30.0), UsageStatus::ReloginRequired), acc(1, 0.0, 40.0), acc(3, 0.0, 20.0)], sw(6, 3, ActiveNeedsRelogin)),
        case("riserva in re-login saltata", 6, vec![acc(6, 96.0, 30.0), acc(1, 0.0, 40.0), status(acc(3, 0.0, 20.0), UsageStatus::ReloginRequired)], sw(6, 1, PrimaryAtLimit)),
        case("riserva disabilitata saltata", 6, vec![acc(6, 96.0, 30.0), acc(1, 0.0, 40.0), disabled(acc(3, 0.0, 20.0))], sw(6, 1, PrimaryAtLimit)),
        case("riserva assente da cswap", 6, vec![acc(6, 96.0, 30.0), acc(3, 0.0, 20.0)], sw(6, 3, PrimaryAtLimit)),
        case("attivo escluso", 2, vec![acc(6, 96.0, 30.0), acc(2, 99.0, 20.0)], idle(ActiveExcluded)),
        case("attivo fuori dalle regole", 4, vec![acc(6, 96.0, 30.0), acc(4, 99.0, 20.0)], idle(ActiveOutsideRules)),
        Case { name: "nessun account attivo", active: None, accounts: vec![acc(6, 96.0, 30.0)], mem: EngineMemory::default(), expect: susp(SuspendReason::NoActiveAccount) },
        with_mem("ritorno dopo il reset", 3, vec![acc(6, 5.0, 31.0), acc(1, 0.0, 40.0), acc(3, 20.0, 20.0)], passed.clone(), back(3, PrimaryReset)),
        with_mem("isteresi: principale al 90% resta", 3, vec![acc(6, 90.0, 31.0), acc(3, 20.0, 20.0)], passed.clone(), idle(OnReserve)),
        with_mem("reset non ancora passato", 3, vec![acc(6, 5.0, 31.0), acc(3, 20.0, 20.0)], future, idle(OnReserve)),
        with_mem("principale in re-login non torna", 3, vec![status(acc(6, 0.0, 0.0), UsageStatus::ReloginRequired), acc(3, 20.0, 20.0)], passed.clone(), idle(OnReserve)),
        with_mem("principale vecchio non torna", 3, vec![stale(acc(6, 0.0, 0.0), 4000.0), acc(3, 20.0, 20.0)], passed.clone(), idle(OnReserve)),
        with_mem("lasciato per la 5h: 7d al 93% non frena", 3, vec![acc(6, 5.0, 93.0), acc(3, 20.0, 20.0)], passed.clone(), back(3, PrimaryReset)),
        with_mem("senza memoria: 7d al 90% non frena", 3, vec![acc(6, 5.0, 90.0), acc(3, 20.0, 20.0)], EngineMemory::default(), back(3, PrimaryReset)),
        with_mem("lasciato per la 7d: 7d al 93% resta", 3, vec![acc(6, 5.0, 93.0), acc(3, 20.0, 20.0)], seven_day.clone(), idle(OnReserve)),
        with_mem("lasciato per la 7d: 7d all'85% torna", 3, vec![acc(6, 5.0, 85.0), acc(3, 20.0, 20.0)], seven_day, back(3, PrimaryReset)),
    ];
    let mut rules = user_rules();
    rules.excluded = vec![2];
    run_cases(&rules, cases);
}

#[test]
#[rustfmt::skip]
fn cooldown_blocks_unless_hard_limit() {
    let mem = EngineMemory { last_switch_at: Some(NOW - 100), ..EngineMemory::default() };
    let done = EngineMemory { last_switch_at: Some(NOW - 300), ..EngineMemory::default() };
    let cases = vec![
        with_mem("pausa minima", 6, vec![acc(6, 96.0, 30.0), acc(1, 0.0, 40.0)], mem.clone(), idle(IdleReason::Cooldown)),
        with_mem("al 100% si cambia subito", 6, vec![acc(6, 100.0, 30.0), acc(1, 0.0, 40.0)], mem.clone(), sw(6, 1, SwitchCause::PrimaryAtLimit)),
        with_mem("re-login si cambia subito", 6, vec![status(acc(6, 1.0, 30.0), UsageStatus::ReloginRequired), acc(1, 0.0, 40.0)], mem, sw(6, 1, SwitchCause::ActiveNeedsRelogin)),
        with_mem("pausa trascorsa", 6, vec![acc(6, 96.0, 30.0), acc(1, 0.0, 40.0)], done, sw(6, 1, SwitchCause::PrimaryAtLimit)),
    ];
    run_cases(&user_rules(), cases);
}

#[test]
fn criteria_and_implicit_reserves() {
    let accounts = || {
        vec![
            acc(6, 99.0, 30.0),
            acc(1, 60.0, 10.0),
            acc(2, 0.0, 70.0),
            acc(3, 30.0, 50.0),
        ]
    };
    let at_limit = |to| sw(6, to, SwitchCause::PrimaryAtLimit);
    let mut rules = user_rules();
    rules.reserves = vec![1, 2, 3];
    rules.reserve_criterion = ReserveCriterion::Best;
    // Margini: 1 → min(35, 88) = 35; 2 → min(95, 28) = 28; 3 → min(65, 48) = 48.
    run_cases(&rules, vec![case("best", 6, accounts(), at_limit(3))]);
    rules.reserve_criterion = ReserveCriterion::FixedOrder;
    run_cases(
        &rules,
        vec![case("ordine fisso", 6, accounts(), at_limit(1))],
    );
    rules.reserve_criterion = ReserveCriterion::LowestWeekly;
    rules.reserves = Vec::new();
    rules.excluded = vec![1];
    // Riserve implicite: tutti tranne principale ed esclusi (2 e 3); settimanale più bassa = 3.
    run_cases(&rules, vec![case("implicite", 6, accounts(), at_limit(3))]);
    rules.seven_day_threshold = None;
    let d = idle(IdleReason::BelowThreshold);
    run_cases(
        &rules,
        vec![case("7d spenta", 6, vec![acc(6, 10.0, 99.0)], d)],
    );
}

#[test]
fn disabled_dry_run_invalid_and_conflicts() {
    let l = list(Some(6), vec![acc(6, 99.0, 30.0), acc(1, 0.0, 40.0)]);
    let mem = EngineMemory::default();
    let expected = sw(6, 1, SwitchCause::PrimaryAtLimit);
    let off = EngineRules::default();
    assert!(!off.enabled, "il motore è spento al primo avvio");
    let obs = Observation {
        list: &l,
        conflicts: &[],
    };
    assert_eq!(decide(obs, &off, &mem, NOW), susp(SuspendReason::Disabled));

    let dry = EngineRules {
        enabled: false,
        dry_run: true,
        ..user_rules()
    };
    assert_eq!(decide(obs, &dry, &mem, NOW), expected);

    let invalid = EngineRules {
        primary: None,
        ..user_rules()
    };
    assert!(matches!(
        decide(obs, &invalid, &mem, NOW),
        Decision::Suspended {
            reason: SuspendReason::InvalidRules,
            ..
        }
    ));
    let invalid = EngineRules {
        reserves: vec![6],
        interval_secs: 5,
        ..user_rules()
    };
    assert_eq!(invalid.validate().len(), 2);

    let conflicts = [guard_task()];
    let obs = Observation {
        list: &l,
        conflicts: &conflicts,
    };
    assert_eq!(
        decide(obs, &user_rules(), &mem, NOW),
        Decision::Suspended {
            reason: SuspendReason::Conflict,
            detail: vec!["task:cswap-guard".to_owned()]
        }
    );
    let ignoring = EngineRules {
        ignored_conflicts: vec!["TASK:cswap-guard".to_owned()],
        ..user_rules()
    };
    assert_eq!(decide(obs, &ignoring, &mem, NOW), expected);
    let possible = [Conflict {
        certain: false,
        id: "process:tui".to_owned(),
        ..guard_task()
    }];
    let obs = Observation {
        list: &l,
        conflicts: &possible,
    };
    assert_eq!(decide(obs, &user_rules(), &mem, NOW), expected);
}

#[test]
fn no_oscillation_near_threshold() {
    let rules = user_rules();
    let mut mem = EngineMemory::default();
    let mut active = 6;
    let mut switches = 0;
    // (minuti dall'inizio, % 5h del principale): oscilla intorno al 95%, poi il reset al minuto 60.
    let script: &[(i64, f64)] = &[
        (0, 94.0),
        (1, 95.0),
        (2, 94.0),
        (3, 96.0),
        (10, 93.0),
        (30, 94.5),
        (61, 3.0),
        (62, 96.0),
        (70, 4.0),
    ];
    for &(min, p5) in script {
        let now = NOW + min * 60;
        let reset = if now >= H5_RESET {
            H5_RESET + 5 * 3600
        } else {
            H5_RESET
        };
        let mut primary = acc(6, p5, 30.0);
        primary.usage.as_mut().unwrap().five_hour = Some(window(p5, reset));
        let l = list(
            Some(active),
            vec![primary, acc(1, 0.0, 40.0), acc(3, 0.0, 20.0)],
        );
        let obs = Observation {
            list: &l,
            conflicts: &[],
        };
        if let Some(to) = decide(obs, &rules, &mem, now).target() {
            mem.record_switch(&rules, &l, to, now);
            active = to;
            switches += 1;
        }
        if min == 1 {
            assert_eq!(mem.primary_reset_at, Some(H5_RESET), "reset ricordato");
            assert_eq!(mem.primary_left_windows, Some(vec![LimitWindow::FiveHour]));
        }
    }
    // 6→3 al minuto 1, ritorno 3→6 al 61; al 62 il 96% cade nella pausa minima; al 70 è al 4%.
    assert_eq!(switches, 2);
    assert_eq!(active, 6);
    assert_eq!(mem.primary_reset_at, None);
}
