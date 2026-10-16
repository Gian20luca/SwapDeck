//! Comandi `invoke` del primo avvio e della manutenzione di cswap (`SetupBackend` della UI).
//! L'avanzamento di installazione e aggiornamento arriva come evento `setup-progress`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use swapdeck_core::cswap::{CswapError, CswapErrorKind};
use swapdeck_core::setup::{Setup, SetupDiagnosis, SetupOutcome, SetupProgress, UpdateInfo};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::Res;
use crate::app::{self, DataDir};
use crate::state::Shared;

/// Evento con un [`SetupProgress`].
pub const PROGRESS_EVENT: &str = "setup-progress";

/// Una sola installazione o un solo aggiornamento alla volta.
static RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
struct RunGuard;

impl RunGuard {
    fn take() -> Res<Self> {
        RUNNING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| {
                CswapError::new(
                    CswapErrorKind::Busy,
                    "un'installazione o un aggiornamento di cswap è già in corso",
                )
            })
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::Release);
    }
}

fn setup(app: &AppHandle) -> Res<Setup> {
    app.state::<DataDir>()
        .0
        .clone()
        .map(Setup::new)
        .ok_or_else(|| {
            CswapError::new(
                CswapErrorKind::Failed,
                "cartella dati di SwapDeck non determinabile",
            )
        })
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(CswapError::new(CswapErrorKind::Failed, e.to_string())))
}

fn emitter(app: &AppHandle) -> impl FnMut(SetupProgress) + use<> {
    let app = app.clone();
    move |event| {
        let _ = app.emit(PROGRESS_EVENT, &event);
    }
}

#[tauri::command]
pub async fn diagnose_setup(app: AppHandle) -> Res<SetupDiagnosis> {
    // La diagnosi non usa la cartella dati: va anche senza.
    let setup = setup(&app).unwrap_or_else(|_| Setup::new(std::env::temp_dir()));
    blocking(move || Ok(setup.diagnose())).await
}

#[tauri::command]
pub async fn install_cswap(app: AppHandle) -> Res<SetupOutcome> {
    let guard = RunGuard::take()?;
    let setup = setup(&app)?;
    let mut progress = emitter(&app);
    let outcome = blocking(move || {
        let _guard = guard;
        setup.install(&mut progress)
    })
    .await;
    after_change(&app);
    outcome
}

#[tauri::command]
pub async fn upgrade_cswap(app: AppHandle, shared: State<'_, Arc<Shared>>) -> Res<SetupOutcome> {
    let guard = RunGuard::take()?;
    let setup = setup(&app)?;
    let mut progress = emitter(&app);
    let api = Arc::clone(&shared.api);
    let outcome = blocking(move || {
        let _guard = guard;
        let mut result = None;
        // Ciclo di lettura, motore e `cswap auto` fermi per tutto l'aggiornamento.
        api.exclusive(&mut || result = Some(setup.upgrade(&mut progress)));
        result.unwrap_or_else(|| Err(CswapError::new(CswapErrorKind::Failed, "non eseguito")))
    })
    .await;
    app::set_auto_running(&app, false);
    after_change(&app);
    outcome
}

/// Dopo installazione o aggiornamento (anche non riusciti): cswap si cerca di nuovo e si rilegge.
fn after_change(app: &AppHandle) {
    app.state::<Arc<Shared>>().api.relocate();
    app::poller(app).refresh();
}

#[tauri::command]
pub async fn check_cswap_update(app: AppHandle, force: Option<bool>) -> Res<UpdateInfo> {
    let setup = setup(&app)?;
    blocking(move || setup.check_update(force.unwrap_or(false))).await
}

#[tauri::command]
pub async fn dismiss_cswap_update(app: AppHandle, version: String) -> Res<()> {
    let setup = setup(&app)?;
    blocking(move || setup.dismiss_update(&version)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_run_at_a_time() {
        let first = RunGuard::take().unwrap();
        assert_eq!(RunGuard::take().unwrap_err().kind, CswapErrorKind::Busy);
        drop(first);
        drop(RunGuard::take().unwrap());
    }
}
