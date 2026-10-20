//! Comandi `invoke` di F21 «Rifai login» (`ReloginBackend` della UI). Il flusso è in
//! `swapdeck_core::relogin`; qui il motore dell'app resta fermo (`CswapApi::exclusive`) per
//! tutta l'operazione e gli eventi arrivano come `relogin-progress`. `cswap auto` avviato
//! dall'app si riavvia alla fine con le stesse opzioni; all'uscita dell'app [`shutdown`]
//! annulla l'operazione e ne attende la fine per poco.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use swapdeck_core::applog;
use swapdeck_core::cswap::{
    CswapAction, CswapClient, CswapError, CswapErrorKind, ListResponse, SecretToken,
};
use swapdeck_core::relogin::{
    ClaudeSearch, MAX_CODE_CHARS, Relogin, ReloginCommand, ReloginEvent, ReloginOutcome, SpawnHook,
    locate_claude, relogin_accounts,
};
use swapdeck_core::timeutil::now_unix;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{CswapApi, Res};
use crate::app::{self, DataDir};
use crate::state::{Shared, lock};

/// Evento con un [`ReloginEvent`].
pub const PROGRESS_EVENT: &str = "relogin-progress";

/// Canale dell'operazione in corso (una alla volta): codice e annulla.
static CONTROL: Mutex<Option<Sender<ReloginCommand>>> = Mutex::new(None);
/// Operazione in esecuzione nel thread bloccante (dal turno esclusivo al riavvio di `cswap auto`).
static IN_FLIGHT: AtomicBool = AtomicBool::new(false);
/// L'app si sta chiudendo: niente riavvio di `cswap auto`.
static EXITING: AtomicBool = AtomicBool::new(false);

/// Attesa massima della fine dell'operazione all'uscita dell'app (annulla, ritorno a P).
pub const EXIT_WAIT: Duration = Duration::from_secs(10);
const EXIT_POLL: Duration = Duration::from_millis(50);

/// `IN_FLIGHT` vero finché vive.
struct InFlight;

impl InFlight {
    fn mark() -> Self {
        IN_FLIGHT.store(true, Ordering::Release);
        Self
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        IN_FLIGHT.store(false, Ordering::Release);
    }
}

fn invalid(msg: &str) -> CswapError {
    CswapError::new(CswapErrorKind::InvalidUsage, msg)
}

/// Utenze per cui mostrare «Rifai login» (criterio del core, giorni dell'avviso di scadenza).
#[tauri::command]
pub fn relogin_accounts_for(
    shared: State<'_, Arc<Shared>>,
    list: serde_json::Value,
) -> Res<Vec<u32>> {
    let list: ListResponse =
        serde_json::from_value(list).map_err(|_| invalid("lista non valida"))?;
    let notifications = lock(&shared.settings).notifications.clone();
    Ok(relogin_accounts(&list, &notifications, now_unix()))
}

/// Avvia «Rifai login» per l'utenza `account` e attende l'esito.
#[tauri::command]
pub async fn start_relogin(
    app: AppHandle,
    shared: State<'_, Arc<Shared>>,
    account: u32,
) -> Res<ReloginOutcome> {
    let (tx, rx) = mpsc::channel();
    {
        let mut slot = lock(&CONTROL);
        if slot.is_some() {
            return Err(CswapError::new(
                CswapErrorKind::Busy,
                "un «Rifai login» è già in corso",
            ));
        }
        *slot = Some(tx);
    }
    let configured = lock(&shared.settings).claude_code_path.clone();
    let data_dir = app.state::<DataDir>().0.clone();
    let api = Arc::clone(&shared.api);
    let emitter = app.clone();
    let joined = tauri::async_runtime::spawn_blocking(move || {
        let _flight = InFlight::mark();
        let mut progress = |event: ReloginEvent| {
            let _ = emitter.emit(PROGRESS_EVENT, &event);
        };
        let mut result = None;
        let job_api = Arc::clone(&api);
        // Su Windows Claude Code entra nel Job Object dell'app: non resta orfano se l'app muore.
        let hook = SpawnHook::new(move |child| {
            job_api.adopt_child(child);
        });
        // Ciclo di lettura, motore e `cswap auto` fermi per tutta l'operazione.
        let auto = exclusive_keeping_auto(api.as_ref(), &mut || {
            result = Some(CswapClient::locate().and_then(|client| {
                client.check_version()?;
                let claude = locate_claude(configured.as_deref(), &ClaudeSearch::from_process());
                Relogin::new(client, data_dir.clone(), claude)
                    .with_spawn_hook(hook.clone())
                    .run(account, &rx, &mut progress)
            }));
        });
        let result =
            result.unwrap_or_else(|| Err(CswapError::new(CswapErrorKind::Failed, "non eseguito")));
        (result, auto)
    })
    .await;
    let (outcome, auto) = joined.unwrap_or_else(|e| {
        (
            Err(CswapError::new(CswapErrorKind::Failed, e.to_string())),
            false,
        )
    });
    *lock(&CONTROL) = None;
    app::set_auto_running(&app, auto);
    app::poller(&app).refresh();
    outcome
}

/// Ferma `cswap auto` dell'app ricordandone le opzioni, esegue `f` nel turno esclusivo e alla
/// fine (in ogni caso) lo riavvia, se l'app non si sta chiudendo. `true` se è di nuovo attivo.
pub(crate) fn exclusive_keeping_auto(api: &dyn CswapApi, f: &mut dyn FnMut()) -> bool {
    let resume = api.pause_auto();
    api.exclusive(f);
    let Some(options) = resume else {
        return false;
    };
    if EXITING.load(Ordering::Acquire) {
        return false;
    }
    match api.execute(&CswapAction::AutoStart { options }) {
        Ok(_) => {
            applog::info("rifai login: cswap auto riavviato");
            true
        }
        Err(e) => {
            applog::warn(&format!(
                "rifai login: cswap auto non riavviato, {}",
                e.log_summary()
            ));
            false
        }
    }
}

/// All'uscita dell'app: annulla l'operazione in corso (Claude Code chiuso, ritorno a P) e ne
/// attende la fine per al massimo `wait`. `true` se non resta nulla in esecuzione.
pub fn shutdown(wait: Duration) -> bool {
    EXITING.store(true, Ordering::Release);
    if let Some(tx) = lock(&CONTROL).as_ref() {
        let _ = tx.send(ReloginCommand::Cancel);
    }
    let deadline = Instant::now() + wait;
    while IN_FLIGHT.load(Ordering::Acquire) {
        if Instant::now() >= deadline {
            applog::warn("rifai login: ancora in corso alla chiusura dell'app");
            return false;
        }
        std::thread::sleep(EXIT_POLL);
    }
    true
}

/// Codice incollato: va solo su stdin di Claude Code (mai nei log né negli eventi).
#[tauri::command]
pub fn submit_relogin_code(code: String) -> Res<()> {
    if code.chars().count() > MAX_CODE_CHARS {
        drop(SecretToken::new(code));
        return Err(invalid("codice troppo lungo"));
    }
    send(ReloginCommand::Code(SecretToken::new(code)))
}

#[tauri::command]
pub fn cancel_relogin() -> Res<()> {
    send(ReloginCommand::Cancel)
}

fn send(cmd: ReloginCommand) -> Res<()> {
    lock(&CONTROL)
        .as_ref()
        .and_then(|tx| tx.send(cmd).ok())
        .ok_or_else(|| invalid("nessun «Rifai login» in corso"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::MockApi;
    use swapdeck_core::cswap::AutoOptions;

    /// `CONTROL`, `EXITING` e `IN_FLIGHT` sono globali: questi test girano uno alla volta.
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn auto_is_restarted_with_its_options() {
        let _s = lock(&SERIAL);
        EXITING.store(false, Ordering::Release);
        let api = MockApi::new();
        let options = AutoOptions {
            once: true,
            ..AutoOptions::default()
        };
        *api.paused_auto.lock().unwrap() = Some(options.clone());
        let mut ran = false;
        assert!(exclusive_keeping_auto(&api, &mut || ran = true));
        assert!(ran);
        let actions = api.actions.lock().unwrap().clone();
        assert_eq!(actions[..2], ["pause_auto", "exclusive"]);
        assert_eq!(
            actions[2],
            format!("{:?}", CswapAction::AutoStart { options })
        );

        // Non attivo prima: non si avvia.
        let api = MockApi::new();
        assert!(!exclusive_keeping_auto(&api, &mut || {}));
        assert_eq!(*api.actions.lock().unwrap(), ["pause_auto", "exclusive"]);
    }

    #[test]
    fn shutdown_cancels_and_waits_for_the_end() {
        let _s = lock(&SERIAL);
        let (tx, rx) = mpsc::channel();
        *lock(&CONTROL) = Some(tx);
        let flight = InFlight::mark();
        let worker = std::thread::spawn(move || {
            // Come il flusso: all'annulla finisce e libera IN_FLIGHT.
            let got = rx.recv_timeout(Duration::from_secs(5));
            drop(flight);
            matches!(got, Ok(ReloginCommand::Cancel))
        });
        assert!(shutdown(Duration::from_secs(5)));
        assert!(worker.join().unwrap(), "annulla inviato");
        *lock(&CONTROL) = None;

        // Durante l'uscita `cswap auto` non riparte.
        let api = MockApi::new();
        *api.paused_auto.lock().unwrap() = Some(AutoOptions::default());
        assert!(!exclusive_keeping_auto(&api, &mut || {}));
        assert_eq!(*api.actions.lock().unwrap(), ["pause_auto", "exclusive"]);

        let flight = InFlight::mark();
        assert!(!shutdown(Duration::from_millis(100)), "tempo massimo");
        drop(flight);
        EXITING.store(false, Ordering::Release);
    }

    #[test]
    fn commands_without_an_operation_are_refused() {
        let _s = lock(&SERIAL);
        assert_eq!(
            cancel_relogin().unwrap_err().kind,
            CswapErrorKind::InvalidUsage
        );
        assert_eq!(
            submit_relogin_code("x".repeat(MAX_CODE_CHARS + 1))
                .unwrap_err()
                .kind,
            CswapErrorKind::InvalidUsage
        );
        let (tx, rx) = mpsc::channel();
        *lock(&CONTROL) = Some(tx);
        submit_relogin_code("abc".into()).unwrap();
        assert!(matches!(rx.try_recv(), Ok(ReloginCommand::Code(_))));
        *lock(&CONTROL) = None;
    }
}
