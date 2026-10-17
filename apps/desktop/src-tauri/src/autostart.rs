//! Avvio automatico al login, gestito direttamente con `auto-launch`: il plugin di Tauri scrive
//! il percorso dell'eseguibile senza virgolette nella chiave `Run` di Windows, ambiguo se il
//! percorso contiene spazi (per esempio `C:\Users\Nome Cognome\...`). Le parti pure (percorso,
//! quoting, controlli, AppImage, voce spenta dal desktop) stanno in `swapdeck_core::autostart`.

use std::path::Path;

use auto_launch::{AutoLaunch, AutoLaunchBuilder};
#[cfg(target_os = "linux")]
use swapdeck_core::autostart::{appimage_target, desktop_entry_disabled};
use swapdeck_core::autostart::{launch_path, launch_path_problem};
use tauri::AppHandle;
#[cfg(target_os = "linux")]
use tauri::Manager as _;

/// Argomento con cui l'avvio automatico lancia l'app: parte nell'area di notifica, senza finestra.
pub const MINIMIZED_ARG: &str = "--minimized";

/// Nome della voce di avvio (uguale a quello usato dal plugin, così una voce già scritta si ritrova).
const ENTRY_NAME: &str = "SwapDeck";

/// L'avvio automatico tocca il sistema solo nelle build di rilascio: in sviluppo l'eseguibile
/// cambia a ogni build e non va registrato al login.
pub fn managed() -> bool {
    !cfg!(debug_assertions)
}

/// Cosa fare all'avvio dell'app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Startup {
    /// Non toccare niente (build di sviluppo o config non valida/solo in memoria).
    Skip,
    /// Primo avvio (chiave `autostart` assente): attiva l'avvio automatico e salva la scelta.
    Enable,
    /// Avvii successivi: vale lo stato del sistema, la config si allinea.
    Adopt,
}

pub fn startup_plan(managed: bool, persistent: bool, key_present: bool) -> Startup {
    match (managed && persistent, key_present) {
        (false, _) => Startup::Skip,
        (true, false) => Startup::Enable,
        (true, true) => Startup::Adopt,
    }
}

fn launcher(app: &AppHandle) -> Result<AutoLaunch, String> {
    let exe = current_launch_target(app)?;
    if let Some(problem) = launch_path_problem(&exe, cfg!(target_os = "macos")) {
        return Err(problem.to_owned());
    }
    let mut builder = AutoLaunchBuilder::new();
    builder
        .set_app_name(ENTRY_NAME)
        .set_app_path(&launch_path(&exe, std::env::consts::OS))
        .set_args(&[MINIMIZED_ARG]);
    #[cfg(target_os = "macos")]
    builder.set_use_launch_agent(true);
    builder.build().map_err(|e| e.to_string())
}

#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn current_launch_target(app: &AppHandle) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    {
        let env = app.env();
        if let Some(appimage) =
            appimage_target(env.appimage.as_deref(), env.appdir.as_deref(), &exe)
        {
            return path_string(&appimage);
        }
    }
    #[cfg(target_os = "macos")]
    let exe = exe.canonicalize().map_err(|e| e.to_string())?;
    path_string(&exe)
}

fn path_string(p: &Path) -> Result<String, String> {
    p.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "percorso dell'eseguibile non in UTF-8".to_owned())
}

pub fn is_enabled(app: &AppHandle) -> Result<bool, String> {
    let on = launcher(app)?.is_enabled().map_err(|e| e.to_string())?;
    #[cfg(target_os = "linux")]
    if on && linux_entry_disabled(app) {
        return Ok(false);
    }
    Ok(on)
}

#[cfg(target_os = "linux")]
fn linux_entry_disabled(app: &AppHandle) -> bool {
    use std::io::Read as _;
    // Stesso percorso di `auto-launch`: `~/.config/autostart/<nome>.desktop`.
    let Ok(home) = app.path().home_dir() else {
        return false;
    };
    let path = home
        .join(".config")
        .join("autostart")
        .join(format!("{ENTRY_NAME}.desktop"));
    let mut text = String::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(64 * 1024).read_to_string(&mut text))
        .is_ok_and(|_| desktop_entry_disabled(&text))
}

/// Applica la scelta al sistema. Attivando riscrive sempre la voce: corregge un percorso
/// cambiato (aggiornamento, spostamento) o scritto senza virgolette da versioni precedenti.
pub fn apply(app: &AppHandle, enabled: bool) -> Result<(), String> {
    if !managed() {
        return Ok(());
    }
    let launcher = launcher(app)?;
    if enabled {
        launcher.enable().map_err(|e| e.to_string())
    } else if launcher.is_enabled().map_err(|e| e.to_string())? {
        launcher.disable().map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_forces_only_first_run() {
        assert_eq!(startup_plan(true, true, false), Startup::Enable);
        assert_eq!(startup_plan(true, true, true), Startup::Adopt);
        // Config non valida o solo in memoria: il sistema non si tocca.
        assert_eq!(startup_plan(true, false, false), Startup::Skip);
        assert_eq!(startup_plan(true, false, true), Startup::Skip);
        // Build di sviluppo.
        assert_eq!(startup_plan(false, true, false), Startup::Skip);
    }
}
