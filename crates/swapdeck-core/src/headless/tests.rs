//! Runner con finto cswap in memoria: mai il cswap reale.

use super::*;
use crate::conflicts::ConflictKind;
use crate::cswap::{AccountRef, CswapErrorKind, SwitchReason, SwitchStrategyUsed};
use crate::engine::tests::{acc, list, user_rules};

const T0: i64 = 1_800_000_000;

struct FakePort {
    state: ListResponse,
    switches: Vec<u32>,
    fail_list: bool,
}

impl FakePort {
    fn new(state: ListResponse) -> Self {
        Self {
            state,
            switches: Vec::new(),
            fail_list: false,
        }
    }
}

impl CswapPort for FakePort {
    fn list(&mut self) -> Result<ListResponse, CswapError> {
        if self.fail_list {
            return Err(CswapError::new(CswapErrorKind::Timeout, "tempo scaduto"));
        }
        Ok(self.state.clone())
    }

    fn switch_to(&mut self, number: u32) -> Result<SwitchResponse, CswapError> {
        let from = self.state.active_account_number;
        self.switches.push(number);
        self.state.active_account_number = Some(number);
        for a in &mut self.state.accounts {
            a.active = a.number == number;
        }
        Ok(SwitchResponse {
            schema_version: 1,
            switched: true,
            from: from.map(|n| AccountRef {
                number: Some(n),
                email: String::new(),
            }),
            to: Some(AccountRef {
                number: Some(number),
                email: String::new(),
            }),
            strategy: SwitchStrategyUsed::Direct,
            reason: SwitchReason::Switched,
            message: String::new(),
            warnings: Vec::new(),
            models: None,
            model_source: None,
        })
    }
}

struct FakeClock(i64);

impl Clock for FakeClock {
    fn now(&mut self) -> i64 {
        self.0
    }
    fn sleep(&mut self, secs: u32, _stop: &AtomicBool) {
        self.0 += i64::from(secs);
    }
}

struct FakeConflicts(Vec<Conflict>);

impl ConflictSource for FakeConflicts {
    fn detect(&mut self) -> Vec<Conflict> {
        self.0.clone()
    }
}

struct Outcome {
    result: RunOutcome,
    port: FakePort,
    output: String,
}

fn run_with(
    dir: &Path,
    rules: EngineRules,
    port: FakePort,
    conflicts: Vec<Conflict>,
    tweak: impl FnOnce(&mut RunOptions),
) -> Outcome {
    let mut opts = RunOptions::new(dir.to_path_buf(), rules);
    opts.once = true;
    tweak(&mut opts);
    let mut port = port;
    let mut out = Vec::new();
    let stop = AtomicBool::new(false);
    let result = run(
        &opts,
        Runtime {
            port: &mut port,
            conflicts: &mut FakeConflicts(conflicts),
            clock: &mut FakeClock(T0),
            out: &mut out,
            stop: &stop,
        },
    )
    .unwrap();
    Outcome {
        result,
        port,
        output: String::from_utf8(out).unwrap(),
    }
}

fn at_limit() -> FakePort {
    FakePort::new(list(
        Some(6),
        vec![acc(6, 96.0, 30.0), acc(1, 0.0, 40.0), acc(3, 0.0, 20.0)],
    ))
}

fn history(dir: &Path) -> Vec<HistoryRecord> {
    History::new(dir).read_range(T0 - 10, T0 + 86_400).unwrap()
}

#[test]
fn once_switches_and_records() {
    let dir = tempfile::tempdir().unwrap();
    let o = run_with(dir.path(), user_rules(), at_limit(), Vec::new(), |_| {});
    assert_eq!(
        o.result,
        RunOutcome::Finished {
            ticks: 1,
            switches: 1
        }
    );
    assert_eq!(o.port.switches, vec![3]);
    assert!(o.output.contains("CAMBIO ESEGUITO 6 -> 3"), "{}", o.output);
    let rows = history(dir.path());
    let samples = rows
        .iter()
        .filter(|r| matches!(r, HistoryRecord::Sample(_)))
        .count();
    assert_eq!(samples, 3);
    assert!(
        rows.iter()
            .any(|r| matches!(r, HistoryRecord::Switch(s) if s.switched && s.to == 3))
    );
    let mem = load_memory(dir.path());
    assert_eq!(mem.last_switch_at, Some(T0));
    assert!(mem.primary_reset_at.is_some());
    assert!(
        !dir.path().join(lock::LOCK_FILE).exists(),
        "lock rilasciato"
    );
}

#[test]
fn dry_run_and_disabled_never_switch() {
    let dir = tempfile::tempdir().unwrap();
    let o = run_with(dir.path(), user_rules(), at_limit(), Vec::new(), |o| {
        o.dry_run = true;
    });
    assert!(o.port.switches.is_empty());
    assert!(
        o.output.contains("[solo osservazione] cambio 6 -> 3"),
        "{}",
        o.output
    );
    assert!(history(dir.path()).iter().any(|r| matches!(
        r,
        HistoryRecord::Decision(d) if d.dry_run && d.decision.target() == Some(3)
    )));

    let dir = tempfile::tempdir().unwrap();
    let rules = EngineRules {
        primary: Some(6),
        reserves: vec![1, 3],
        ..EngineRules::default()
    };
    let o = run_with(dir.path(), rules, at_limit(), Vec::new(), |_| {});
    assert!(o.port.switches.is_empty());
    assert!(o.output.contains("sospeso: motore spento"));
    assert!(
        !history(dir.path()).is_empty(),
        "lo storico si scrive anche a motore spento"
    );
}

#[test]
fn conflict_suspends() {
    let dir = tempfile::tempdir().unwrap();
    let guard = Conflict {
        id: "task:cswap-guard".to_owned(),
        kind: ConflictKind::ScheduledTask,
        label: "attività pianificata \\cswap-guard".to_owned(),
        certain: true,
    };
    let tui = Conflict {
        id: "process:tui".to_owned(),
        kind: ConflictKind::Process,
        label: "cswap tui (PID 7)".to_owned(),
        certain: false,
    };
    let o = run_with(
        dir.path(),
        user_rules(),
        at_limit(),
        vec![guard, tui],
        |_| {},
    );
    assert!(o.port.switches.is_empty());
    assert!(
        o.output
            .contains("sospeso: altro motore attivo (task:cswap-guard)")
    );
    assert!(o.output.contains("possibile conflitto con cswap tui"));
}

#[test]
fn once_with_busy_lock_and_waiting_loop() {
    let dir = tempfile::tempdir().unwrap();
    let holder = match lock::try_acquire(dir.path(), 60, T0).unwrap() {
        Acquire::Acquired(g) => g,
        Acquire::Busy(_) => unreachable!(),
    };
    let o = run_with(dir.path(), user_rules(), at_limit(), Vec::new(), |_| {});
    assert!(matches!(o.result, RunOutcome::LockBusy(Some(_))));
    assert!(o.port.switches.is_empty());

    // Senza --once resta in attesa: osserva ma non cambia e non scrive lo storico.
    let o = run_with(dir.path(), user_rules(), at_limit(), Vec::new(), |o| {
        o.once = false;
        o.max_ticks = Some(2);
    });
    assert!(o.port.switches.is_empty());
    assert!(o.output.contains("in attesa del lock"), "{}", o.output);
    assert!(history(dir.path()).is_empty());
    assert!(holder.still_owned());
}

#[test]
fn loop_keeps_going_after_errors_and_respects_memory() {
    let dir = tempfile::tempdir().unwrap();
    let mut port = at_limit();
    port.fail_list = true;
    let o = run_with(dir.path(), user_rules(), port, Vec::new(), |o| {
        o.once = false;
        o.max_ticks = Some(3);
    });
    assert_eq!(
        o.result,
        RunOutcome::Finished {
            ticks: 3,
            switches: 0
        }
    );
    assert_eq!(o.output.matches("cswap list non riuscito").count(), 3);

    // Tre giri: cambio al primo, poi su riserva senza altri cambi.
    let o = run_with(dir.path(), user_rules(), at_limit(), Vec::new(), |o| {
        o.once = false;
        o.max_ticks = Some(3);
    });
    assert_eq!(o.port.switches, vec![3]);
    assert!(o.output.contains("ok: su riserva"), "{}", o.output);
}

/// Orologio che al primo `sleep` rilascia il lock di un'altra istanza.
struct ReleasingClock {
    t: i64,
    holder: Option<LockGuard>,
}

impl Clock for ReleasingClock {
    fn now(&mut self) -> i64 {
        self.t
    }
    fn sleep(&mut self, secs: u32, _stop: &AtomicBool) {
        self.t += i64::from(secs);
        self.holder.take();
    }
}

#[test]
fn decision_is_recorded_again_after_acquiring_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let holder = match lock::try_acquire(dir.path(), 60, T0).unwrap() {
        Acquire::Acquired(g) => g,
        Acquire::Busy(_) => unreachable!(),
    };
    let mut opts = RunOptions::new(dir.path().to_path_buf(), user_rules());
    opts.dry_run = true;
    opts.max_ticks = Some(2);
    let mut port = at_limit();
    let mut out = Vec::new();
    let stop = AtomicBool::new(false);
    run(
        &opts,
        Runtime {
            port: &mut port,
            conflicts: &mut FakeConflicts(Vec::new()),
            clock: &mut ReleasingClock {
                t: T0,
                holder: Some(holder),
            },
            out: &mut out,
            stop: &stop,
        },
    )
    .unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("lock del motore acquisito"), "{out}");
    assert!(
        history(dir.path())
            .iter()
            .any(|r| matches!(r, HistoryRecord::Decision(d) if d.decision.target() == Some(3))),
        "la decisione vista in attesa va nello storico: {out}"
    );
}

/// Registra l'ordine delle letture: i conflitti vanno letti prima di `list`.
struct OrderConflicts(std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>);

impl ConflictSource for OrderConflicts {
    fn detect(&mut self) -> Vec<Conflict> {
        self.0.borrow_mut().push("conflicts");
        Vec::new()
    }
}

struct OrderPort(FakePort, std::rc::Rc<std::cell::RefCell<Vec<&'static str>>>);

impl CswapPort for OrderPort {
    fn list(&mut self) -> Result<ListResponse, CswapError> {
        self.1.borrow_mut().push("list");
        self.0.list()
    }
    fn switch_to(&mut self, number: u32) -> Result<SwitchResponse, CswapError> {
        self.0.switch_to(number)
    }
}

#[test]
fn conflicts_are_read_before_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut opts = RunOptions::new(dir.path().to_path_buf(), user_rules());
    opts.once = true;
    let order = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut port = OrderPort(at_limit(), order.clone());
    let stop = AtomicBool::new(false);
    run(
        &opts,
        Runtime {
            port: &mut port,
            conflicts: &mut OrderConflicts(order.clone()),
            clock: &mut FakeClock(T0),
            out: &mut Vec::new(),
            stop: &stop,
        },
    )
    .unwrap();
    assert_eq!(*order.borrow(), vec!["conflicts", "list"]);
}

#[test]
fn stop_flag_ends_after_current_tick() {
    let dir = tempfile::tempdir().unwrap();
    let opts = RunOptions::new(dir.path().to_path_buf(), user_rules());
    let mut port = at_limit();
    let stop = AtomicBool::new(true);
    let mut out = Vec::new();
    let result = run(
        &opts,
        Runtime {
            port: &mut port,
            conflicts: &mut FakeConflicts(Vec::new()),
            clock: &mut FakeClock(T0),
            out: &mut out,
            stop: &stop,
        },
    )
    .unwrap();
    assert_eq!(
        result,
        RunOutcome::Finished {
            ticks: 1,
            switches: 1
        }
    );
    assert!(!dir.path().join(lock::LOCK_FILE).exists());
}

#[test]
fn parses_cli_flags() {
    let a = |s: &[&str]| s.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        parse_flags(&a(&["--once", "--dry-run"])),
        Ok(CliFlags {
            once: true,
            dry_run: true
        })
    );
    assert!(parse_flags(&a(&["--twice"])).is_err());
    assert_eq!(
        cli_main(&a(&["--bad"]), &AtomicBool::new(false)),
        exit_code::USAGE
    );
}

#[test]
fn engine_steps_switch_only_with_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let opts = RunOptions::new(dir.path().to_path_buf(), user_rules());
    let stop = AtomicBool::new(false);
    let (mut out_a, mut out_b) = (Vec::new(), Vec::new());
    let (mut port_a, mut port_b) = (at_limit(), at_limit());
    let (mut conf_a, mut conf_b) = (FakeConflicts(Vec::new()), FakeConflicts(Vec::new()));
    let (mut clock_a, mut clock_b) = (FakeClock(T0), FakeClock(T0));
    let mut a = Engine::new(
        &opts,
        Runtime {
            port: &mut port_a,
            conflicts: &mut conf_a,
            clock: &mut clock_a,
            out: &mut out_a,
            stop: &stop,
        },
    )
    .unwrap();
    let step = a.step();
    assert!(step.holder);
    assert_eq!(step.switched_to, Some(3));
    assert!(step.list.is_ok());
    let mut b = Engine::new(
        &opts,
        Runtime {
            port: &mut port_b,
            conflicts: &mut conf_b,
            clock: &mut clock_b,
            out: &mut out_b,
            stop: &stop,
        },
    )
    .unwrap();
    // Senza lock: osserva e decide, ma non cambia.
    let step = b.step();
    assert!(!step.holder);
    assert_eq!(step.switched_to, None);
    assert!(step.decision.is_some());
    a.release();
    // Lock rilasciato: il secondo motore lo prende al giro dopo.
    assert!(b.step().holder);
    drop(b);
    assert!(port_b.switches.len() <= 1);
    assert_eq!(port_a.switches, vec![3]);
    assert!(
        !dir.path().join(lock::LOCK_FILE).exists(),
        "lock rilasciato al drop"
    );
    assert!(
        String::from_utf8(out_b)
            .unwrap()
            .contains("in attesa del lock")
    );
}

#[test]
fn engine_applies_new_rules_and_interval_without_restart() {
    let dir = tempfile::tempdir().unwrap();
    let off = EngineRules::default();
    let opts = RunOptions::new(dir.path().to_path_buf(), off);
    let stop = AtomicBool::new(false);
    let mut out = Vec::new();
    let mut port = at_limit();
    let mut conflicts = FakeConflicts(Vec::new());
    let mut clock = FakeClock(T0);
    let mut engine = Engine::new(
        &opts,
        Runtime {
            port: &mut port,
            conflicts: &mut conflicts,
            clock: &mut clock,
            out: &mut out,
            stop: &stop,
        },
    )
    .unwrap();
    let step = engine.step();
    assert!(matches!(
        step.decision,
        Some(Decision::Suspended {
            reason: SuspendReason::Disabled,
            ..
        })
    ));
    assert_eq!(step.switched_to, None);
    engine.set_rules(&user_rules());
    engine.set_interval(10_000);
    assert!(engine.rules().enabled);
    let step = engine.step();
    assert_eq!(step.switched_to, Some(3), "regole nuove al giro dopo");
    let text = std::fs::read_to_string(dir.path().join(lock::LOCK_FILE)).unwrap();
    let info: LockInfo = serde_json::from_str(&text).unwrap();
    assert_eq!(
        info.interval_secs, MAX_POLL_INTERVAL_SECONDS,
        "battito con l'intervallo nuovo"
    );
    drop(engine);
    assert_eq!(port.switches, vec![3]);
}

#[test]
fn run_rereads_rules_from_the_config_before_each_tick() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(config::CONFIG_FILE);
    let saved = config::AppConfig {
        engine: user_rules(),
        ..config::AppConfig::default()
    };
    config::save(&path, &saved).unwrap();
    let off = EngineRules::default();
    let o = run_with(dir.path(), off.clone(), at_limit(), Vec::new(), |o| {
        o.config_file = Some(path.clone());
    });
    assert_eq!(
        o.port.switches,
        [3],
        "regole della config, non quelle di avvio"
    );
    assert!(
        o.output.contains("regole del motore aggiornate"),
        "{}",
        o.output
    );

    std::fs::write(&path, "{ rotto").unwrap();
    let o = run_with(dir.path(), off, at_limit(), Vec::new(), |o| {
        o.config_file = Some(path.clone());
        o.once = false;
        o.max_ticks = Some(2);
    });
    assert!(o.port.switches.is_empty(), "restano le regole di prima");
    assert_eq!(
        o.output.matches("config non riletta").count(),
        1,
        "errore scritto una volta: {}",
        o.output
    );
}
