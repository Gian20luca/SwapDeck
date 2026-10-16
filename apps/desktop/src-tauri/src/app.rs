//! Avvio dell'app Tauri: plugin, stato, area di notifica, ciclo di lettura.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use swapdeck_core::cswap::ListResponse;
use swapdeck_core::paths::AppPaths;
use tauri::{AppHandle, Emitter, Manager, RunEvent};
use tauri_plugin_notification::NotificationExt as _;

use crate::api::{AutoPoll, CswapApi, Outcome, Res};
use crate::auto_watch::{AUTO_WATCH_PERIOD, AutoWatcher};
use crate::autostart::{self, MINIMIZED_ARG, Startup};
use crate::poller::Poller;
use crate::settings::SettingsStore;
use crate::state::{STATE_EVENT, Shared, Snapshot, lock, now_ms};
use crate::{commands, engine_loop, text, tray, windows};

/// Cartella dati (storico e lock del motore); `None` se la home non è determinabile.
pub struct DataDir(pub Option<PathBuf>);

/// Thread del ciclo di lettura, atteso all'uscita.
struct PollerThread(Mutex<Option<JoinHandle<()>>>);

/// Attesa massima della fine del ciclo all'uscita (un giro in corso può durare di più).
const POLLER_EXIT_WAIT: Duration = Duration::from_secs(3);

impl PollerThread {
    fn join_within(&self, max: Duration) {
        let Some(handle) = lock(&self.0).take() else {
            return;
        };
        let deadline = Instant::now() + max;
        while !handle.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if handle.is_finished() {
            let _ = handle.join();
        }
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
    update_snapshot(app, |s| {
        s.apply(result, now_ms());
        true
    });
    if let Ok(list) = result {
        let notices = lock(&shared.notifier).observe(list);
        let lang = lock(&shared.settings).lang;
        for n in &notices {
            notify(app, text::notice(lang, n));
        }
    }
}

/// Modifica lo stato; se `change` restituisce `true` lo invia alla UI e aggiorna l'icona.
pub fn update_snapshot(app: &AppHandle, change: impl FnOnce(&mut Snapshot) -> bool) {
    let shared = app.state::<Arc<Shared>>();
    let snap = {
        let mut s = lock(&shared.snapshot);
        if !change(&mut s) {
            return;
        }
        s.clone()
    };
    if let Err(e) = app.emit(STATE_EVENT, &snap) {
        eprintln!("evento di stato non inviato: {e}");
    }
    tray::update(app, &snap);
}

/// `cswap auto` avviato o fermato dall'app.
pub fn set_auto_running(app: &AppHandle, running: bool) {
    update_snapshot(app, |s| {
        std::mem::replace(&mut s.auto_running, running) != running
    });
}

/// Esito del controllo di `cswap auto`: stato nella UI, rilettura dopo un cambio, notifica alla fine.
fn on_auto_poll(app: &AppHandle, poll: &AutoPoll) {
    match poll {
        AutoPoll::NotRunning => set_auto_running(app, false),
        AutoPoll::Running { switched } => {
            set_auto_running(app, true);
            if *switched {
                poller(app).refresh();
            }
        }
        AutoPoll::Ended { code, error } => {
            set_auto_running(app, false);
            poller(app).refresh();
            let lang = lock(&app.state::<Arc<Shared>>().settings).lang;
            notify(app, text::auto_ended(lang, *code, error.as_ref()));
        }
    }
}

/// Avvio automatico all'avvio dell'app: forzato solo al primo avvio, poi vale il sistema.
fn sync_autostart(app: &AppHandle, shared: &Shared) {
    let saved = lock(&shared.settings).autostart_saved;
    let plan = autostart::startup_plan(autostart::managed(), shared.store.is_persistent(), saved);
    let enabled = match plan {
        Startup::Skip => return,
        Startup::Enable => autostart::apply(app, true).map(|()| true),
        Startup::Adopt => autostart::is_enabled(app).and_then(|on| {
            // Riscrive la voce attiva: percorso aggiornato e tra virgolette.
            if on {
                autostart::apply(app, true)?;
            }
            Ok(on)
        }),
    };
    match enabled {
        Ok(on) => {
            {
                let mut s = lock(&shared.settings);
                s.autostart = on;
                s.autostart_saved = true;
            }
            if let Err(e) = shared.save_settings() {
                eprintln!("config non salvata: {e}");
            }
        }
        Err(e) => eprintln!("avvio automatico non aggiornato: {e}"),
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
    let data_dir = AppPaths::from_env_or_system().map(|p| p.data_dir);
    let shared = Arc::new(Shared::new(api, store, settings));

    let app = tauri::Builder::default()
        // Per prima: la seconda istanza mostra la finestra principale e termina.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_main(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::clone(&shared))
        .manage(DataDir(data_dir.clone()))
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
            commands::get_snapshot,
            commands::list_mappings,
            commands::get_engine_state,
            commands::set_engine_rules,
            commands::get_preferences,
            commands::set_preferences,
            commands::get_history_events,
        ])
        .on_window_event(windows::on_window_event)
        .setup(move |app| {
            let handle = app.handle().clone();
            sync_autostart(&handle, &shared);
            tray::create(&handle)?;
            let cb = handle.clone();
            let engine_cb = handle.clone();
            let api = Arc::clone(&shared.api);
            let dir = data_dir.clone();
            let live = Arc::clone(&shared);
            let found = Arc::clone(&shared);
            let (poller, thread) = Poller::spawn_with(
                interval,
                move |r| on_list(&cb, r),
                move |lp| {
                    engine_loop::run(
                        lp,
                        api,
                        dir,
                        // Regole e intervallo riletti a ogni giro (cambiano dalla UI).
                        move || {
                            let s = lock(&live.settings);
                            (s.engine.clone(), s.poll_interval)
                        },
                        move |view| {
                            // Senza invio: l'esito della lettura segue subito e lo manda.
                            update_snapshot(&engine_cb, |s| {
                                s.set_engine(view);
                                false
                            });
                        },
                        move |conflicts| *lock(&found.conflicts) = conflicts.to_vec(),
                    );
                },
            );
            app.manage(poller);
            app.manage(PollerThread(Mutex::new(Some(thread))));
            let cb = handle.clone();
            let (watcher, _thread) =
                AutoWatcher::spawn(Arc::clone(&shared.api), AUTO_WATCH_PERIOD, move |p| {
                    on_auto_poll(&cb, p);
                });
            app.manage(watcher);
            if !start_hidden {
                windows::show_main(&handle);
            }
            // Solo per le prove di sviluppo (screenshot senza cliccare il menu).
            #[cfg(debug_assertions)]
            if std::env::var_os("SWAPDECK_DEV_OPEN_MINI").is_some() {
                windows::toggle_mini(&handle);
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
            // Tauri non rilascia lo stato gestito: `cswap auto` va fermato qui, prima del ciclo.
            if let Some(w) = app.try_state::<AutoWatcher>() {
                w.stop();
            }
            app.state::<Arc<Shared>>().api.shutdown();
            if let Some(p) = app.try_state::<Poller>() {
                p.stop();
            }
            // Il ciclo rilascia il lock del motore uscendo: lo si aspetta un poco.
            if let Some(t) = app.try_state::<PollerThread>() {
                t.join_within(POLLER_EXIT_WAIT);
            }
            if let Err(e) = app.state::<Arc<Shared>>().save_settings() {
                eprintln!("config non salvata all'uscita: {e}");
            }
        }
        _ => {}
    });
}
