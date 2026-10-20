//! Binario `swapdeck`: app nell'area di notifica (Tauri 2); con `--headless` il motore senza
//! interfaccia (step 2).
// In rilascio niente console su Windows (vedi HANDOFF: output di --headless).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Un binario di rilascio non deve mai caricare la pagina dal server di sviluppo (`devUrl`):
// `dev` lo definisce tauri-build quando manca `tauri/custom-protocol`.
#[cfg(all(dev, not(debug_assertions)))]
compile_error!(
    "build di rilascio senza custom-protocol: usare `--features custom-protocol` o `tauri build`"
);

mod api;
mod app;
mod auto_watch;
mod autostart;
mod commands;
mod config_watch;
mod engine_loop;
mod notify;
mod poller;
mod relogin_cmd;
mod ring;
mod settings;
mod setup_cmd;
mod state;
mod terminal;
mod text;
mod tray;
mod updater;
mod windows;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--headless") {
        swapdeck_core::applog::init("headless");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        if let Err(e) = ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst)) {
            eprintln!("gestore di Ctrl+C non installato: {e}");
        }
        std::process::exit(swapdeck_core::headless::cli_main(&args[1..], &stop));
    }
    app::run(Arc::new(api::ClientApi::new()));
}
