//! Cambi della config fatti fuori dall'app (VS Code, CLI, `--headless`): polling leggero di
//! dimensione e data di modifica di `config.json` e del segnale `vscode-sync.json` che VS Code
//! riscrive dopo una sua azione (cambio di account, configurazione di cswap, cartelle).
//!
//! Un cambio vale quando la firma del file resta ferma per un giro (le scritture ravvicinate si
//! raggruppano). Le scritture dell'app stessa non contano: la config letta si confronta con
//! l'ultima nota ([`Shared::absorb_config`]), che l'app aggiorna a ogni suo salvataggio. Nessun
//! link seguito (`symlink_metadata`), nessuna scrittura: solo letture.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use swapdeck_core::applog;
use swapdeck_core::config;
use tauri::{AppHandle, Emitter};

use crate::state::Shared;

/// Evento verso tutte le finestre: le schede aperte rileggono i loro dati (`CONFIG_EVENT` in
/// `TauriBackend.ts`).
pub const CONFIG_EVENT: &str = "config-changed";
/// Segnale di VS Code (`SYNC_FILE` in `apps/vscode/src/sync.ts`).
pub const SYNC_FILE: &str = "vscode-sync.json";
/// Periodo del controllo: un cambio arriva alla UI tra uno e due periodi dopo.
pub const WATCH_PERIOD: Duration = Duration::from_millis(500);

/// Firma di un file: assente, oppure dimensione e data di modifica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sig(Option<(u64, Option<SystemTime>)>);

fn sig(path: &Path) -> Sig {
    Sig(fs::symlink_metadata(path)
        .ok()
        .filter(fs::Metadata::is_file)
        .map(|m| (m.len(), m.modified().ok())))
}

/// Un file osservato: firma già considerata e firma nuova in attesa di restare ferma.
#[derive(Debug)]
struct Slot {
    seen: Sig,
    pending: Option<Sig>,
}

impl Slot {
    /// `true` quando una firma nuova è rimasta uguale per due controlli di fila.
    fn step(&mut self, now: Sig) -> bool {
        if now == self.seen {
            self.pending = None;
            false
        } else if self.pending == Some(now) {
            self.seen = now;
            self.pending = None;
            true
        } else {
            self.pending = Some(now);
            false
        }
    }
}

/// Esito di un controllo.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Changes {
    pub config: bool,
    pub signal: bool,
}

/// Stato del polling della cartella della config.
#[derive(Debug)]
pub struct Watch {
    config: PathBuf,
    signal: PathBuf,
    slots: [Slot; 2],
}

impl Watch {
    /// Parte dallo stato attuale dei file: quello che c'è già non è un cambio.
    pub fn new(config_file: &Path) -> Self {
        let dir = config_file.parent().unwrap_or(Path::new(""));
        let signal = dir.join(SYNC_FILE);
        let slot = |p: &Path| Slot {
            seen: sig(p),
            pending: None,
        };
        Self {
            slots: [slot(config_file), slot(&signal)],
            config: config_file.to_path_buf(),
            signal,
        }
    }

    pub fn poll(&mut self) -> Changes {
        let config = self.slots[0].step(sig(&self.config));
        let signal = self.slots[1].step(sig(&self.signal));
        Changes { config, signal }
    }
}

/// Applica un controllo: config rilette (se cambiate da fuori) e segnale. `true` = la UI deve
/// rileggere. Config non leggibile o non valida: si ignora (resta quella in memoria).
pub fn absorb(shared: &Shared, config_file: &Path, changes: Changes) -> bool {
    let mut external = changes.signal;
    if changes.config {
        match config::load(config_file) {
            Ok(loaded) if loaded.from_file => external |= shared.absorb_config(&loaded.config),
            Ok(_) => {}
            Err(e) => applog::warn(&format!("config cambiata ma non leggibile: {e}")),
        }
    }
    external
}

/// Thread del controllo; si ferma con `stop`.
pub fn spawn(
    app: AppHandle,
    shared: Arc<Shared>,
    config_file: PathBuf,
    stop: Arc<AtomicBool>,
    on_external: impl Fn(&AppHandle) + Send + 'static,
) {
    if let Ok(loaded) = config::load(&config_file) {
        shared.note_config(&loaded.config);
    }
    let spawned = std::thread::Builder::new()
        .name("config-watch".into())
        .spawn(move || {
            let mut watch = Watch::new(&config_file);
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(WATCH_PERIOD);
                let changes = watch.poll();
                let seen = changes != Changes::default() && absorb(&shared, &config_file, changes);
                // Anche un cambio esterno trovato nel file da una scrittura dell'app.
                if shared.take_external() | seen {
                    applog::info("config cambiata fuori dall'app: le schede rileggono");
                    on_external(&app);
                    if let Err(e) = app.emit(CONFIG_EVENT, ()) {
                        applog::warn(&format!("evento {CONFIG_EVENT} non inviato: {e}"));
                    }
                }
            }
        });
    if let Err(e) = spawned {
        applog::warn(&format!("controllo della config non avviato: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::MockApi;
    use crate::settings::{Settings, SettingsStore};
    use swapdeck_core::paths::AppPaths;

    fn write(path: &Path, text: &str) {
        fs::write(path, text).unwrap();
    }

    #[test]
    fn a_change_counts_once_it_stays_still() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        let mut w = Watch::new(&file);
        assert_eq!(w.poll(), Changes::default(), "file assente fin dall'inizio");
        write(&file, "{}");
        assert_eq!(
            w.poll(),
            Changes::default(),
            "appena cambiato: si aspetta un giro"
        );
        assert!(w.poll().config, "fermo per un giro: cambio");
        assert_eq!(w.poll(), Changes::default(), "già considerato");
        write(&dir.path().join(SYNC_FILE), r#"{"source":"x","at":1}"#);
        w.poll();
        assert_eq!(
            w.poll(),
            Changes {
                config: false,
                signal: true
            }
        );
        // Un file che cambia di nuovo prima del controllo dopo si raggruppa.
        write(&file, "{ }");
        w.poll();
        write(&file, "{  }");
        assert!(!w.poll().config);
        assert!(w.poll().config);
        fs::remove_file(&file).unwrap();
        w.poll();
        assert!(w.poll().config, "anche la cancellazione è un cambio");
    }

    #[test]
    fn existing_files_are_not_changes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config.json");
        write(&file, "{}");
        write(&dir.path().join(SYNC_FILE), "{}");
        let mut w = Watch::new(&file);
        assert_eq!(w.poll(), Changes::default());
        assert_eq!(w.poll(), Changes::default());
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("altro.json");
        write(&target, "{}");
        let file = dir.path().join("config.json");
        std::os::unix::fs::symlink(&target, &file).unwrap();
        assert_eq!(sig(&file), Sig(None));
    }

    fn shared_at(dir: &Path) -> (Arc<Shared>, PathBuf) {
        let paths = AppPaths::from_root(dir);
        let file = paths.config_file();
        let (store, settings, _) = SettingsStore::at(paths).read();
        (
            Arc::new(Shared::new(Arc::new(MockApi::new()), store, settings)),
            file,
        )
    }

    const CONFIG_ONLY: Changes = Changes {
        config: true,
        signal: false,
    };

    #[test]
    fn own_writes_are_not_external_but_other_writers_are() {
        let dir = tempfile::tempdir().unwrap();
        let (shared, file) = shared_at(dir.path());
        // Salvataggi dell'app (preferenze e chiavi dell'app): non sono cambi esterni.
        shared
            .update_config(|c| {
                c.poll_interval_seconds = 120;
                Ok(())
            })
            .unwrap();
        assert!(!absorb(&shared, &file, CONFIG_ONLY));
        lock_settings(&shared).autostart = false;
        shared.save_settings().unwrap();
        assert!(!absorb(&shared, &file, CONFIG_ONLY));

        // Un altro processo (CLI, VS Code) cambia la config: si rilegge e si applica.
        config::update(&file, |c| {
            c.poll_interval_seconds = 300;
            c.theme = swapdeck_core::config::Theme::Dark;
            Ok(())
        })
        .unwrap();
        assert!(absorb(&shared, &file, CONFIG_ONLY));
        let s = lock_settings(&shared).clone();
        assert_eq!(s.poll_interval, Duration::from_secs(300));
        assert_eq!(s.theme, swapdeck_core::config::Theme::Dark);
        assert!(!s.autostart, "le chiavi dell'app restano");
        // Stessa config riletta (nessun cambio vero): niente.
        assert!(!absorb(&shared, &file, CONFIG_ONLY));
        // Le impostazioni assorbite non vanno riscritte.
        shared.save_settings().unwrap();
        assert!(!absorb(&shared, &file, CONFIG_ONLY));

        // File non valido: si ignora, restano quelle in memoria.
        write(&file, "{ non json");
        assert!(!absorb(&shared, &file, CONFIG_ONLY));
        assert_eq!(
            lock_settings(&shared).poll_interval,
            Duration::from_secs(300)
        );
        // Il segnale di VS Code conta sempre.
        assert!(absorb(
            &shared,
            &file,
            Changes {
                config: false,
                signal: true
            }
        ));
    }

    #[test]
    fn an_external_change_found_by_an_own_write_is_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        let (shared, file) = shared_at(dir.path());
        shared.save_settings().unwrap();
        lock_settings(&shared).autostart = false;
        shared.save_settings().unwrap();
        assert!(!shared.take_external(), "solo scritture dell'app");
        // Un altro processo cambia la config e l'app salva prima del controllo.
        config::update(&file, |c| {
            c.poll_interval_seconds = 240;
            Ok(())
        })
        .unwrap();
        lock_settings(&shared).mini_compact = true;
        shared.save_settings().unwrap();
        assert!(shared.take_external(), "cambio esterno segnalato");
        assert!(!shared.take_external(), "una volta sola");
        assert_eq!(
            lock_settings(&shared).poll_interval,
            Duration::from_secs(240)
        );
        assert!(
            !absorb(&shared, &file, CONFIG_ONLY),
            "il file è quello già visto"
        );
        // Lo stesso con una modifica delle preferenze dall'app.
        config::update(&file, |c| {
            c.poll_interval_seconds = 180;
            Ok(())
        })
        .unwrap();
        shared
            .update_config(|c| {
                c.language = swapdeck_core::config::Language::En;
                Ok(())
            })
            .unwrap();
        assert!(shared.take_external());
        assert_eq!(
            lock_settings(&shared).poll_interval,
            Duration::from_secs(180)
        );
    }

    #[test]
    fn startup_config_is_known() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(dir.path());
        paths.ensure().unwrap();
        config::update(&paths.config_file(), |c| {
            c.poll_interval_seconds = 90;
            Ok(())
        })
        .unwrap();
        let (shared, file) = shared_at(dir.path());
        let loaded = config::load(&file).unwrap();
        shared.note_config(&loaded.config);
        assert!(!absorb(&shared, &file, CONFIG_ONLY));
    }

    fn lock_settings(shared: &Shared) -> std::sync::MutexGuard<'_, Settings> {
        crate::state::lock(&shared.settings)
    }
}
