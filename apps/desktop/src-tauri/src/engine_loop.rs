//! Motore dentro l'app: il ciclo di lettura esegue un giro del motore di swapdeck-core
//! (`headless::Engine`). Con il lock: storico, battito, decisione e cambio; senza lock (un'altra
//! istanza o `--headless` lo tiene) solo osservazione. Il motore resta spento finché la sezione
//! `engine` della config non lo accende. Regole e intervallo si rileggono a ogni giro: una
//! modifica dalla UI vale dal giro successivo, senza riavvio.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use swapdeck_core::conflicts::{Conflict, ConflictSource, SystemConflicts};
use swapdeck_core::cswap::{CswapAction, CswapError, CswapErrorKind, ListResponse, SwitchResponse};
use swapdeck_core::engine::EngineRules;
pub use swapdeck_core::engine::EngineView;
use swapdeck_core::headless::{CswapPort, Engine, RunOptions, Runtime, Step, SystemClock};

use crate::api::CswapApi;
use crate::poller::Loop;

/// Vista del motore dopo un giro; `None` se il giro non ha deciso nulla (lettura fallita):
/// resta la vista precedente.
pub fn view_of(rules: &EngineRules, step: &Step) -> Option<EngineView> {
    let decision = step.decision.as_ref()?;
    Some(EngineView::new(step.holder, rules, Some(decision)))
}

/// cswap visto dal motore attraverso l'interfaccia dell'app (stesso lock di chiamata dei comandi).
struct ApiPort(Arc<dyn CswapApi>);

impl CswapPort for ApiPort {
    fn list(&mut self) -> Result<ListResponse, CswapError> {
        self.0.list()
    }

    fn switch_to(&mut self, number: u32) -> Result<SwitchResponse, CswapError> {
        let action = CswapAction::Switch {
            target: Some(number),
            strategy: None,
            models: None,
            force: false,
        };
        self.0
            .execute(&action)?
            .switch
            .ok_or_else(|| CswapError::new(CswapErrorKind::Failed, "switch senza risposta JSON"))
    }
}

/// Conflitti letti solo con il motore acceso o in osservazione: spento non servono e la
/// lettura delle attività pianificate costa. Ogni esito va a `report` (vuoto se spento).
struct GatedConflicts<S: ConflictSource, R: Fn(&[Conflict])> {
    inner: S,
    active: Rc<Cell<bool>>,
    report: R,
}

impl<S: ConflictSource, R: Fn(&[Conflict])> ConflictSource for GatedConflicts<S, R> {
    fn detect(&mut self) -> Vec<Conflict> {
        let found = if self.active.get() {
            self.inner.detect()
        } else {
            Vec::new()
        };
        (self.report)(&found);
        found
    }
}

fn interval_secs(interval: Duration) -> u32 {
    u32::try_from(interval.as_secs()).unwrap_or(u32::MAX)
}

/// Corpo del ciclo di lettura con il motore. `live` dà regole e intervallo correnti (letti a
/// ogni giro); `on_view` riceve lo stato del motore a ogni giro che decide, prima dell'esito
/// della lettura; `on_conflicts` i conflitti letti. Senza cartella dati: solo lettura.
pub fn run(
    lp: Loop,
    api: Arc<dyn CswapApi>,
    data_dir: Option<PathBuf>,
    live: impl Fn() -> (EngineRules, Duration),
    on_view: impl Fn(EngineView),
    on_conflicts: impl Fn(&[Conflict]),
) {
    run_with(
        lp,
        api,
        data_dir,
        (live, on_view, on_conflicts),
        SystemConflicts::default(),
    );
}

/// Come [`run`], con la fonte dei conflitti data (finta nei test).
fn run_with(
    lp: Loop,
    api: Arc<dyn CswapApi>,
    data_dir: Option<PathBuf>,
    (live, on_view, on_conflicts): (
        impl Fn() -> (EngineRules, Duration),
        impl Fn(EngineView),
        impl Fn(&[Conflict]),
    ),
    source: impl ConflictSource,
) {
    let Some(dir) = data_dir else {
        lp.run_paced(|| (api.list(), Some(live().1)));
        return;
    };
    let (mut rules, interval) = live();
    let mut opts = RunOptions::new(dir, rules.clone());
    opts.interval_secs = interval_secs(interval);
    let mut port = ApiPort(Arc::clone(&api));
    let active = Rc::new(Cell::new(rules.enabled || rules.dry_run));
    let mut conflicts = GatedConflicts {
        inner: source,
        active: Rc::clone(&active),
        report: on_conflicts,
    };
    let mut clock = SystemClock;
    let mut out = std::io::stderr();
    let stop = AtomicBool::new(false);
    let engine = Engine::new(
        &opts,
        Runtime {
            port: &mut port,
            conflicts: &mut conflicts,
            clock: &mut clock,
            out: &mut out,
            stop: &stop,
        },
    );
    let mut engine = match engine {
        Ok(e) => e,
        Err(e) => {
            eprintln!("motore non avviato ({e}): solo lettura");
            lp.run_paced(|| (api.list(), Some(live().1)));
            return;
        }
    };
    // Motore acceso: il primo giro attende la lettura delle attività pianificate (anche
    // decine di secondi). Prima una lettura semplice per la UI, poi subito il motore.
    let mut quick_first = rules.enabled || rules.dry_run;
    lp.run_paced(|| {
        if std::mem::take(&mut quick_first) {
            return (api.list(), Some(Duration::ZERO));
        }
        let (now_rules, interval) = live();
        if now_rules != rules {
            engine.set_rules(&now_rules);
            active.set(now_rules.enabled || now_rules.dry_run);
            rules = now_rules;
        }
        engine.set_interval(interval_secs(interval));
        let step = engine.step();
        if let Some(view) = view_of(&rules, &step) {
            on_view(view);
        }
        // Dopo un cambio la lista letta prima è vecchia: si rilegge.
        if step.switched_to.is_some() {
            return (api.list(), Some(interval));
        }
        (step.list, Some(interval))
    });
    engine.release();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::MockApi;
    use crate::poller::{Poller, REFRESH_WAIT};
    use std::sync::Mutex;
    use swapdeck_core::conflicts::ConflictKind;
    use swapdeck_core::engine::{Decision, SuspendReason};
    use swapdeck_core::timeutil::now_unix;

    #[test]
    fn view_marks_only_attention_suspensions() {
        let rules = EngineRules::default();
        let step = |decision| Step {
            list: Err(CswapError::new(CswapErrorKind::Failed, "x")),
            decision,
            holder: true,
            switched_to: None,
        };
        assert_eq!(view_of(&rules, &step(None)), None);
        let off: Decision =
            serde_json::from_str(r#"{"kind":"suspended","reason":"disabled","detail":[]}"#)
                .unwrap();
        let v = view_of(&rules, &step(Some(off))).unwrap();
        assert!(!v.paused());
        let conflict: Decision = serde_json::from_str(
            r#"{"kind":"suspended","reason":"conflict","detail":["cswap-guard"]}"#,
        )
        .unwrap();
        let v = view_of(&rules, &step(Some(conflict))).unwrap();
        assert!(v.paused());
        assert_eq!(v.detail, ["cswap-guard"]);
        let json = serde_json::to_value(&v).unwrap();
        assert_eq!(json["suspended"], "conflict");
        assert_eq!(json["dryRun"], false);
    }

    #[test]
    fn loop_holds_lock_records_history_and_releases() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().to_path_buf();
        let api: Arc<dyn CswapApi> = Arc::new(MockApi::new());
        let views = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&views);
        let (poller, handle) = Poller::spawn_with(
            Duration::from_secs(3600),
            |_| {},
            move |lp| {
                run(
                    lp,
                    api,
                    Some(data),
                    || (EngineRules::default(), Duration::from_secs(60)),
                    move |v| sink.lock().unwrap().push(v),
                    |_| {},
                );
            },
        );
        poller.refresh_wait(REFRESH_WAIT).unwrap();
        poller.stop();
        handle.join().unwrap();
        let views = views.lock().unwrap();
        assert!(views.iter().all(|v| v.holder && !v.paused()));
        assert_eq!(views[0].suspended, Some(SuspendReason::Disabled));
        let records = swapdeck_core::history::History::new(dir.path())
            .read_range(now_unix() - 3600, now_unix() + 3600)
            .unwrap();
        assert!(!records.is_empty(), "storico scritto da chi tiene il lock");
        assert!(
            !dir.path().join(swapdeck_core::lock::LOCK_FILE).exists(),
            "lock rilasciato all'arresto"
        );
    }

    struct FixedConflicts(Vec<Conflict>);

    impl ConflictSource for FixedConflicts {
        fn detect(&mut self) -> Vec<Conflict> {
            self.0.clone()
        }
    }

    /// Regole cambiate mentre il ciclo gira: valgono dal giro dopo, senza riavvio.
    #[test]
    fn rules_apply_on_the_next_round_without_restart() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().to_path_buf();
        let api: Arc<dyn CswapApi> = Arc::new(MockApi::new());
        let live = Arc::new(Mutex::new((EngineRules::default(), Duration::from_secs(60))));
        let views = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (live2, views2, seen2) = (Arc::clone(&live), Arc::clone(&views), Arc::clone(&seen));
        let guard = Conflict {
            id: "task:cswap-guard".into(),
            kind: ConflictKind::ScheduledTask,
            label: "cswap-guard".into(),
            certain: true,
        };
        let source = FixedConflicts(vec![guard]);
        let (poller, handle) = Poller::spawn_with(
            Duration::from_secs(3600),
            |_| {},
            move |lp| {
                run_with(
                    lp,
                    api,
                    Some(data),
                    (
                        move || live2.lock().unwrap().clone(),
                        move |v| views2.lock().unwrap().push(v),
                        move |c: &[Conflict]| seen2.lock().unwrap().push(c.len()),
                    ),
                    source,
                );
            },
        );
        poller.refresh_wait(REFRESH_WAIT).unwrap();
        *live.lock().unwrap() = (
            EngineRules {
                enabled: true,
                primary: Some(6),
                ..EngineRules::default()
            },
            Duration::from_secs(120),
        );
        poller.refresh_wait(REFRESH_WAIT).unwrap();
        poller.stop();
        handle.join().unwrap();
        let views = views.lock().unwrap();
        assert_eq!(views[0].suspended, Some(SuspendReason::Disabled));
        let last = views.last().unwrap();
        assert!(last.enabled, "regole nuove applicate");
        assert_eq!(last.suspended, Some(SuspendReason::Conflict));
        assert_eq!(last.detail, ["task:cswap-guard"]);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.first(), Some(&0), "spento: conflitti non letti");
        assert_eq!(seen.last(), Some(&1));
    }
}
