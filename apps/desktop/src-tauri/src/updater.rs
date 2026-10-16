//! Aggiornamenti dalle GitHub Releases (`tauri-plugin-updater`), solo con `updates.enabled`
//! nella config (spento di default finché il repo è privato).
//!
//! Un thread controlla `latest.json` poco dopo l'avvio e poi una volta al giorno (con attesa
//! crescente dopo un errore di rete). Il pacchetto si scarica in memoria e il plugin ne verifica
//! la firma con la chiave pubblica di `tauri.conf.json`; si installa solo all'uscita dall'app,
//! dopo l'arresto ordinato (`cswap auto` fermato, lock del motore rilasciato).
//! Proxy: quelli delle variabili d'ambiente (`HTTPS_PROXY`, `NO_PROXY`), come `reqwest`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt as _};

use crate::state::{Shared, lock};
use crate::{app, text};

const FIRST_CHECK_DELAY: Duration = Duration::from_secs(30);
const CHECK_PERIOD: Duration = Duration::from_secs(24 * 3600);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const RETRY_BASE: Duration = Duration::from_secs(60);
const RETRY_MAX: Duration = Duration::from_secs(6 * 3600);

/// Aggiornamento scaricato e verificato, in attesa dell'uscita.
#[derive(Default)]
struct Pending(Mutex<Option<(Update, Vec<u8>)>>);

/// Attesa dopo `failures` errori consecutivi: 1, 2, 4… minuti, al massimo 6 ore.
fn retry_delay(failures: u32) -> Duration {
    RETRY_BASE
        .saturating_mul(1u32 << failures.min(16))
        .min(RETRY_MAX)
}

/// Avvia il controllo in background (da chiamare solo con `updates.enabled`).
pub fn spawn(app: &AppHandle) {
    app.manage(Pending::default());
    let app = app.clone();
    let started = std::thread::Builder::new()
        .name("updater".into())
        .spawn(move || {
            let mut delay = FIRST_CHECK_DELAY;
            let mut failures = 0u32;
            loop {
                std::thread::sleep(delay);
                match tauri::async_runtime::block_on(fetch(&app)) {
                    Ok(Some((update, bytes))) => {
                        let lang = lock(&app.state::<Arc<Shared>>().settings).lang;
                        app::notify(&app, text::update_ready(lang, &update.version));
                        *lock(&app.state::<Pending>().0) = Some((update, bytes));
                        return;
                    }
                    Ok(None) => {
                        failures = 0;
                        delay = CHECK_PERIOD;
                    }
                    Err(e) => {
                        eprintln!("aggiornamenti: controllo non riuscito: {e}");
                        delay = retry_delay(failures);
                        failures = failures.saturating_add(1);
                    }
                }
            }
        });
    if let Err(e) = started {
        eprintln!("aggiornamenti: thread non avviato: {e}");
    }
}

async fn fetch(app: &AppHandle) -> tauri_plugin_updater::Result<Option<(Update, Vec<u8>)>> {
    let updater = app.updater_builder().timeout(REQUEST_TIMEOUT).build()?;
    let Some(update) = updater.check().await? else {
        return Ok(None);
    };
    // `download` verifica la firma prima di restituire il pacchetto.
    let bytes = update.download(|_, _| {}, || {}).await?;
    Ok(Some((update, bytes)))
}

/// All'uscita: installa l'aggiornamento pronto, se c'è (su Windows avvia l'installer e termina).
pub fn install_pending(app: &AppHandle) {
    let Some(pending) = app.try_state::<Pending>() else {
        return;
    };
    let Some((update, bytes)) = lock(&pending.0).take() else {
        return;
    };
    if let Err(e) = update.install(bytes) {
        eprintln!("aggiornamenti: installazione non riuscita: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_grows_and_is_capped() {
        assert_eq!(retry_delay(0), Duration::from_secs(60));
        assert_eq!(retry_delay(1), Duration::from_secs(120));
        assert_eq!(retry_delay(3), Duration::from_secs(480));
        assert_eq!(retry_delay(9), RETRY_MAX);
        assert_eq!(retry_delay(u32::MAX), RETRY_MAX);
    }
}
