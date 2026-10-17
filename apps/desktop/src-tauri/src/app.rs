//! Avvio dell'app Tauri: plugin, stato, area di notifica, ciclo di lettura.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use swapdeck_core::applog;
use swapdeck_core::config::Language;
use swapdeck_core::cswap::ListResponse;
use swapdeck_core::engine::EngineRules;
use swapdeck_core::paths::AppPaths;
use tauri::{AppHandle, Emitter, Manager, RunEvent};
#[cfg(not(windows))]
use tauri_plugin_notification::NotificationExt as _;

use crate::api::{AutoPoll, CswapApi, Outcome, Res};
use crate::auto_watch::{AUTO_WATCH_PERIOD, AutoWatcher};
use crate::autostart::{self, MINIMIZED_ARG, Startup};
use crate::notify::AppNotifier;
use crate::poller::Poller;
use crate::settings::SettingsStore;
use crate::state::{STATE_EVENT, Shared, Snapshot, lock, now_ms};
use crate::{commands, config_watch, engine_loop, setup_cmd, text, tray, updater, windows};

/// Cartella dati (storico e lock del motore); `None` se la home non è determinabile.
pub struct DataDir(pub Option<PathBuf>);

/// Arresto del controllo della config (`config_watch`), all'uscita.
struct ConfigWatchStop(Arc<AtomicBool>);

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

/// Su Windows con lo stesso notificatore di `--headless` (AUMID registrato `com.swapdeck.desktop`,
/// nome e icona di SwapDeck): il plugin usa l'identificativo solo fuori da `target\debug` e
/// `target\release`, così sviluppo, zip portatile e installer si comportano uguale.
#[cfg(windows)]
pub fn notify(_app: &AppHandle, (title, body): (String, String)) {
    use swapdeck_core::headless::{OsNotifier as _, SystemNotifier};
    use swapdeck_core::notify::Severity;
    // In un thread: la notifica può attendere fino a 5 s e non deve fermare il chiamante.
    let spawned = std::thread::Builder::new()
        .name("app-notify".into())
        .spawn(move || {
            if let Err(e) = SystemNotifier.show(&title, &body, Severity::Info) {
                eprintln!("notifica non mostrata: {e}");
            }
        });
    if let Err(e) = spawned {
        eprintln!("notifica non mostrata: {e}");
    }
}

#[cfg(not(windows))]
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
    let (lang, rules, profile) = {
        let s = lock(&shared.settings);
        let profile = s.engine_profiles.active().map(|p| p.name.clone());
        (s.lang, s.notifications.clone(), profile)
    };
    let api = Arc::clone(&shared.api);
    let notices = lock(&shared.notifier).observe(
        result.as_ref().ok(),
        (&rules, profile.as_deref()),
        &mut || api.installed_version(),
    );
    let language = match lang {
        text::Lang::It => Language::It,
        text::Lang::En => Language::En,
    };
    for n in &notices {
        notify(
            app,
            (
                n.title.get(language).to_owned(),
                n.body.get(language).to_owned(),
            ),
        );
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
            applog::info(&format!(
                "avvio automatico: {} ({plan:?})",
                if on { "attivo" } else { "spento" }
            ));
            {
                let mut s = lock(&shared.settings);
                s.autostart = on;
                s.autostart_saved = true;
            }
            if let Err(e) = shared.save_settings() {
                eprintln!("config non salvata: {e}");
            }
        }
        Err(e) => {
            eprintln!("avvio automatico non aggiornato: {e}");
            applog::warn(&format!("avvio automatico non aggiornato: {e}"));
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

/// Regole e intervallo per un giro del motore. Le regole (e le mappe) si rileggono dal file a
/// ogni giro, come `--headless`: una mappa scelta da CLI o da VS Code vale dal giro dopo. File
/// assente o non leggibile: restano quelle in memoria.
fn engine_inputs(shared: &Shared, config: Option<&Path>) -> (EngineRules, Duration) {
    let loaded = config
        .and_then(|p| swapdeck_core::config::load(p).ok())
        .filter(|l| l.from_file);
    let mut s = lock(&shared.settings);
    if let Some(l) = loaded {
        s.engine = l.config.engine;
        s.engine_profiles = l.config.engine_profiles;
    }
    (s.engine.clone(), s.poll_interval)
}

pub fn run(api: Arc<dyn CswapApi>) {
    applog::init("desktop");
    let start_hidden = std::env::args().any(|a| a == MINIMIZED_ARG);
    applog::info(&format!(
        "{}{}",
        applog::startup_line("dell'app"),
        if start_hidden {
            " (avvio nascosto)"
        } else {
            ""
        }
    ));
    swapdeck_core::headless::log_app_id_registration(swapdeck_core::headless::register_app_id());
    let (store, settings, warnings) = SettingsStore::load();
    for w in warnings {
        eprintln!("config: {w}");
        applog::warn(&format!("config: {w}"));
    }
    let interval = settings.poll_interval;
    let updates = settings.updates_enabled;
    let data_dir = AppPaths::from_env_or_system().map(|p| p.data_dir);
    let config_file = AppPaths::from_env_or_system()
        .filter(|_| store.is_persistent())
        .map(|p| p.config_file());
    let shared = Arc::new(Shared::new(api, store, settings));
    *lock(&shared.notifier) = AppNotifier::new(data_dir.as_deref());

    let app = tauri::Builder::default()
        // Per prima: la seconda istanza mostra la finestra principale e termina.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_main(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
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
            commands::mini_hide,
            commands::mini_set_compact,
            commands::mini_is_compact,
            commands::mini_fit_height,
            commands::get_snapshot,
            commands::list_mappings,
            commands::get_engine_state,
            commands::set_engine_rules,
            commands::get_engine_profiles,
            commands::apply_engine_profile,
            commands::get_preferences,
            commands::set_preferences,
            commands::get_notification_settings,
            commands::set_notification_settings,
            commands::get_history_events,
            commands::get_usage_breakdown,
            commands::get_diagnostics,
            commands::open_logs_folder,
            setup_cmd::diagnose_setup,
            setup_cmd::install_cswap,
            setup_cmd::upgrade_cswap,
            setup_cmd::check_cswap_update,
            setup_cmd::dismiss_cswap_update,
        ])
        .on_window_event(windows::on_window_event)
        .setup(move |app| {
            let handle = app.handle().clone();
            sync_autostart(&handle, &shared);
            tray::create(&handle)?;
            if updates {
                updater::spawn(&handle);
            }
            let cb = handle.clone();
            let engine_cb = handle.clone();
            let api = Arc::clone(&shared.api);
            let dir = data_dir.clone();
            let live = Arc::clone(&shared);
            let found = Arc::clone(&shared);
            let watched = config_file.clone();
            let (poller, thread) = Poller::spawn_with(
                interval,
                move |r| on_list(&cb, r),
                move |lp| {
                    engine_loop::run(
                        lp,
                        api,
                        dir,
                        // Regole e intervallo riletti a ogni giro (cambiano dalla UI, dalla
                        // CLI e da VS Code).
                        move || engine_inputs(&live, config_file.as_deref()),
                        move |rules, step| {
                            lock(&engine_cb.state::<Arc<Shared>>().notifier).on_step(rules, step);
                            let Some(view) = engine_loop::view_of(rules, step) else {
                                return;
                            };
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
            // Config cambiata da VS Code o dalla CLI: preferenze applicate, schede aperte rilette.
            if let Some(file) = watched {
                let stop = Arc::new(AtomicBool::new(false));
                config_watch::spawn(
                    handle.clone(),
                    Arc::clone(&shared),
                    file,
                    Arc::clone(&stop),
                    |app| {
                        windows::apply_saved_theme(app);
                        self::poller(app).refresh();
                    },
                );
                app.manage(ConfigWatchStop(stop));
            }
            windows::apply_saved_theme(&handle);
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
            if let Some(s) = app.try_state::<ConfigWatchStop>() {
                s.0.store(true, Ordering::Relaxed);
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
                applog::warn(&format!("config non salvata all'uscita: {e}"));
            }
            applog::info("app chiusa");
            // Per ultimo, dopo l'arresto ordinato: su Windows avvia l'installer e termina.
            updater::install_pending(app);
        }
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::MockApi;
    use crate::settings::{Settings, SettingsStore};

    #[test]
    fn engine_rules_are_reread_from_the_file_each_round() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path());
        paths.ensure().unwrap();
        let file = paths.config_file();
        let shared = Shared::new(
            Arc::new(MockApi::new()),
            SettingsStore::in_memory(),
            Settings::default(),
        );
        // File assente: restano le regole in memoria.
        assert_eq!(
            engine_inputs(&shared, Some(&file)).0,
            EngineRules::default()
        );
        // Mappa cambiata da fuori (CLI o VS Code): vale dal giro dopo.
        std::fs::write(
            &file,
            r#"{"schemaVersion":1,"engine":{"order":[3,1],"fiveHourThreshold":80}}"#,
        )
        .unwrap();
        let (rules, _) = engine_inputs(&shared, Some(&file));
        assert_eq!((rules.primary, rules.five_hour_threshold), (Some(3), 80.0));
        assert_eq!(lock(&shared.settings).engine, rules);
        // File non valido: restano le ultime lette.
        std::fs::write(&file, "{ non json").unwrap();
        assert_eq!(engine_inputs(&shared, Some(&file)).0, rules);
        assert_eq!(engine_inputs(&shared, None).0, rules);
    }
}
