//! Scenari dei criteri nuovi, del flusso a lista con priorità, dei motivi e della migrazione.

use super::tests::{
    H5_RESET, NOW, acc, back, case, idle, list, run_cases, status, susp, sw, user_rules, window,
    with_mem,
};
use super::*;
use crate::timeutil::format_iso;

const DAY: i64 = 86_400;

/// Account con il reset 7d tra `days` giorni.
fn week(number: u32, p5: f64, p7: f64, days: f64) -> Account {
    let mut a = acc(number, p5, p7);
    a.usage.as_mut().unwrap().seven_day = Some(window(p7, NOW + (days * DAY as f64) as i64));
    a
}

/// Campi di ritmo sulla 7d.
fn pace(
    mut a: Account,
    lasts: Option<bool>,
    expected: Option<f64>,
    exhaust_days: Option<i64>,
) -> Account {
    let w = a.usage.as_mut().unwrap().seven_day.as_mut().unwrap();
    w.will_last_to_reset = lasts;
    w.expected_pct = expected;
    w.projected_exhaustion_at = exhaust_days.map(|d| format_iso(NOW + d * DAY));
    a
}

fn scoped(mut a: Account, name: &str, pct: f64) -> Account {
    let w = UsageWindow {
        name: Some(name.to_owned()),
        ..window(pct, NOW + 2 * DAY)
    };
    a.usage.as_mut().unwrap().scoped = Some(vec![w]);
    a
}

fn alias(mut a: Account, name: &str) -> Account {
    a.alias = Some(name.to_owned());
    a
}

fn rules_with(criterion: ReserveCriterion) -> EngineRules {
    EngineRules {
        enabled: true,
        order: Some(vec![6, 1, 2, 3]),
        reserve_criterion: criterion,
        headroom_model: Some("Fable".to_owned()),
        ..EngineRules::default()
    }
}

fn at_limit_to(to: u32) -> Decision {
    sw(6, to, SwitchCause::PrimaryAtLimit)
}

fn no_target() -> Decision {
    susp(SuspendReason::NoViableTarget)
}

/// Principale al limite e riserve date: dove va ogni criterio.
#[test]
#[rustfmt::skip]
fn criteria_tables() {
    use ReserveCriterion::*;
    let main = || acc(6, 99.0, 30.0);
    let mut no_reset = week(2, 0.0, 10.0, 0.5);
    no_reset.usage.as_mut().unwrap().seven_day.as_mut().unwrap().resets_at = None;
    let mut no_5h = acc(2, 0.0, 10.0);
    no_5h.usage.as_mut().unwrap().five_hour = None;
    let mut no_7d = acc(2, 0.0, 10.0);
    no_7d.usage.as_mut().unwrap().seven_day = None;
    let tables: Vec<(ReserveCriterion, Vec<_>)> = vec![
        (ResetSoonest, vec![
            case("reset più vicino", 6, vec![main(), week(1, 0.0, 10.0, 3.0), week(2, 0.0, 50.0, 1.0), week(3, 0.0, 10.0, 2.0)], at_limit_to(2)),
            case("stesso reset: 7d più bassa", 6, vec![main(), week(1, 0.0, 50.0, 1.0), week(2, 0.0, 30.0, 1.0)], at_limit_to(2)),
            case("parità completa: ordine", 6, vec![main(), week(3, 0.0, 30.0, 1.0), week(1, 0.0, 30.0, 1.0)], at_limit_to(1)),
            case("reset sconosciuto: esclusa", 6, vec![main(), week(1, 0.0, 10.0, 3.0), no_reset.clone()], at_limit_to(1)),
            case("reset passato: esclusa", 6, vec![main(), week(1, 0.0, 10.0, 3.0), week(2, 0.0, 10.0, -0.1)], at_limit_to(1)),
            case("riserva sopra soglia saltata", 6, vec![main(), week(1, 0.0, 10.0, 3.0), week(2, 96.0, 10.0, 1.0)], at_limit_to(1)),
            case("nessun dato: nessuna candidata", 6, vec![main(), no_reset], no_target()),
        ]),
        (PaceSafe, vec![
            case("arriva al reset con più margine", 6, vec![main(), pace(acc(1, 0.0, 40.0), Some(true), Some(45.0), None), pace(acc(2, 0.0, 40.0), Some(true), Some(60.0), None), pace(acc(3, 0.0, 10.0), Some(false), Some(5.0), Some(1))], at_limit_to(2)),
            case("chi arriva vince sul margine", 6, vec![main(), pace(acc(1, 0.0, 10.0), Some(false), Some(90.0), Some(2)), pace(acc(3, 0.0, 40.0), Some(true), Some(41.0), None)], at_limit_to(3)),
            case("nessuna arriva: si esaurisce per ultima", 6, vec![main(), pace(acc(1, 0.0, 40.0), Some(false), Some(30.0), Some(1)), pace(acc(2, 0.0, 40.0), Some(false), Some(30.0), Some(2))], at_limit_to(2)),
            case("parità: ordine", 6, vec![main(), pace(acc(3, 0.0, 40.0), Some(true), Some(50.0), None), pace(acc(1, 0.0, 40.0), Some(true), Some(50.0), None)], at_limit_to(1)),
            case("ritmo sconosciuto: esclusa", 6, vec![main(), pace(acc(1, 0.0, 40.0), None, Some(90.0), None), pace(acc(2, 0.0, 40.0), Some(true), Some(41.0), None)], at_limit_to(2)),
            case("atteso sconosciuto: esclusa", 6, vec![main(), pace(acc(1, 0.0, 40.0), Some(true), None, None), pace(acc(2, 0.0, 40.0), Some(true), Some(41.0), None)], at_limit_to(2)),
            case("esaurimento sconosciuto: esclusa", 6, vec![main(), pace(acc(1, 0.0, 40.0), Some(false), Some(30.0), None), pace(acc(2, 0.0, 40.0), Some(false), Some(30.0), Some(1))], at_limit_to(2)),
            case("nessun dato di ritmo", 6, vec![main(), acc(1, 0.0, 40.0)], no_target()),
        ]),
        (Lowest5h, vec![
            case("5h più libera", 6, vec![main(), acc(1, 60.0, 10.0), acc(2, 10.0, 70.0), acc(3, 30.0, 0.0)], at_limit_to(2)),
            case("stessa 5h: 7d più bassa", 6, vec![main(), acc(1, 10.0, 50.0), acc(3, 10.0, 20.0)], at_limit_to(3)),
            case("parità: ordine", 6, vec![main(), acc(3, 10.0, 20.0), acc(1, 10.0, 20.0)], at_limit_to(1)),
            case("5h sconosciuta: esclusa", 6, vec![main(), acc(1, 50.0, 10.0), no_5h], at_limit_to(1)),
        ]),
        (EvenSpread, vec![
            case("più sotto la media 7d", 6, vec![main(), acc(1, 0.0, 50.0), acc(2, 0.0, 20.0), acc(3, 0.0, 35.0)], at_limit_to(2)),
            case("stessa 7d: 5h più bassa", 6, vec![main(), acc(1, 40.0, 20.0), acc(3, 5.0, 20.0)], at_limit_to(3)),
            case("7d sconosciuta: esclusa", 6, vec![main(), acc(1, 0.0, 50.0), no_7d], at_limit_to(1)),
        ]),
        (ModelHeadroom, vec![
            case("più margine sul modello", 6, vec![main(), scoped(acc(1, 0.0, 10.0), "Fable", 80.0), scoped(acc(2, 0.0, 60.0), "fable", 10.0)], at_limit_to(2)),
            case("modello assente: esclusa", 6, vec![main(), scoped(acc(1, 0.0, 10.0), "Fable", 80.0), scoped(acc(2, 0.0, 10.0), "Opus", 0.0), acc(3, 0.0, 0.0)], at_limit_to(1)),
            case("stesso margine: 7d più bassa", 6, vec![main(), scoped(acc(1, 0.0, 40.0), "Fable", 10.0), scoped(acc(3, 0.0, 20.0), "Fable", 10.0)], at_limit_to(3)),
        ]),
        (BalancedScore, vec![
            // Default: 1 → 0,4·(35/95) + 0,3·(88/98) + 0,15·(1-3/7) + 0,15·0,5 ≈ 0,58;
            // 2 → 0,4·(95/95) + 0,3·(28/98) + 0,15·(1-1/7) + 0,15·0,9 ≈ 0,75.
            case("punteggio più alto", 6, vec![main(), pace(week(1, 60.0, 10.0, 3.0), Some(true), Some(10.0), None), pace(week(2, 0.0, 70.0, 1.0), Some(true), Some(110.0), None)], at_limit_to(2)),
            case("ritmo sconosciuto: esclusa", 6, vec![main(), pace(week(1, 60.0, 10.0, 3.0), Some(true), Some(10.0), None), week(2, 0.0, 70.0, 1.0)], at_limit_to(1)),
        ]),
    ];
    for (criterion, cases) in tables {
        run_cases(&rules_with(criterion), cases);
    }
    // Pesi regolabili: solo la 7d conta, i dati di ritmo non servono più.
    let only7 = EngineRules {
        score_weights: Some(ScoreWeights { five_hour: 0.0, seven_day: 1.0, reset: 0.0, pace: 0.0 }),
        ..rules_with(BalancedScore)
    };
    run_cases(&only7, vec![
        case("solo 7d", 6, vec![main(), acc(1, 90.0, 10.0), acc(2, 0.0, 70.0)], at_limit_to(1)),
        case("parità: ordine", 6, vec![main(), acc(3, 0.0, 10.0), acc(1, 50.0, 10.0)], at_limit_to(1)),
    ]);
}

#[test]
fn criterion_parameters_are_validated() {
    let mut r = rules_with(ReserveCriterion::ModelHeadroom);
    assert!(r.validate().is_empty());
    r.headroom_model = Some("  ".to_owned());
    assert_eq!(
        r.validate(),
        ["manca il modello del criterio «margine sul modello»"]
    );
    r.headroom_model = Some("x".repeat(MAX_MODEL_NAME_LEN + 1));
    assert!(!r.validate_for_save().is_empty());
    let mut r = rules_with(ReserveCriterion::BalancedScore);
    for bad in [
        ScoreWeights {
            five_hour: 0.0,
            seven_day: 0.0,
            reset: 0.0,
            pace: 0.0,
        },
        ScoreWeights {
            five_hour: f64::NAN,
            ..ScoreWeights::default()
        },
        ScoreWeights {
            pace: -1.0,
            ..ScoreWeights::default()
        },
    ] {
        r.score_weights = Some(bad);
        assert_eq!(r.validate().len(), 1, "{bad:?}");
    }
}

fn left(number: u32, reset_at: i64, windows: Vec<LimitWindow>) -> LeftAccount {
    LeftAccount {
        number,
        left_at: NOW - 6 * 3600,
        reset_at: Some(reset_at),
        windows,
    }
}

fn mem_left(records: Vec<LeftAccount>) -> EngineMemory {
    EngineMemory {
        last_switch_at: Some(NOW - 3600),
        left_accounts: records,
        ..EngineMemory::default()
    }
}

/// Criterio «ordine»: sempre la prima utenza libera della lista [6, 1, 3].
#[test]
#[rustfmt::skip]
fn priority_flow_scenarios() {
    use IdleReason::*;
    use SwitchCause::*;
    let five = vec![LimitWindow::FiveHour];
    // 6 e 1 lasciate per la 5h; il reset di 1 è passato, quello di 6 no.
    let one_reset = mem_left(vec![left(6, NOW + 600, five.clone()), left(1, NOW - 60, five.clone())]);
    let both_reset = mem_left(vec![left(6, NOW - 60, five.clone()), left(1, NOW - 60, five.clone())]);
    let seven = mem_left(vec![left(6, NOW + 600, five.clone()), left(1, NOW - 60, vec![LimitWindow::SevenDay])]);
    let cases = vec![
        case("principale al limite: la prima libera", 6, vec![acc(6, 96.0, 30.0), acc(1, 50.0, 90.0), acc(3, 0.0, 10.0)], sw(6, 1, PrimaryAtLimit)),
        case("la seconda è al limite: la terza", 6, vec![acc(6, 96.0, 30.0), acc(1, 97.0, 10.0), acc(3, 0.0, 10.0)], sw(6, 3, PrimaryAtLimit)),
        with_mem("sulla terza, torna libera la seconda: si passa alla seconda", 3, vec![acc(6, 97.0, 30.0), acc(1, 5.0, 30.0), acc(3, 20.0, 10.0)], one_reset.clone(), sw(3, 1, EarlierAvailable)),
        with_mem("sulla terza, libere prima e seconda: la prima", 3, vec![acc(6, 5.0, 30.0), acc(1, 5.0, 30.0), acc(3, 20.0, 10.0)], both_reset.clone(), back(3, PrimaryReset)),
        with_mem("sulla seconda, la prima resettata: la prima", 1, vec![acc(6, 5.0, 30.0), acc(1, 20.0, 30.0), acc(3, 0.0, 10.0)], both_reset.clone(), back(1, PrimaryReset)),
        with_mem("isteresi: la seconda al 90% non basta", 3, vec![acc(6, 97.0, 30.0), acc(1, 90.0, 30.0), acc(3, 20.0, 10.0)], one_reset.clone(), idle(OnReserve)),
        with_mem("isteresi solo sulla finestra che l'ha fatta lasciare", 3, vec![acc(6, 97.0, 30.0), acc(1, 90.0, 85.0), acc(3, 20.0, 10.0)], seven.clone(), sw(3, 1, EarlierAvailable)),
        with_mem("lasciata per la 7d: 7d al 90% resta", 3, vec![acc(6, 97.0, 30.0), acc(1, 5.0, 90.0), acc(3, 20.0, 10.0)], seven, idle(OnReserve)),
        with_mem("reset della seconda non passato", 3, vec![acc(6, 97.0, 30.0), acc(1, 5.0, 30.0), acc(3, 20.0, 10.0)], mem_left(vec![left(1, NOW + 60, five.clone())]), idle(OnReserve)),
        with_mem("pausa minima anche al ritorno", 3, vec![acc(6, 97.0, 30.0), acc(1, 5.0, 30.0), acc(3, 20.0, 10.0)], EngineMemory { last_switch_at: Some(NOW - 10), ..one_reset.clone() }, idle(Cooldown)),
        with_mem("terza al limite, precedenti sopra l'isteresi: la seconda sotto soglia", 3, vec![acc(6, 97.0, 30.0), acc(1, 90.0, 30.0), acc(3, 96.0, 10.0)], one_reset.clone(), sw(3, 1, ReserveAtLimit)),
        with_mem("terza al limite, solo la principale sotto soglia", 3, vec![acc(6, 90.0, 30.0), acc(1, 97.0, 30.0), acc(3, 96.0, 10.0)], both_reset, back(3, ReservesAtLimit)),
        case("tutte al limite", 3, vec![acc(6, 97.0, 30.0), acc(1, 97.0, 30.0), acc(3, 96.0, 10.0)], susp(SuspendReason::NoViableTarget)),
        with_mem("seconda in re-login: non si torna", 3, vec![acc(6, 97.0, 30.0), status(acc(1, 5.0, 30.0), UsageStatus::ReloginRequired), acc(3, 20.0, 10.0)], one_reset.clone(), idle(OnReserve)),
    ];
    let flow = EngineRules { enabled: true, order: Some(vec![6, 1, 3]), reserve_criterion: ReserveCriterion::FixedOrder, ..EngineRules::default() };
    run_cases(&flow, cases);

    // Senza ritorno: si resta finché l'attiva non supera la soglia.
    let no_return = EngineRules { return_to_primary: false, ..flow.clone() };
    run_cases(&no_return, vec![
        with_mem("ritorno spento", 3, vec![acc(6, 5.0, 30.0), acc(1, 5.0, 30.0), acc(3, 20.0, 10.0)], one_reset.clone(), idle(OnReserve)),
    ]);
    // Con gli altri criteri il ritorno va solo alla principale.
    let weekly = EngineRules { reserve_criterion: ReserveCriterion::LowestWeekly, ..flow.clone() };
    run_cases(&weekly, vec![
        with_mem("altri criteri: la seconda libera non basta", 3, vec![acc(6, 97.0, 30.0), acc(1, 5.0, 30.0), acc(3, 20.0, 10.0)], one_reset, idle(OnReserve)),
    ]);
    // Numeri che non esistono in cswap: si saltano (9 non c'è, la principale diventa 6).
    let missing = EngineRules { order: Some(vec![9, 6, 1]), ..flow };
    let l = list(Some(6), vec![acc(6, 1.0, 1.0), acc(1, 1.0, 1.0)]);
    assert_eq!(missing.missing_accounts(&l), vec![9]);
    run_cases(&missing, vec![
        case("principale assente: la prima che esiste", 6, vec![acc(6, 96.0, 30.0), acc(1, 0.0, 10.0)], sw(6, 1, PrimaryAtLimit)),
        case("ritorno alla prima che esiste", 1, vec![acc(6, 5.0, 30.0), acc(1, 20.0, 10.0)], back(1, PrimaryReset)),
    ]);
}

/// Giro per giro: la seconda oscilla intorno alla soglia, poi si resetta. La principale
/// resta al limite della 7d tutto il tempo.
#[test]
fn priority_flow_without_oscillation() {
    let rules = EngineRules {
        enabled: true,
        order: Some(vec![6, 1, 3]),
        reserve_criterion: ReserveCriterion::FixedOrder,
        ..EngineRules::default()
    };
    let r1 = H5_RESET; // reset 5h della seconda
    // (minuti, % 5h della seconda, account attivo atteso dopo il giro)
    let script: &[(i64, f64, u32)] = &[
        (0, 94.0, 1),
        (1, 95.0, 3), // lascia la seconda
        (2, 94.0, 3), // sotto soglia ma reset non passato
        (10, 80.0, 3),
        (61, 88.0, 3), // reset passato, ma sopra soglia meno isteresi
        (62, 84.0, 1), // torna alla seconda
        (63, 95.0, 1), // pausa minima
        (70, 95.0, 3), // la lascia di nuovo
        (71, 50.0, 3), // il nuovo reset è tra 5 ore
    ];
    let mut mem = EngineMemory::default();
    let mut active = 1;
    let mut switches = 0;
    for &(min, p5, expect) in script {
        let now = NOW + min * 60;
        let reset = if now >= r1 { r1 + 5 * 3600 } else { r1 };
        let mut second = acc(1, p5, 30.0);
        second.usage.as_mut().unwrap().five_hour = Some(window(p5, reset));
        let l = list(
            Some(active),
            vec![acc(6, 10.0, 99.0), second, acc(3, 10.0, 10.0)],
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
        assert_eq!(active, expect, "minuto {min}");
    }
    assert_eq!(switches, 3);
}

#[test]
fn reasons_and_current_candidate() {
    let rules = rules_with(ReserveCriterion::ResetSoonest);
    let accounts = |p5: f64| {
        vec![
            alias(week(6, p5, 30.0, 5.0), "links-enrico"),
            alias(week(1, 0.0, 30.0, 1.0), "links-erica"),
            week(2, 0.0, 10.0, 3.0),
        ]
    };
    let eval = |active: u32, accounts: Vec<Account>, mem: &EngineMemory| {
        let l = list(Some(active), accounts);
        let obs = Observation {
            list: &l,
            conflicts: &[],
        };
        evaluate(obs, &rules, mem, NOW)
    };
    let mem = EngineMemory::default();
    let e = eval(6, accounts(95.0), &mem);
    assert_eq!(e.decision, at_limit_to(1));
    assert_eq!(
        e.reason,
        "5h di links-enrico al 95%: passo a links-erica, reset settimanale più vicino (tra 24 h)"
    );
    // Sotto soglia: nessun cambio, ma il candidato attuale c'è.
    let e = eval(6, accounts(40.0), &mem);
    assert_eq!(e.decision, idle(IdleReason::BelowThreshold));
    assert_eq!(e.reason, "links-enrico sotto soglia (5h al 40%, 7d al 30%)");
    assert_eq!(
        e.candidate,
        Some(Candidate {
            number: 1,
            criterion: ReserveCriterion::ResetSoonest,
            fallback: false,
            why: "reset settimanale più vicino (tra 24 h)".to_owned(),
        })
    );
    // Su una riserva, il candidato è l'altra riserva; senza alias si usa il numero.
    let e = eval(1, accounts(97.0), &mem);
    assert_eq!(e.candidate.as_ref().map(|c| c.number), Some(2));
    assert_eq!(
        e.reason,
        "su links-erica: links-enrico non è ancora di nuovo libera"
    );
    // Fuori dalla lista: nessun candidato.
    let e = eval(4, vec![acc(6, 1.0, 1.0), acc(4, 1.0, 1.0)], &mem);
    assert_eq!(e.candidate, None);
    assert_eq!(e.reason, "#4 non è nella lista delle regole");
    // Pausa minima e motore spento.
    let cooldown = EngineMemory {
        last_switch_at: Some(NOW - 100),
        ..EngineMemory::default()
    };
    let e = eval(6, accounts(96.0), &cooldown);
    assert_eq!(e.reason, "pausa minima tra due cambi: ancora 4 min");
    let off = EngineRules {
        enabled: false,
        ..rules.clone()
    };
    let l = list(Some(6), accounts(96.0));
    let obs = Observation {
        list: &l,
        conflicts: &[],
    };
    let e = evaluate(obs, &off, &mem, NOW);
    assert_eq!(e.reason, "motore spento");
    assert_eq!(
        e.candidate.map(|c| c.number),
        Some(1),
        "candidato anche a motore spento"
    );
}

#[test]
fn reasons_never_contain_emails() {
    let rules = user_rules();
    let scenarios = [
        (6, vec![acc(6, 96.0, 30.0), acc(1, 0.0, 40.0)]),
        (6, vec![acc(6, 96.0, 30.0)]),
        (1, vec![acc(6, 5.0, 30.0), acc(1, 0.0, 40.0)]),
        (1, vec![acc(6, 90.0, 30.0), acc(1, 99.0, 40.0)]),
        (2, vec![acc(6, 5.0, 30.0), acc(2, 0.0, 40.0)]),
    ];
    for (active, accounts) in scenarios {
        let l = list(Some(active), accounts);
        let obs = Observation {
            list: &l,
            conflicts: &[],
        };
        let e = evaluate(obs, &rules, &EngineMemory::default(), NOW);
        assert!(!e.reason.contains('@'), "{}", e.reason);
        assert!(!e.reason.is_empty());
    }
}

#[test]
fn rules_migration_from_primary_and_reserves() {
    let old: EngineRules =
        serde_json::from_str(r#"{"enabled":true,"primary":6,"reserves":[1,3]}"#).unwrap();
    assert_eq!(old.order, None);
    assert_eq!(old.configured_order(), vec![6, 1, 3]);
    let mut synced = old.clone();
    synced.sync_order();
    assert_eq!(synced.order, Some(vec![6, 1, 3]));
    let new: EngineRules = serde_json::from_str(r#"{"enabled":true,"order":[6,1,3]}"#).unwrap();
    assert!(new.validate().is_empty(), "basta la lista");
    let l = list(
        Some(6),
        vec![acc(6, 96.0, 30.0), acc(1, 0.0, 40.0), acc(3, 0.0, 20.0)],
    );
    let obs = Observation {
        list: &l,
        conflicts: &[],
    };
    let mem = EngineMemory::default();
    assert_eq!(decide(obs, &old, &mem, NOW), decide(obs, &new, &mem, NOW));
    let mut mirrored = new.clone();
    mirrored.sync_order();
    assert_eq!((mirrored.primary, mirrored.reserves), (Some(6), vec![1, 3]));
    // Forma precedente scritta dall'interfaccia attuale: prevale su `order`.
    let ui: EngineRules =
        serde_json::from_str(r#"{"order":[6,1,3],"primary":1,"reserves":[3]}"#).unwrap();
    assert_eq!(ui.configured_order(), vec![1, 3]);
    // Bozza senza principale: le riserve restano.
    let mut draft = EngineRules {
        reserves: vec![1],
        ..EngineRules::default()
    };
    draft.sync_order();
    assert_eq!((draft.order, draft.reserves), (None, vec![1]));
    // Le regole vecchie si rileggono identiche (nessun campo nuovo obbligatorio).
    let text = serde_json::to_string(&EngineRules::default()).unwrap();
    assert!(!text.contains("order") && !text.contains("scoreWeights"));
}

#[test]
fn legacy_memory_is_absorbed_on_switch() {
    let rules = user_rules();
    let mut mem: EngineMemory = serde_json::from_str(
        r#"{"lastSwitchAt":1,"leftPrimaryAt":5,"primaryResetAt":7,"primaryLeftWindows":["sevenDay"]}"#,
    )
    .unwrap();
    let l = list(
        Some(1),
        vec![acc(6, 0.0, 0.0), acc(1, 99.0, 0.0), acc(3, 0.0, 0.0)],
    );
    mem.record_switch(&rules, &l, 3, NOW);
    assert_eq!(
        mem.left_accounts,
        vec![
            LeftAccount {
                number: 6,
                left_at: 5,
                reset_at: Some(7),
                windows: vec![LimitWindow::SevenDay],
            },
            LeftAccount {
                number: 1,
                left_at: NOW,
                reset_at: Some(H5_RESET),
                windows: vec![LimitWindow::FiveHour],
            },
        ]
    );
    assert_eq!(
        (
            mem.left_primary_at,
            mem.primary_reset_at,
            mem.primary_left_windows.clone()
        ),
        (None, None, None)
    );
    // Ritorno alla principale: la sua memoria si toglie.
    let l = list(Some(3), vec![acc(6, 0.0, 0.0), acc(3, 0.0, 0.0)]);
    mem.record_switch(&rules, &l, 6, NOW + 10);
    assert_eq!(
        mem.left_accounts
            .iter()
            .map(|r| r.number)
            .collect::<Vec<_>>(),
        [1, 3]
    );
}
