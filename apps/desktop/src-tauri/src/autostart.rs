//! Avvio automatico al login, gestito direttamente con `auto-launch`: il plugin di Tauri scrive
//! il percorso dell'eseguibile senza virgolette nella chiave `Run` di Windows, ambiguo se il
//! percorso contiene spazi (per esempio `C:\Users\Nome Cognome\...`).

use std::path::Path;

use auto_launch::{AutoLaunch, AutoLaunchBuilder};
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

/// Percorso dell'eseguibile come lo legge chi avvia la voce.
pub fn launch_path(exe: &str) -> String {
    if cfg!(windows) {
        // La riga di `Run` va a CreateProcess: virgolette sempre (un percorso Windows non le contiene).
        format!("\"{exe}\"")
    } else if cfg!(target_os = "linux") {
        desktop_exec_quote(exe)
    } else {
        // macOS: il percorso è un elemento a sé di `ProgramArguments`.
        exe.to_owned()
    }
}

/// Quoting della chiave `Exec` dei file `.desktop` (specifica Desktop Entry).
fn desktop_exec_quote(s: &str) -> String {
    let s = s.replace('%', "%%");
    let reserved = |c: char| " \t\n\"'\\><~|&;$*?#()`".contains(c);
    if !s.chars().any(reserved) {
        return s;
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

fn launcher(app: &AppHandle) -> Result<AutoLaunch, String> {
    let exe = current_launch_target(app)?;
    let mut builder = AutoLaunchBuilder::new();
    builder
        .set_app_name(ENTRY_NAME)
        .set_app_path(&launch_path(&exe))
        .set_args(&[MINIMIZED_ARG]);
    #[cfg(target_os = "macos")]
    builder.set_use_launch_agent(true);
    builder.build().map_err(|e| e.to_string())
}

#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn current_launch_target(app: &AppHandle) -> Result<String, String> {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = app.env().appimage {
        return path_string(Path::new(&appimage));
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
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
    launcher(app)?.is_enabled().map_err(|e| e.to_string())
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

    #[test]
    fn launch_path_is_quoted() {
        let p = launch_path(r"C:\Users\Nome Cognome\AppData\Local\SwapDeck\swapdeck.exe");
        if cfg!(windows) {
            assert_eq!(
                p,
                r#""C:\Users\Nome Cognome\AppData\Local\SwapDeck\swapdeck.exe""#
            );
        } else if cfg!(target_os = "linux") {
            assert!(p.starts_with('"') && p.ends_with('"'));
        }
    }

    #[test]
    fn desktop_exec_quoting() {
        assert_eq!(
            desktop_exec_quote("/opt/swapdeck/swapdeck"),
            "/opt/swapdeck/swapdeck"
        );
        assert_eq!(desktop_exec_quote("/home/a b/x"), "\"/home/a b/x\"");
        assert_eq!(desktop_exec_quote("/h/$x\\y"), "\"/h/\\$x\\\\y\"");
        assert_eq!(desktop_exec_quote("/h/100%"), "/h/100%%");
    }
}
