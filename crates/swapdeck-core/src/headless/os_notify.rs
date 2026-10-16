//! Notifiche di sistema del motore senza interfaccia (`--notify=os|stdout|both`).
//!
//! - Windows: toast con l'AppUserModelID dell'app (`identifier` di Tauri), registrato per utente
//!   in `HKCU\Software\Classes\AppUserModelId` con nome e icona ([`register_app_id`], anche dallo
//!   zip portatile) o dall'installer sul collegamento del menu Start; senza nessuno dei due, quello
//!   di PowerShell (ripiego di `notify-rust`). Windows tiene in memoria nome e icona di un AUMID
//!   dal suo primo toast: se quello è partito prima della registrazione, «SwapDeck» compare solo
//!   dopo un nuovo accesso (la voce in `Notifications\Settings` non c'entra, provato).
//! - Linux: D-Bus (`org.freedesktop.Notifications`); senza bus di sessione o sessione grafica si
//!   salta senza errori.
//! - macOS: Centro notifiche, con l'identificativo dell'app se il bundle è installato.
//!
//! Un errore non ferma mai il motore: il chiamante lo scrive in una riga di log.

use std::ffi::OsString;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use crate::notify::Severity;

/// `identifier` di `apps/desktop/src-tauri/tauri.conf.json` (AUMID su Windows, bundle su macOS).
pub const APP_IDENTIFIER: &str = "com.swapdeck.desktop";
/// `productName` di Tauri: nome del collegamento nel menu Start e del bundle macOS.
pub const PRODUCT_NAME: &str = "SwapDeck";
/// Chiave per utente dell'AUMID sotto `HKEY_CURRENT_USER` (metodo per le app non pacchettizzate);
/// la toglie anche il disinstallatore NSIS (`apps/desktop/src-tauri/windows/hooks.nsh`).
pub const AUMID_KEY: &str = r"Software\Classes\AppUserModelId\com.swapdeck.desktop";
/// Icona dei toast nella cartella dati standard dell'utente (non `SWAPDECK_HOME`: la chiave del
/// registro è una sola per utente). La toglie anche il disinstallatore NSIS.
pub const ICON_FILE: &str = "notify-icon.png";
/// Icona incorporata nel binario: vale anche per lo zip portatile.
#[cfg(windows)]
const ICON_BYTES: &[u8] = include_bytes!("../../../../apps/desktop/src-tauri/icons/128x128.png");
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
    if let Some(id) = windows_app_id(
        RegistryStore::open().is_some_and(|s| aumid_registered(&s)),
        &[
            std::env::var_os("APPDATA").map(PathBuf::from),
            std::env::var_os("ProgramData").map(PathBuf::from),
        ],
    ) {
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

/// AUMID per i toast di Windows: quello dell'app se è registrato nel registro (`registered`) o se
/// l'installer ha messo il collegamento nel menu Start (per utente in `APPDATA`, per macchina in
/// `ProgramData`); `None` = ripiego di PowerShell (un AUMID sconosciuto non mostrerebbe nulla).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn windows_app_id(registered: bool, roots: &[Option<PathBuf>]) -> Option<&'static str> {
    if registered {
        return Some(APP_IDENTIFIER);
    }
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

/// Esito di [`register_app_id`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// Chiave e icona già a posto: nessuna scrittura.
    Unchanged,
    /// Scritte (prima volta, o valori e icona diversi).
    Updated,
    /// Non serve su questo sistema (Linux, macOS).
    NotNeeded,
}

/// Registra per utente l'AUMID dell'app (nome «SwapDeck» e icona), senza amministratore: si scrive
/// solo ciò che manca o è diverso. `Err` con un testo breve per il log: le notifiche ripiegano sul
/// comportamento di prima (collegamento del menu Start o PowerShell) e il motore continua.
pub fn register_app_id() -> Result<Registration, String> {
    #[cfg(windows)]
    {
        let base = crate::paths::AppPaths::from_system()
            .ok_or("cartella dati dell'utente non determinabile")?;
        let icon = base.data_dir.join(ICON_FILE);
        crate::paths::create_private_dir(&base.data_dir)
            .map_err(|e| format!("cartella dell'icona: {e}"))?;
        let icon_written = ensure_icon(&icon, ICON_BYTES).map_err(|e| format!("icona: {e}"))?;
        let values = aumid_values(&icon).ok_or("percorso dell'icona non rappresentabile")?;
        let mut store = RegistryStore::create()?;
        let key_written = ensure_aumid(&mut store, &values)?;
        Ok(if icon_written || key_written {
            Registration::Updated
        } else {
            Registration::Unchanged
        })
    }
    #[cfg(not(windows))]
    Ok(Registration::NotNeeded)
}

/// Valori della chiave dell'AUMID (finta nei test).
pub(crate) trait AumidStore {
    fn get(&self, name: &str) -> Option<String>;
    fn set(&mut self, name: &str, value: &str) -> Result<(), String>;
}

/// Nome mostrato e icona (percorso assoluto `.png`); `None` se il percorso non è Unicode valido.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn aumid_values(icon: &Path) -> Option<[(&'static str, String); 2]> {
    Some([
        ("DisplayName", PRODUCT_NAME.to_owned()),
        ("IconUri", icon.to_str()?.to_owned()),
    ])
}

/// Scrive solo i valori mancanti o diversi; `true` se ha scritto qualcosa.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn ensure_aumid(
    store: &mut impl AumidStore,
    values: &[(&str, String)],
) -> Result<bool, String> {
    let mut written = false;
    for (name, value) in values {
        if store.get(name).as_deref() != Some(value.as_str()) {
            store.set(name, value)?;
            written = true;
        }
    }
    Ok(written)
}

/// AUMID utilizzabile: chiave con il nome dell'app.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn aumid_registered(store: &impl AumidStore) -> bool {
    store.get("DisplayName").is_some_and(|n| !n.is_empty())
}

/// Scrive l'icona solo se manca o è diversa, in modo atomico (file temporaneo nella stessa
/// cartella e rinomina, che sostituisce un eventuale collegamento senza seguirlo).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn ensure_icon(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    let same = std::fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
        && std::fs::read(path).is_ok_and(|current| current == bytes);
    if same {
        return Ok(false);
    }
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "percorso senza cartella"))?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(true)
}

/// Chiave dell'AUMID nel registro di Windows (`winreg`, senza `unsafe` nel progetto).
#[cfg(windows)]
struct RegistryStore(winreg::RegKey);

#[cfg(windows)]
impl RegistryStore {
    fn hkcu() -> winreg::RegKey {
        winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
    }

    /// Solo lettura; `None` se la chiave non c'è.
    fn open() -> Option<Self> {
        Self::hkcu()
            .open_subkey_with_flags(AUMID_KEY, winreg::enums::KEY_READ)
            .ok()
            .map(Self)
    }

    fn create() -> Result<Self, String> {
        Self::hkcu()
            .create_subkey_with_flags(
                AUMID_KEY,
                winreg::enums::KEY_READ | winreg::enums::KEY_WRITE,
            )
            .map(|(key, _)| Self(key))
            .map_err(|e| format!("registro non scrivibile: {e}"))
    }
}

#[cfg(windows)]
impl AumidStore for RegistryStore {
    fn get(&self, name: &str) -> Option<String> {
        self.0.get_value::<String, _>(name).ok()
    }

    fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
        self.0
            .set_value(name, &value.to_owned())
            .map_err(|e| format!("registro non scrivibile ({name}): {e}"))
    }
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
        // Niente installazione né registrazione: ripiego.
        assert_eq!(windows_app_id(false, &roots), None);
        assert_eq!(windows_app_id(false, &[]), None);
        // Registrato nel registro (zip portatile): basta quello.
        assert_eq!(windows_app_id(true, &[]), Some(APP_IDENTIFIER));
        // Una cartella con il nome del collegamento non conta.
        std::fs::create_dir_all(programs(machine.path()).join("SwapDeck.lnk")).unwrap();
        assert_eq!(windows_app_id(false, &roots), None);
        // Installazione per utente.
        std::fs::write(programs(user.path()).join("SwapDeck.lnk"), b"lnk").unwrap();
        assert_eq!(windows_app_id(false, &roots), Some(APP_IDENTIFIER));
    }

    #[test]
    fn windows_app_id_per_machine_install() {
        let machine = tempfile::tempdir().unwrap();
        std::fs::write(programs(machine.path()).join("SwapDeck.lnk"), b"lnk").unwrap();
        assert_eq!(
            windows_app_id(false, &[None, Some(machine.path().to_path_buf())]),
            Some(APP_IDENTIFIER)
        );
    }

    /// Registro finto: valori in memoria e scritture contate.
    #[derive(Default)]
    struct FakeStore {
        values: std::collections::BTreeMap<String, String>,
        writes: usize,
        read_only: bool,
    }

    impl AumidStore for FakeStore {
        fn get(&self, name: &str) -> Option<String> {
            self.values.get(name).cloned()
        }

        fn set(&mut self, name: &str, value: &str) -> Result<(), String> {
            if self.read_only {
                return Err("accesso negato".to_owned());
            }
            self.writes += 1;
            self.values.insert(name.to_owned(), value.to_owned());
            Ok(())
        }
    }

    #[test]
    fn aumid_registration_is_idempotent() {
        let icon = std::env::temp_dir().join(ICON_FILE);
        let values = aumid_values(&icon).unwrap();
        let mut store = FakeStore::default();
        assert!(!aumid_registered(&store));
        // Prima volta: nome e icona.
        assert_eq!(ensure_aumid(&mut store, &values), Ok(true));
        assert_eq!(store.writes, 2);
        assert_eq!(store.values["DisplayName"], "SwapDeck");
        assert_eq!(store.values["IconUri"], icon.to_str().unwrap());
        assert!(aumid_registered(&store));
        // Seconda volta: nessuna scrittura.
        assert_eq!(ensure_aumid(&mut store, &values), Ok(false));
        assert_eq!(store.writes, 2);
        // Un valore diverso (icona spostata): si riscrive solo quello.
        store.values.insert("IconUri".into(), "vecchia.png".into());
        assert_eq!(ensure_aumid(&mut store, &values), Ok(true));
        assert_eq!(store.writes, 3);
        // Nome vuoto: non utilizzabile.
        store.values.insert("DisplayName".into(), String::new());
        assert!(!aumid_registered(&store));
    }

    #[test]
    fn aumid_registration_with_read_only_registry() {
        let values = aumid_values(&std::env::temp_dir().join(ICON_FILE)).unwrap();
        let mut store = FakeStore {
            read_only: true,
            ..FakeStore::default()
        };
        assert!(ensure_aumid(&mut store, &values).is_err());
        assert!(!aumid_registered(&store));
        // Già registrato: nessuna scrittura, quindi nessun errore.
        let mut done = FakeStore::default();
        ensure_aumid(&mut done, &values).unwrap();
        done.read_only = true;
        assert_eq!(ensure_aumid(&mut done, &values), Ok(false));
    }

    #[test]
    fn icon_written_only_when_missing_or_different() {
        let dir = tempfile::tempdir().unwrap();
        let icon = dir.path().join(ICON_FILE);
        assert!(ensure_icon(&icon, b"png-1").unwrap());
        assert!(!ensure_icon(&icon, b"png-1").unwrap());
        assert!(ensure_icon(&icon, b"png-2").unwrap());
        assert_eq!(std::fs::read(&icon).unwrap(), b"png-2");
        // Una cartella al posto dell'icona: errore, niente panic.
        let blocked = dir.path().join("blocked");
        std::fs::create_dir_all(&blocked).unwrap();
        assert!(ensure_icon(&blocked, b"png").is_err());
        // Nessun file temporaneo rimasto.
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn registration_key_and_other_systems() {
        assert!(AUMID_KEY.ends_with(&format!("\\{APP_IDENTIFIER}")));
        assert!(AUMID_KEY.starts_with("Software\\Classes\\AppUserModelId\\"));
        if !cfg!(windows) {
            assert_eq!(register_app_id(), Ok(Registration::NotNeeded));
        }
    }

    /// Il disinstallatore toglie la stessa chiave e la stessa icona.
    #[test]
    fn nsis_hook_matches_constants() {
        let hook = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src-tauri/windows/hooks.nsh");
        let text = std::fs::read_to_string(hook).unwrap();
        assert!(text.contains(&format!("DeleteRegKey HKCU \"{AUMID_KEY}\"")));
        assert!(text.contains(&format!("$APPDATA\\{}\\{ICON_FILE}", crate::paths::APP_DIR)));
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
