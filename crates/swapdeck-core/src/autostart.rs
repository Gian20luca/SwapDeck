//! Parti pure dell'avvio automatico dell'app (la scrittura della voce sta nell'app desktop, con
//! `auto-launch`): qui per poterle provare su ogni sistema, anche senza GTK.
//!
//! Cosa scrive `auto-launch` 0.5 (verificato nel sorgente):
//! - Windows: valore `SwapDeck` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.
//! - Linux: `~/.config/autostart/SwapDeck.desktop` (sempre `~/.config`, `XDG_CONFIG_HOME` non
//!   conta) con `Exec=<percorso> --minimized`; attiva = il file esiste.
//! - macOS (con `set_use_launch_agent(true)`): `~/Library/LaunchAgents/SwapDeck.plist`, `Label`
//!   `SwapDeck`, `ProgramArguments` = percorso e `--minimized`, `RunAtLoad`; nessun
//!   `launchctl load`: vale dal login successivo. Attiva = il file esiste.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Percorso dell'eseguibile come lo legge chi avvia la voce, per il sistema `os`
/// (`std::env::consts::OS`).
pub fn launch_path(exe: &str, os: &str) -> String {
    match os {
        // La riga di `Run` va a CreateProcess: virgolette sempre (un percorso Windows non le contiene).
        "windows" => format!("\"{exe}\""),
        "linux" => desktop_exec_quote(exe),
        // macOS: il percorso è un elemento a sé di `ProgramArguments`.
        _ => exe.to_owned(),
    }
}

/// Quoting della chiave `Exec` dei file `.desktop` (specifica Desktop Entry).
pub fn desktop_exec_quote(s: &str) -> String {
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

/// Motivo per cui un percorso non va scritto nella voce di avvio (`None` = va bene).
/// - caratteri di controllo: un a capo spezzerebbe il file `.desktop` o il plist;
/// - macOS: `auto-launch` non fa l'escape XML del plist (`&`, `<`, `>` lo rendono non valido)
///   e un'app avviata in quarantena gira da un percorso casuale (App Translocation) che sparisce.
pub fn launch_path_problem(path: &str, macos: bool) -> Option<&'static str> {
    if path.chars().any(char::is_control) {
        return Some("percorso dell'eseguibile con caratteri di controllo");
    }
    if macos && path.contains("/AppTranslocation/") {
        return Some(
            "app avviata in quarantena (App Translocation): spostala in Applicazioni e riaprila",
        );
    }
    if macos && path.contains(['&', '<', '>']) {
        return Some("percorso dell'app con &, < o >: spostala in Applicazioni");
    }
    None
}

/// AppImage da registrare: `APPIMAGE` vale solo se l'eseguibile gira davvero dentro `APPDIR`
/// (il punto di montaggio dell'AppImage). Tauri la prende così com'è anche quando è ereditata
/// da un altro processo (per esempio un terminale aperto da un editor in AppImage): con un .deb
/// la voce punterebbe all'AppImage sbagliata.
pub fn appimage_target(
    appimage: Option<&OsStr>,
    appdir: Option<&OsStr>,
    exe: &Path,
) -> Option<PathBuf> {
    let appimage = Path::new(appimage?);
    let appdir = Path::new(appdir?);
    (appimage.is_absolute() && appdir.is_absolute() && exe.starts_with(appdir))
        .then(|| appimage.to_path_buf())
}

/// Voce `.desktop` spenta dal desktop senza toglierla (`Hidden=true`, oppure
/// `X-GNOME-Autostart-enabled=false`): `auto-launch` guarda solo se il file esiste, e
/// riscrivendola all'avvio annullerebbe la scelta fatta dall'utente nelle impostazioni del sistema.
pub fn desktop_entry_disabled(content: &str) -> bool {
    content.lines().any(|line| {
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        let (key, value) = (key.trim(), value.trim().to_ascii_lowercase());
        (key == "Hidden" && value == "true")
            || (key == "X-GNOME-Autostart-enabled" && value == "false")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_path_per_system() {
        let win = r"C:\Users\Nome Cognome\AppData\Local\SwapDeck\swapdeck.exe";
        assert_eq!(launch_path(win, "windows"), format!("\"{win}\""));
        assert_eq!(launch_path("/home/a b/x", "linux"), "\"/home/a b/x\"");
        assert_eq!(
            launch_path(
                "/Applications/SwapDeck.app/Contents/MacOS/swapdeck",
                "macos"
            ),
            "/Applications/SwapDeck.app/Contents/MacOS/swapdeck"
        );
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

    #[test]
    fn launch_path_problems() {
        assert_eq!(launch_path_problem("/usr/bin/swapdeck", false), None);
        assert_eq!(
            launch_path_problem("/Applications/SwapDeck.app/Contents/MacOS/swapdeck", true),
            None
        );
        assert!(launch_path_problem("/home/a\nExec=evil/x", false).is_some());
        let translocated =
            "/private/var/folders/x/T/AppTranslocation/ABC/d/SwapDeck.app/Contents/MacOS/swapdeck";
        assert!(launch_path_problem(translocated, true).is_some());
        assert!(
            launch_path_problem("/Users/a/A&B/SwapDeck.app/Contents/MacOS/swapdeck", true)
                .is_some()
        );
        // Su Linux `&` è gestito dal quoting di `Exec`.
        assert_eq!(launch_path_problem("/home/a/A&B/swapdeck", false), None);
    }

    #[cfg(unix)]
    #[test]
    fn appimage_only_when_running_inside_it() {
        let img = Some(OsStr::new("/home/a/Apps/SwapDeck.AppImage"));
        let dir = Some(OsStr::new("/tmp/.mount_SwapDeAbc"));
        let inside = Path::new("/tmp/.mount_SwapDeAbc/usr/bin/swapdeck");
        assert_eq!(
            appimage_target(img, dir, inside).as_deref(),
            Some(Path::new("/home/a/Apps/SwapDeck.AppImage"))
        );
        // .deb avviato con `APPIMAGE`/`APPDIR` ereditate da un altro AppImage.
        assert_eq!(
            appimage_target(img, dir, Path::new("/usr/bin/swapdeck")),
            None
        );
        assert_eq!(appimage_target(img, None, inside), None);
        assert_eq!(appimage_target(None, dir, inside), None);
        assert_eq!(
            appimage_target(Some(OsStr::new("rel.AppImage")), dir, inside),
            None
        );
    }

    #[test]
    fn desktop_entry_disabled_by_the_desktop() {
        let base = "[Desktop Entry]\nType=Application\nName=SwapDeck\nExec=/usr/bin/swapdeck --minimized\n";
        assert!(!desktop_entry_disabled(base));
        assert!(desktop_entry_disabled(&format!("{base}Hidden=true\n")));
        assert!(desktop_entry_disabled(&format!(
            "{base}X-GNOME-Autostart-enabled=false\n"
        )));
        assert!(!desktop_entry_disabled(&format!(
            "{base}X-GNOME-Autostart-enabled=true\n"
        )));
        assert!(!desktop_entry_disabled(&format!("{base}Hidden=false\n")));
    }
}
