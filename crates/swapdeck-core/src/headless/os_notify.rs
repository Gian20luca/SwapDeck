//! Notifiche di sistema del motore senza interfaccia (`--notify=os|stdout|both`).
//!
//! - Windows: toast con l'AppUserModelID dell'app installata (`identifier` di Tauri, registrato
//!   dall'installer sul collegamento del menu Start); senza collegamento, quello di PowerShell
//!   (ripiego di `notify-rust`, funziona anche dallo zip portatile).
//! - Linux: D-Bus (`org.freedesktop.Notifications`); senza bus di sessione o sessione grafica si
//!   salta senza errori.
//! - macOS: Centro notifiche, con l'identificativo dell'app se il bundle è installato.
//!
//! Un errore non ferma mai il motore: il chiamante lo scrive in una riga di log.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use crate::notify::Severity;

/// `identifier` di `apps/desktop/src-tauri/tauri.conf.json` (AUMID su Windows, bundle su macOS).
pub const APP_IDENTIFIER: &str = "com.swapdeck.desktop";
/// `productName` di Tauri: nome del collegamento nel menu Start e del bundle macOS.
pub const PRODUCT_NAME: &str = "SwapDeck";
/// Attesa massima di una notifica (D-Bus senza risposta, servizio bloccato).
const SHOW_TIMEOUT: Duration = Duration::from_secs(5);

/// Dove vanno le notifiche del motore senza interfaccia.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NotifyChannel {
    /// Notifiche di sistema (default della riga di comando).
    #[default]
    Os,
    /// Solo righe JSON su stdout (chi ha avviato il motore le mostra: estensione VS Code).
    Stdout,
    Both,
}

impl NotifyChannel {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "os" => Some(Self::Os),
            "stdout" => Some(Self::Stdout),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    pub fn to_os(self) -> bool {
        matches!(self, Self::Os | Self::Both)
    }

    pub fn to_stdout(self) -> bool {
        matches!(self, Self::Stdout | Self::Both)
    }
}

/// Esito di una notifica di sistema riuscita o saltata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsOutcome {
    Shown,
    /// Nessun posto dove mostrarla (per esempio Linux senza sessione grafica).
    Skipped(&'static str),
}

/// Chi mostra le notifiche col sistema operativo (finto nei test).
pub trait OsNotifier {
    /// `Err` con un testo breve per il log; mai un panic.
    fn show(&mut self, title: &str, body: &str, severity: Severity) -> Result<OsOutcome, String>;
}

/// Notifiche reali con `notify-rust`, in un thread a parte con tempo massimo.
#[derive(Debug, Default)]
pub struct SystemNotifier;

impl OsNotifier for SystemNotifier {
    fn show(&mut self, title: &str, body: &str, severity: Severity) -> Result<OsOutcome, String> {
        if cfg!(all(unix, not(target_os = "macos")))
            && !graphical_session(
                |k| std::env::var_os(k),
                |p| std::fs::symlink_metadata(p).is_ok(),
            )
        {
            return Ok(OsOutcome::Skipped("nessuna sessione grafica"));
        }
        let (tx, rx) = mpsc::channel();
        let (title, body) = (title.to_owned(), body.to_owned());
        std::thread::Builder::new()
            .name("os-notify".into())
            .spawn(move || {
                let _ = tx.send(show_blocking(&title, &body, severity));
            })
            .map_err(|e| format!("thread non avviato: {e}"))?;
        match rx.recv_timeout(SHOW_TIMEOUT) {
            Ok(result) => result.map(|()| OsOutcome::Shown),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "nessuna risposta entro {} s",
                SHOW_TIMEOUT.as_secs()
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err("interrotta".to_owned()),
        }
    }
}

fn show_blocking(title: &str, body: &str, severity: Severity) -> Result<(), String> {
    let mut n = notify_rust::Notification::new();
    n.summary(title).body(body).appname(PRODUCT_NAME);
    #[cfg(windows)]
    if let Some(id) = windows_app_id(&[
        std::env::var_os("APPDATA").map(PathBuf::from),
        std::env::var_os("ProgramData").map(PathBuf::from),
    ]) {
        n.app_id(id);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    n.urgency(match severity {
        Severity::Critical => notify_rust::Urgency::Critical,
        Severity::Warning => notify_rust::Urgency::Normal,
        Severity::Info => notify_rust::Urgency::Low,
    });
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    let _ = severity;
    #[cfg(target_os = "macos")]
    prepare_macos();
    n.show().map(|_| ()).map_err(|e| e.to_string())
}

/// AUMID per i toast di Windows: quello dell'app se l'installer ha messo il collegamento nel menu
/// Start (per utente in `APPDATA`, per macchina in `ProgramData`); `None` = ripiego di PowerShell.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_app_id(roots: &[Option<PathBuf>]) -> Option<&'static str> {
    let shortcut = format!("{PRODUCT_NAME}.lnk");
    roots
        .iter()
        .flatten()
        .map(|root| {
            root.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join(&shortcut)
        })
        // Senza seguire collegamenti simbolici: conta solo un file vero.
        .any(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file()))
        .then_some(APP_IDENTIFIER)
}

/// Linux e BSD: bus di sessione (variabile o socket in `XDG_RUNTIME_DIR`) e indizio di una
/// sessione grafica (display X11 o Wayland, tipo di sessione o desktop).
#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
pub(crate) fn graphical_session(
    env: impl Fn(&str) -> Option<OsString>,
    exists: impl Fn(&Path) -> bool,
) -> bool {
    let set = |k: &str| env(k).is_some_and(|v| !v.is_empty());
    let bus = set("DBUS_SESSION_BUS_ADDRESS")
        || env("XDG_RUNTIME_DIR")
            .filter(|d| !d.is_empty())
            .is_some_and(|d| exists(&Path::new(&d).join("bus")));
    let session_type = env("XDG_SESSION_TYPE")
        .is_some_and(|t| t.eq_ignore_ascii_case("x11") || t.eq_ignore_ascii_case("wayland"));
    let display =
        set("DISPLAY") || set("WAYLAND_DISPLAY") || set("XDG_CURRENT_DESKTOP") || session_type;
    bus && display
}

/// macOS: bundle dell'app in `/Applications` o `~/Applications`.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn macos_app_installed(home: Option<&Path>, exists: impl Fn(&Path) -> bool) -> bool {
    let bundle = format!("{PRODUCT_NAME}.app");
    exists(&Path::new("/Applications").join(&bundle))
        || home.is_some_and(|h| exists(&h.join("Applications").join(&bundle)))
}

/// Identificativo dell'app per il Centro notifiche, una volta per processo (`set_application`
/// rifiuta una seconda chiamata). Senza bundle installato resta il default di `notify-rust`.
#[cfg(target_os = "macos")]
fn prepare_macos() {
    static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if macos_app_installed(home.as_deref(), |p| p.is_dir()) {
            let _ = notify_rust::set_application(APP_IDENTIFIER);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_parse_and_routes() {
        assert_eq!(NotifyChannel::default(), NotifyChannel::Os);
        assert_eq!(NotifyChannel::parse("os"), Some(NotifyChannel::Os));
        assert_eq!(NotifyChannel::parse("stdout"), Some(NotifyChannel::Stdout));
        assert_eq!(NotifyChannel::parse("both"), Some(NotifyChannel::Both));
        assert_eq!(NotifyChannel::parse("OS"), None);
        assert_eq!(NotifyChannel::parse(""), None);
        let routes = |c: NotifyChannel| (c.to_os(), c.to_stdout());
        assert_eq!(routes(NotifyChannel::Os), (true, false));
        assert_eq!(routes(NotifyChannel::Stdout), (false, true));
        assert_eq!(routes(NotifyChannel::Both), (true, true));
    }

    fn programs(root: &Path) -> PathBuf {
        let dir = root.join("Microsoft/Windows/Start Menu/Programs");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn windows_app_id_uses_installed_shortcut() {
        let user = tempfile::tempdir().unwrap();
        let machine = tempfile::tempdir().unwrap();
        let roots = [
            Some(user.path().to_path_buf()),
            None,
            Some(machine.path().to_path_buf()),
        ];
        // Niente installazione: ripiego.
        assert_eq!(windows_app_id(&roots), None);
        assert_eq!(windows_app_id(&[]), None);
        // Una cartella con il nome del collegamento non conta.
        std::fs::create_dir_all(programs(machine.path()).join("SwapDeck.lnk")).unwrap();
        assert_eq!(windows_app_id(&roots), None);
        // Installazione per utente.
        std::fs::write(programs(user.path()).join("SwapDeck.lnk"), b"lnk").unwrap();
        assert_eq!(windows_app_id(&roots), Some(APP_IDENTIFIER));
    }

    #[test]
    fn windows_app_id_per_machine_install() {
        let machine = tempfile::tempdir().unwrap();
        std::fs::write(programs(machine.path()).join("SwapDeck.lnk"), b"lnk").unwrap();
        assert_eq!(
            windows_app_id(&[None, Some(machine.path().to_path_buf())]),
            Some(APP_IDENTIFIER)
        );
    }

    fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn linux_session_needs_bus_and_display() {
        let none = |_: &Path| false;
        let bus_socket = |p: &Path| p == Path::new("/run/user/1000/bus");
        // Nessuna sessione (cron, ssh, container): si salta.
        assert!(!graphical_session(env_of(&[]), none));
        assert!(!graphical_session(env_of(&[("DISPLAY", ":0")]), none));
        assert!(!graphical_session(
            env_of(&[("DBUS_SESSION_BUS_ADDRESS", "unix:path=/x")]),
            none
        ));
        assert!(!graphical_session(
            env_of(&[("DBUS_SESSION_BUS_ADDRESS", ""), ("DISPLAY", ":0")]),
            none
        ));
        // Bus e display.
        assert!(graphical_session(
            env_of(&[
                ("DBUS_SESSION_BUS_ADDRESS", "unix:path=/x"),
                ("DISPLAY", ":0")
            ]),
            none
        ));
        // Servizio utente: bus dal socket di XDG_RUNTIME_DIR, Wayland o tipo di sessione.
        assert!(graphical_session(
            env_of(&[
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
                ("WAYLAND_DISPLAY", "wayland-0")
            ]),
            bus_socket
        ));
        assert!(graphical_session(
            env_of(&[
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
                ("XDG_SESSION_TYPE", "Wayland")
            ]),
            bus_socket
        ));
        assert!(!graphical_session(
            env_of(&[
                ("XDG_RUNTIME_DIR", "/run/user/1000"),
                ("XDG_SESSION_TYPE", "tty")
            ]),
            bus_socket
        ));
        assert!(!graphical_session(
            env_of(&[("XDG_RUNTIME_DIR", "/run/user/1000"), ("DISPLAY", ":0")]),
            none
        ));
    }

    #[test]
    fn macos_bundle_lookup() {
        let home = Path::new("/Users/u");
        let in_home = |p: &Path| p == Path::new("/Users/u/Applications/SwapDeck.app");
        let system = |p: &Path| p == Path::new("/Applications/SwapDeck.app");
        assert!(macos_app_installed(Some(home), in_home));
        assert!(!macos_app_installed(None, in_home));
        assert!(macos_app_installed(None, system));
        assert!(!macos_app_installed(Some(home), |_| false));
    }

    /// Le costanti devono restare uguali alla config di Tauri (AUMID dell'installer).
    #[test]
    fn constants_match_tauri_config() {
        let conf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src-tauri/tauri.conf.json");
        let text = std::fs::read_to_string(conf).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["identifier"], APP_IDENTIFIER);
        assert_eq!(json["productName"], PRODUCT_NAME);
    }
}
