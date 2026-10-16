//! Motore dentro l'app: il ciclo di lettura esegue un giro del motore di swapdeck-core
//! (`headless::Engine`). Con il lock: storico, battito, decisione e cambio; senza lock (un'altra
//! istanza o `--headless` lo tiene) solo osservazione. Il motore resta spento finché la sezione
//! `engine` della config non lo accende.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::Serialize;
use swapdeck_core::conflicts::{Conflict, ConflictSource, SystemConflicts};
use swapdeck_core::cswap::{CswapAction, CswapError, CswapErrorKind, ListResponse, SwitchResponse};
use swapdeck_core::engine::{Decision, EngineRules, SuspendReason};
use swapdeck_core::headless::{CswapPort, Engine, RunOptions, Runtime, Step, SystemClock};

use crate::api::CswapApi;
use crate::poller::Loop;

/// Stato del motore mostrato nella UI (parte dello `Snapshot`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineView {
    /// Il lock è di questa app: storico e cambi sono suoi.
    pub holder: bool,
    pub enabled: bool,
    pub dry_run: bool,
    /// Motivo della sospensione (anche `disabled` con il motore spento).
    pub suspended: Option<SuspendReason>,
    /// Dettagli della sospensione (per esempio gli id dei conflitti).
    pub detail: Vec<String>,
}

impl EngineView {
    /// `None` se il giro non ha deciso nulla (lettura fallita): resta la vista precedente.
    pub fn from_step(rules: &EngineRules, step: &Step) -> Option<Self> {
        let decision = step.decision.as_ref()?;
        let (suspended, detail) = match decision {
            Decision::Suspended { reason, detail } => (Some(*reason), detail.clone()),
            _ => (None, Vec::new()),
        };
        Some(Self {
            holder: step.holder,
            enabled: rules.enabled,
            dry_run: rules.dry_run,
            suspended,
            detail,
        })
    }

    /// Sospeso per un motivo che chiede attenzione (non il motore spento): variante «sospeso».
    pub fn paused(&self) -> bool {
        matches!(self.suspended, Some(r) if r != SuspendReason::Disabled)
    }
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
/// lettura delle attività pianificate costa.
struct GatedConflicts {
    inner: SystemConflicts,
    active: bool,
}

impl ConflictSource for GatedConflicts {
    fn detect(&mut self) -> Vec<Conflict> {
        if self.active {
            self.inner.detect()
        } else {
            Vec::new()
        }
    }
}

/// Corpo del ciclo di lettura con il motore. `on_view` riceve lo stato del motore a ogni
/// giro che decide, prima dell'esito della lettura. Senza cartella dati: solo lettura.
pub fn run(
    lp: Loop,
    api: Arc<dyn CswapApi>,
    data_dir: Option<PathBuf>,
    rules: EngineRules,
    interval: Duration,
    on_view: impl Fn(EngineView),
) {
    let Some(dir) = data_dir else {
        lp.run(|| api.list());
        return;
    };
    let mut opts = RunOptions::new(dir, rules.clone());
    opts.interval_secs = u32::try_from(interval.as_secs()).unwrap_or(u32::MAX);
    let mut port = ApiPort(Arc::clone(&api));
    let mut conflicts = GatedConflicts {
        inner: SystemConflicts::default(),
        active: rules.enabled || rules.dry_run,
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
            lp.run(|| api.list());
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
        let step = engine.step();
        if let Some(view) = EngineView::from_step(&rules, &step) {
            on_view(view);
        }
        // Dopo un cambio la lista letta prima è vecchia: si rilegge.
        if step.switched_to.is_some() {
            return (api.list(), None);
        }
        (step.list, None)
    });
    engine.release();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::MockApi;
    use crate::poller::{Poller, REFRESH_WAIT};
    use std::sync::Mutex;
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
        assert_eq!(EngineView::from_step(&rules, &step(None)), None);
        let off: Decision =
            serde_json::from_str(r#"{"kind":"suspended","reason":"disabled","detail":[]}"#)
                .unwrap();
        let v = EngineView::from_step(&rules, &step(Some(off))).unwrap();
        assert!(!v.paused());
        let conflict: Decision = serde_json::from_str(
            r#"{"kind":"suspended","reason":"conflict","detail":["cswap-guard"]}"#,
        )
        .unwrap();
        let v = EngineView::from_step(&rules, &step(Some(conflict))).unwrap();
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
                    EngineRules::default(),
                    Duration::from_secs(60),
                    move |v| sink.lock().unwrap().push(v),
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
}
