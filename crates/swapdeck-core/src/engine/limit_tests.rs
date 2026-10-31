//! F22 «Massimo per utenza»: tetto sulle soglie della mappa, comune a tutte le mappe.

use super::tests::{NOW, acc, back, case, idle, list, run_cases, susp, sw, user_rules, with_mem};
use super::*;

fn limit(five_hour: Option<f64>, seven_day: Option<f64>) -> AccountLimit {
    AccountLimit {
        five_hour,
        seven_day,
    }
}

/// Regole dell'utente (lista 6, 1, 3; 5h 95, 7d 98, isteresi 10) con i massimi dati.
fn limited(limits: &[(u32, AccountLimit)]) -> EngineRules {
    let mut r = user_rules();
    r.account_limits = limits.iter().copied().collect();
    r
}

fn eval(
    rules: &EngineRules,
    active: u32,
    accounts: Vec<Account>,
    mem: &EngineMemory,
) -> Evaluation {
    let l = list(Some(active), accounts);
    let obs = Observation {
        list: &l,
        conflicts: &[],
    };
    evaluate(obs, rules, mem, NOW)
}

#[test]
fn effective_thresholds_are_the_lowest() {
    let r = limited(&[
        (6, limit(Some(85.0), None)),
        (1, limit(Some(99.0), Some(80.0))),
    ]);
    // Massimo più basso della mappa: vince il massimo.
    assert_eq!(
        r.thresholds(6),
        EffectiveThresholds {
            number: 6,
            five_hour: 85.0,
            seven_day: Some(98.0),
            five_hour_capped: true,
            seven_day_capped: false,
        }
    );
    // Più alto: vince la mappa (5h); 7d più basso della mappa.
    let t = r.thresholds(1);
    assert_eq!((t.five_hour, t.five_hour_capped), (95.0, false));
    assert_eq!((t.seven_day, t.seven_day_capped), (Some(80.0), true));
    // Senza massimo: la mappa.
    let t = r.thresholds(3);
    assert_eq!((t.five_hour, t.seven_day), (95.0, Some(98.0)));
    assert!(!t.five_hour_capped && !t.seven_day_capped);
    // Mappa 50, massimo 85: 50.
    let low = EngineRules {
        five_hour_threshold: 50.0,
        ..r.clone()
    };
    assert_eq!(low.thresholds(6).five_hour, 50.0);
    // 7d della mappa `null`: il massimo se c'è, altrimenti nessuna soglia (100 nei confronti).
    let open = EngineRules {
        seven_day_threshold: None,
        ..r.clone()
    };
    assert_eq!(open.thresholds(1).seven_day, Some(80.0));
    assert!(open.thresholds(1).seven_day_capped);
    assert_eq!(open.thresholds(6).seven_day, None);
    assert_eq!(open.thresholds(6).seven_day_limit(), 100.0);
    // Massimo uguale alla mappa o non valido: decide la mappa.
    let odd = limited(&[
        (6, limit(Some(95.0), Some(f64::NAN))),
        (1, limit(Some(0.0), Some(120.0))),
    ]);
    for n in [6, 1] {
        let t = odd.thresholds(n);
        assert_eq!((t.five_hour, t.seven_day), (95.0, Some(98.0)), "utenza {n}");
        assert!(!t.five_hour_capped && !t.seven_day_capped);
    }
}

/// Tabella dei casi del massimo sulle regole dell'utente.
#[test]
#[rustfmt::skip]
fn limit_scenarios() {
    use IdleReason::*;
    use SwitchCause::*;
    let five = vec![LimitWindow::FiveHour];
    let left6 = EngineMemory {
        last_switch_at: Some(NOW - 3600),
        left_accounts: vec![LeftAccount { number: 6, left_at: NOW - 6 * 3600, reset_at: Some(NOW - 60), windows: five }],
        ..EngineMemory::default()
    };
    // Massimo 5h di 6 all'85%: a 86% si cambia (senza massimo resterebbe).
    run_cases(&user_rules(), vec![
        case("senza massimo: 86% resta", 6, vec![acc(6, 86.0, 30.0), acc(1, 0.0, 10.0), acc(3, 0.0, 20.0)], idle(BelowThreshold)),
    ]);
    run_cases(&limited(&[(6, limit(Some(85.0), None))]), vec![
        case("massimo più basso: vince il massimo", 6, vec![acc(6, 86.0, 30.0), acc(1, 0.0, 10.0), acc(3, 0.0, 20.0)], sw(6, 1, PrimaryAtLimit)),
        case("sotto il massimo: resta", 6, vec![acc(6, 84.0, 30.0), acc(1, 0.0, 10.0)], idle(BelowThreshold)),
    ]);
    // Massimo più alto della mappa: decide la mappa (95).
    run_cases(&limited(&[(6, limit(Some(99.0), Some(100.0)))]), vec![
        case("massimo più alto: vince la mappa", 6, vec![acc(6, 96.0, 30.0), acc(1, 0.0, 10.0)], sw(6, 1, PrimaryAtLimit)),
        case("sotto la mappa: resta", 6, vec![acc(6, 94.0, 30.0), acc(1, 0.0, 10.0)], idle(BelowThreshold)),
    ]);
    // 7d della mappa spenta: vale il massimo 7d dell'utenza.
    let open = EngineRules { seven_day_threshold: None, ..limited(&[(6, limit(None, Some(80.0)))]) };
    run_cases(&open, vec![
        case("7d mappa null: il massimo 7d", 6, vec![acc(6, 10.0, 81.0), acc(1, 0.0, 10.0)], sw(6, 1, PrimaryAtLimit)),
        case("7d mappa null: senza massimo solo il 100%", 1, vec![acc(6, 10.0, 81.0), acc(1, 10.0, 99.0)], idle(OnReserve)),
    ]);
    // Riserva oltre il proprio massimo: non si sceglie (1 ha la 7d più bassa ma 5h oltre 50).
    run_cases(&limited(&[(1, limit(Some(50.0), None))]), vec![
        case("riserva oltre il suo massimo esclusa", 6, vec![acc(6, 96.0, 30.0), acc(1, 60.0, 10.0), acc(3, 0.0, 20.0)], sw(6, 3, PrimaryAtLimit)),
        case("riserva sotto il suo massimo scelta", 6, vec![acc(6, 96.0, 30.0), acc(1, 40.0, 10.0), acc(3, 0.0, 20.0)], sw(6, 1, PrimaryAtLimit)),
    ]);
    // Ritorno alla principale: soglia effettiva (80) meno isteresi (10) = sotto 70.
    let back80 = limited(&[(6, limit(Some(80.0), None))]);
    run_cases(&back80, vec![
        with_mem("ritorno: 75% non basta col massimo 80", 1, vec![acc(6, 75.0, 30.0), acc(1, 20.0, 10.0), acc(3, 0.0, 20.0)], left6.clone(), idle(OnReserve)),
        with_mem("ritorno: sotto 70% si torna", 1, vec![acc(6, 65.0, 30.0), acc(1, 20.0, 10.0), acc(3, 0.0, 20.0)], left6.clone(), back(1, PrimaryReset)),
    ]);
    // Massimo (8) sotto l'isteresi (10): il margine scende a metà del massimo, così si torna (sotto 4).
    let low = limited(&[(6, limit(Some(8.0), None))]);
    run_cases(&low, vec![
        with_mem("ritorno col massimo sotto l'isteresi: 0% si torna", 1, vec![acc(6, 0.0, 30.0), acc(1, 20.0, 10.0), acc(3, 0.0, 20.0)], left6.clone(), back(1, PrimaryReset)),
        with_mem("ritorno col massimo sotto l'isteresi: 5% non basta", 1, vec![acc(6, 5.0, 30.0), acc(1, 20.0, 10.0), acc(3, 0.0, 20.0)], left6.clone(), idle(OnReserve)),
    ]);
    // Preset con isteresi alta (45) e massimo 40: si torna sotto 20.
    let high = EngineRules { return_hysteresis: 45.0, ..limited(&[(6, limit(Some(40.0), None))]) };
    run_cases(&high, vec![
        with_mem("isteresi 45, massimo 40: 15% si torna", 1, vec![acc(6, 15.0, 30.0), acc(1, 20.0, 10.0), acc(3, 0.0, 20.0)], left6.clone(), back(1, PrimaryReset)),
        with_mem("isteresi 45, massimo 40: 25% non basta", 1, vec![acc(6, 25.0, 30.0), acc(1, 20.0, 10.0), acc(3, 0.0, 20.0)], left6.clone(), idle(OnReserve)),
    ]);
    run_cases(&user_rules(), vec![
        with_mem("ritorno senza massimo: 75% basta (sotto 85)", 1, vec![acc(6, 75.0, 30.0), acc(1, 20.0, 10.0), acc(3, 0.0, 20.0)], left6, back(1, PrimaryReset)),
    ]);
    // Tutte oltre il proprio massimo: si resta sull'attiva, «nessuna riserva disponibile».
    let all = limited(&[(6, limit(Some(70.0), None)), (1, limit(Some(50.0), None)), (3, limit(None, Some(40.0)))]);
    run_cases(&all, vec![
        case("tutte oltre il loro massimo", 6, vec![acc(6, 71.0, 30.0), acc(1, 60.0, 10.0), acc(3, 0.0, 45.0)], susp(SuspendReason::NoViableTarget)),
        case("una riserva sotto il suo massimo", 6, vec![acc(6, 71.0, 30.0), acc(1, 60.0, 10.0), acc(3, 0.0, 39.0)], sw(6, 3, PrimaryAtLimit)),
    ]);
    // Il motore non torna su un'utenza oltre il suo massimo nemmeno di ripiego.
    let fallback = limited(&[(6, limit(Some(60.0), None))]);
    run_cases(&fallback, vec![
        case("ripiego sulla principale oltre il massimo: no", 3, vec![acc(6, 65.0, 30.0), acc(1, 97.0, 10.0), acc(3, 96.0, 20.0)], susp(SuspendReason::NoViableTarget)),
    ]);
}

/// I criteri che usano i margini li misurano sulle soglie effettive di ciascuna riserva.
#[test]
#[rustfmt::skip]
fn criteria_margins_use_effective_thresholds() {
    let accounts = || vec![acc(6, 99.0, 30.0), acc(1, 50.0, 10.0), acc(3, 60.0, 10.0)];
    let to = |n: u32| sw(6, n, SwitchCause::PrimaryAtLimit);
    let best = EngineRules { reserve_criterion: ReserveCriterion::Best, ..user_rules() };
    let only5 = EngineRules {
        reserve_criterion: ReserveCriterion::BalancedScore,
        score_weights: Some(ScoreWeights { five_hour: 1.0, seven_day: 0.0, reset: 0.0, pace: 0.0 }),
        ..user_rules()
    };
    for rules in [best, only5] {
        // Margini 5h: 1 → 45, 3 → 35. Col massimo 70 su 1: 1 → 20.
        run_cases(&rules, vec![case("senza massimo", 6, accounts(), to(1))]);
        let mut capped = rules.clone();
        capped.account_limits.insert(1, limit(Some(70.0), None));
        run_cases(&capped, vec![case("massimo di 1 a 70", 6, accounts(), to(3))]);
        let e = eval(&capped, 6, accounts(), &EngineMemory::default());
        assert_eq!(e.candidate.map(|c| c.number), Some(3), "{:?}", rules.reserve_criterion);
    }
}

#[test]
fn reasons_name_the_account_maximum() {
    let mem = EngineMemory::default();
    let r = limited(&[(6, limit(None, Some(80.0)))]);
    let e = eval(&r, 6, vec![acc(6, 10.0, 82.0), acc(1, 0.0, 10.0)], &mem);
    assert_eq!(
        e.reason,
        "7d di #6 al 82%, al suo massimo dei 7 giorni (80%): passo a #1, settimanale più bassa (10%)"
    );
    let r = limited(&[(6, limit(Some(85.0), None))]);
    let e = eval(&r, 6, vec![acc(6, 86.0, 30.0), acc(1, 0.0, 10.0)], &mem);
    assert!(
        e.reason
            .starts_with("5h di #6 al 86%, al suo massimo delle 5 ore (85%): passo a #1"),
        "{}",
        e.reason
    );
    // Decide la mappa: nessuna menzione del massimo.
    let r = limited(&[(6, limit(Some(99.0), None))]);
    let e = eval(&r, 6, vec![acc(6, 96.0, 30.0), acc(1, 0.0, 10.0)], &mem);
    assert!(
        e.reason.starts_with("5h di #6 al 96%: passo a #1"),
        "{}",
        e.reason
    );
    // Tutte oltre: stesso motivo «nessuna utenza utilizzabile».
    let r = limited(&[(6, limit(Some(70.0), None)), (1, limit(Some(5.0), None))]);
    let e = eval(&r, 6, vec![acc(6, 71.0, 30.0), acc(1, 6.0, 10.0)], &mem);
    assert_eq!(
        e.reason,
        "5h di #6 al 71%, al suo massimo delle 5 ore (70%): nessuna utenza utilizzabile"
    );
    // «Adesso andrebbe a»: la riserva oltre il suo massimo non è il candidato.
    let e = eval(
        &r,
        6,
        vec![acc(6, 10.0, 30.0), acc(1, 6.0, 10.0), acc(3, 0.0, 50.0)],
        &mem,
    );
    assert_eq!(e.candidate.map(|c| c.number), Some(3));
}

/// Le finestre registrate all'uscita (per l'isteresi del ritorno) seguono il massimo.
#[test]
fn left_windows_follow_the_maximum() {
    let r = limited(&[(6, limit(None, Some(80.0)))]);
    let l = list(Some(6), vec![acc(6, 10.0, 82.0), acc(1, 0.0, 10.0)]);
    let mut mem = EngineMemory::default();
    mem.record_switch(&r, &l, 1, NOW);
    assert_eq!(mem.left_accounts[0].windows, vec![LimitWindow::SevenDay]);
    assert!(mem.left_accounts[0].reset_at.is_some());
}

#[test]
fn limits_validation_and_storage() {
    // Config vecchia senza la sezione: nessun massimo, niente chiave in scrittura.
    let old: EngineRules = serde_json::from_str(r#"{"enabled":true,"order":[6,1]}"#).unwrap();
    assert!(old.account_limits.is_empty());
    assert!(
        !serde_json::to_string(&old)
            .unwrap()
            .contains("accountLimits")
    );
    // Lettura e scrittura; le voci vuote (massimo tolto) spariscono.
    let r: EngineRules = serde_json::from_str(
        r#"{"order":[6,1],"accountLimits":{"6":{"sevenDay":80},"1":{},"3":{"fiveHour":null}}}"#,
    )
    .unwrap();
    assert_eq!(
        r.account_limits,
        BTreeMap::from([(6, limit(None, Some(80.0)))])
    );
    let text = serde_json::to_string(&r).unwrap();
    assert!(
        text.contains(r#""accountLimits":{"6":{"sevenDay":80.0}}"#),
        "{text}"
    );
    // Valori in (0, 100] come le soglie della mappa; numero 0 e troppe voci rifiutati.
    for bad in [0.0, -1.0, 100.5, f64::NAN, f64::INFINITY] {
        let r = limited(&[(1, limit(Some(bad), None))]);
        assert_eq!(
            r.validate(),
            ["massimo dell'utenza 1 fuori da 1–100"],
            "{bad}"
        );
        let r = limited(&[(1, limit(None, Some(bad)))]);
        assert_eq!(r.validate().len(), 1, "{bad}");
    }
    assert!(
        limited(&[(1, limit(Some(100.0), Some(0.5)))])
            .validate()
            .is_empty()
    );
    let zero = limited(&[(0, limit(Some(50.0), None))]);
    assert!(
        zero.validate_for_save()
            .contains(&"i numeri degli account partono da 1".to_owned())
    );
    let many: Vec<(u32, AccountLimit)> = (1..=MAX_RULE_ACCOUNTS as u32 + 1)
        .map(|n| (n, limit(Some(50.0), None)))
        .collect();
    assert!(!limited(&many).validate_for_save().is_empty());
    // Un massimo non valido sospende il motore come una soglia non valida.
    let bad = limited(&[(1, limit(Some(120.0), None))]);
    let l = list(Some(6), vec![acc(6, 10.0, 10.0), acc(1, 0.0, 10.0)]);
    let obs = Observation {
        list: &l,
        conflicts: &[],
    };
    assert!(matches!(
        decide(obs, &bad, &EngineMemory::default(), NOW),
        Decision::Suspended {
            reason: SuspendReason::InvalidRules,
            ..
        }
    ));
}

/// Il massimo è comune a tutte le mappe: si salva anche con un preset attivo e passa da una
/// mappa all'altra.
#[test]
fn limits_are_common_to_all_maps() {
    let current = EngineRules {
        order: Some(vec![6, 1, 3]),
        ..user_rules()
    };
    let mut p = EngineProfiles::seeded(&current);
    let sport = p
        .profiles
        .iter()
        .find(|m| m.preset == Some(PresetKind::Sport))
        .unwrap()
        .id
        .clone();
    let rules = p
        .apply(&ProfileCommand::Select { id: sport }, &current)
        .unwrap();
    p.normalize(&rules);
    // Preset attivo: cambiare solo i massimi è permesso.
    let mut with_limits = rules.clone();
    with_limits
        .account_limits
        .insert(1, limit(Some(80.0), None));
    assert_eq!(p.check_editable(&with_limits), Ok(()));
    p.normalize(&with_limits);
    // Altra mappa: i massimi restano.
    let other = p.apply(
        &ProfileCommand::Select {
            id: profiles::PERSONAL_ID.to_owned(),
        },
        &with_limits,
    );
    let other = other.unwrap();
    assert_eq!(other.account_limits, with_limits.account_limits);
    assert_ne!(other.five_hour_threshold, with_limits.five_hour_threshold);
    // Clone e nuova mappa li portano con sé.
    let created = p
        .apply(
            &ProfileCommand::Create {
                name: "Nuova".into(),
            },
            &other,
        )
        .unwrap();
    assert_eq!(created.account_limits, other.account_limits);
}

#[test]
fn engine_state_carries_effective_thresholds() {
    let r = limited(&[(1, limit(Some(70.0), None))]);
    let l = list(Some(6), vec![acc(6, 10.0, 10.0), acc(1, 0.0, 10.0)]);
    let s = EngineState::new(r.clone(), None, Vec::new(), Some(&l));
    assert_eq!(s.thresholds, vec![r.thresholds(6), r.thresholds(1)]);
    assert_eq!(s.thresholds[1].five_hour, 70.0);
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["thresholds"][1]["fiveHourCapped"], true);
    // Senza lista: niente soglie (né chiave).
    let s = EngineState::new(r, None, Vec::new(), None);
    assert!(
        serde_json::to_value(&s)
            .unwrap()
            .get("thresholds")
            .is_none()
    );
}
