//! Segnaposto dell'app desktop: Tauri 2 arriva con lo step 4.
//! `swapdeck --headless [--once] [--dry-run]` avvia il motore senza interfaccia.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn banner() -> String {
    format!("swapdeck {}", swapdeck_core::version())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--headless") {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        if let Err(e) = ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst)) {
            eprintln!("gestore di Ctrl+C non installato: {e}");
        }
        std::process::exit(swapdeck_core::headless::cli_main(&args[1..], &stop));
    }
    println!("{}", banner());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_contains_version() {
        assert_eq!(banner(), format!("swapdeck {}", swapdeck_core::version()));
    }
}
