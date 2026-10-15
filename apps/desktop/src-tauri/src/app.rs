//! Avvio dell'app Tauri: plugin, stato, area di notifica, ciclo di lettura.

use std::sync::Arc;

use swapdeck_core::cswap::ListResponse;
use tauri::{AppHandle, Emitter, Manager, RunEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt as _};
use tauri_plugin_notification::NotificationExt as _;

use crate::api::{CswapApi, Outcome, Res};
use crate::poller::Poller;
use crate::settings::SettingsStore;
use crate::state::{STATE_EVENT, Shared, lock, now_ms};
use crate::{commands, text, tray, windows};

/// Argomento con cui l'avvio automatico lancia l'app: parte nell'area di notifica, senza finestra.
pub const MINIMIZED_ARG: &str = "--minimized";

/// L'avvio automatico tocca il sistema solo nelle build di rilascio: in sviluppo l'eseguibile
/// cambia a ogni build e non va registrato al login.
pub fn autostart_managed() -> bool {
    !cfg!(debug_assertions)
}

pub fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if !autostart_managed() {
        return Ok(());
    }
    let launcher = app.autolaunch();
    let current = launcher.is_enabled().map_err(|e| e.to_string())?;
    match (enabled, current) {
        (true, false) => launcher.enable().map_err(|e| e.to_string()),
        (false, true) => launcher.disable().map_err(|e| e.to_string()),
        _ => Ok(()),
    }
}

pub fn poller(app: &AppHandle) -> Poller {
    app.state::<Poller>().inner().clone()
}

pub fn notify(app: &AppHandle, (title, body): (String, String)) {
    if let Err(e) = app.notification().builder().title(title).body(body).show() {
        eprintln!("notifica non mostrata: {e}");
    }
}

/// Esito di una lettura: aggiorna lo stato, la UI, l'icona e mostra le notifiche.
pub fn on_list(app: &AppHandle, result: &Res<ListResponse>) {
    let shared = app.state::<Arc<Shared>>();
    let snap = {
        let mut s = lock(&shared.snapshot);
        s.apply(result, now_ms());
        s.clone()
    };
    if let Err(e) = app.emit(STATE_EVENT, &snap) {
        eprintln!("evento di stato non inviato: {e}");
    }
    tray::update(app, &snap);
    if let Ok(list) = result {
        let notices = lock(&shared.notifier).observe(list);
        let lang = lock(&shared.settings).lang;
        for n in &notices {
            notify(app, text::notice(lang, n));
        }
    }
}

/// Dopo un'azione: usa la lista riletta dal client oppure chiede una lettura.
pub fn apply_outcome(app: &AppHandle, outcome: &Outcome) {
    match &outcome.list {
        Some(list) => on_list(app, &Ok(list.clone())),
        None => poller(app).refresh(),
    }
}

pub fn run(api: Arc<dyn CswapApi>) {
    let (store, settings, warnings) = SettingsStore::load();
    for w in warnings {
        eprintln!("config: {w}");
    }
    let start_hidden = std::env::args().any(|a| a == MINIMIZED_ARG);
    let interval = settings.poll_interval;
    let autostart = settings.autostart;
    let shared = Arc::new(Shared::new(api, store, settings));

    let app = tauri::Builder::default()
        // Per prima: la seconda istanza mostra la finestra principale e termina.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_main(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ))
        .manage(Arc::clone(&shared))
        .invoke_handler(tauri::generate_handler![
            commands::list_accounts,
            commands::get_status,
            commands::get_config,
            commands::get_config_value,
            commands::get_history,
            commands::confirmation_for,
            commands::run_action,
            commands::get_app_settings,
            commands::set_autostart,
            commands::show_main,
        ])
        .on_window_event(windows::on_window_event)
        .setup(move |app| {
            let handle = app.handle().clone();
            if let Err(e) = apply_autostart(&handle, autostart) {
                eprintln!("avvio automatico non aggiornato: {e}");
            }
            tray::create(&handle)?;
            let cb = handle.clone();
            let (poller, _thread) =
                Poller::spawn(Arc::clone(&shared.api), interval, move |r| on_list(&cb, r));
            app.manage(poller);
            if !start_hidden {
                windows::show_main(&handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("avvio dell'app non riuscito");

    app.run(|app, event| match event {
        // L'ultima finestra chiusa non termina l'app: resta nell'area di notifica.
        RunEvent::ExitRequested {
            api, code: None, ..
        } => api.prevent_exit(),
        RunEvent::Exit => {
            if let Some(p) = app.try_state::<Poller>() {
                p.stop();
            }
            if let Err(e) = app.state::<Arc<Shared>>().save_settings() {
                eprintln!("config non salvata all'uscita: {e}");
            }
        }
        _ => {}
    });
}
