//! Ambiente dei processi figli quando l'app gira dentro un'AppImage.
//!
//! L'`AppRun` dell'AppImage (linuxdeploy e plugin gtk) esporta variabili che puntano dentro il
//! punto di montaggio: `PYTHONHOME`, `PYTHONPATH`, `PERLLIB`, `LD_LIBRARY_PATH`, `GTK_*`,
//! `GIO_MODULE_DIR`, e mette le sue cartelle in testa a `PATH` e `XDG_DATA_DIRS`. Servono
//! all'app e a WebKit, ma ereditate da cswap (Python installato da uv), uv, claude, dal
//! terminale o da `xdg-open` li rompono: con `PYTHONHOME` sbagliata Python non parte nemmeno.
//! Ai figli si tolgono quindi le voci sotto `APPDIR` e le variabili proprie dell'AppImage.

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// Variabili dell'AppImage che non devono arrivare ai figli (un'app da `.deb` lanciata da
/// lì le prenderebbe per sue).
const APPIMAGE_VARS: [&str; 4] = ["APPDIR", "APPIMAGE", "ARGV0", "OWD"];

/// Modifica all'ambiente di un figlio: `None` toglie la variabile.
pub type EnvChange = (OsString, Option<OsString>);

/// Modifiche per i figli, dato l'ambiente del processo e il suo eseguibile. Vuoto se l'app non
/// gira dentro `APPDIR` (`APPDIR` ereditata da un altro processo non conta). Nelle variabili
/// con più percorsi separati da `:` si tolgono solo le voci sotto `APPDIR` (e quelle vuote);
/// la variabile sparisce se non resta niente. I valori non UTF-8 restano come sono.
pub fn appimage_changes<I>(vars: I, exe: &Path) -> Vec<EnvChange>
where
    I: IntoIterator<Item = (OsString, OsString)>,
{
    let vars: Vec<(OsString, OsString)> = vars.into_iter().collect();
    let Some(appdir) = vars
        .iter()
        .find(|(k, _)| k == "APPDIR")
        .map(|(_, v)| Path::new(v))
        .filter(|d| d.is_absolute() && d.components().count() > 1 && exe.starts_with(d))
    else {
        return Vec::new();
    };
    let mut changes = Vec::new();
    for (key, value) in &vars {
        if APPIMAGE_VARS.iter().any(|v| key == OsStr::new(v)) {
            changes.push((key.clone(), None));
            continue;
        }
        let Some(text) = value.to_str() else {
            continue;
        };
        let entries: Vec<&str> = text.split(':').collect();
        if !entries
            .iter()
            .any(|e| !e.is_empty() && Path::new(e).starts_with(appdir))
        {
            continue;
        }
        let kept: Vec<&str> = entries
            .into_iter()
            .filter(|e| !e.is_empty() && !Path::new(e).starts_with(appdir))
            .collect();
        let new = (!kept.is_empty()).then(|| OsString::from(kept.join(":")));
        changes.push((key.clone(), new));
    }
    changes
}

/// Modifiche per questo processo, calcolate una volta sola.
fn current_changes() -> &'static [EnvChange] {
    static CHANGES: OnceLock<Vec<EnvChange>> = OnceLock::new();
    CHANGES.get_or_init(|| {
        if !cfg!(target_os = "linux") {
            return Vec::new();
        }
        std::env::current_exe()
            .map(|exe| appimage_changes(std::env::vars_os(), &exe))
            .unwrap_or_default()
    })
}

/// Applica al comando le modifiche dell'ambiente (nessuna fuori da un'AppImage). Da chiamare
/// prima delle variabili proprie del comando, che così restano valide.
pub fn sanitize(cmd: &mut Command) {
    for (key, value) in current_changes() {
        match value {
            Some(v) => cmd.env(key, v),
            None => cmd.env_remove(key),
        };
    }
}

// Percorsi in stile Linux, dove esistono le AppImage: su Windows `/tmp/...` non è assoluto e la
// funzione, giustamente, non riconosce nessuna AppImage.
#[cfg(all(test, unix))]
mod tests {
    use super::*;

    const APPDIR: &str = "/tmp/.mount_SwapDeX";

    fn env(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)))
            .collect()
    }

    fn exe_inside() -> std::path::PathBuf {
        Path::new(APPDIR).join("usr/bin/swapdeck")
    }

    fn change<'a>(changes: &'a [EnvChange], key: &str) -> Option<&'a Option<OsString>> {
        changes.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn appimage_env() -> Vec<(OsString, OsString)> {
        env(&[
            ("APPDIR", APPDIR),
            ("APPIMAGE", "/home/u/SwapDeck.AppImage"),
            ("ARGV0", "./SwapDeck.AppImage"),
            ("OWD", "/home/u"),
            ("PYTHONHOME", "/tmp/.mount_SwapDeX/usr/"),
            ("PYTHONPATH", "/tmp/.mount_SwapDeX/usr/share/pyshared/:"),
            (
                "PATH",
                "/tmp/.mount_SwapDeX/usr/bin/:/tmp/.mount_SwapDeX/bin/:/home/u/.local/bin:/usr/bin",
            ),
            (
                "LD_LIBRARY_PATH",
                "/tmp/.mount_SwapDeX/usr/lib/:/tmp/.mount_SwapDeX/usr/lib/x86_64-linux-gnu/",
            ),
            (
                "XDG_DATA_DIRS",
                "/tmp/.mount_SwapDeX/usr/share:/usr/share:/usr/local/share/",
            ),
            ("GTK_PATH", "/tmp/.mount_SwapDeX//usr/lib/gtk-3.0"),
            ("GIO_MODULE_DIR", "/tmp/.mount_SwapDeX//usr/lib/gio/modules"),
            ("HOME", "/home/u"),
            ("LANG", "it_IT.UTF-8"),
        ])
    }

    #[test]
    fn inside_appimage_removes_python_and_mount_paths() {
        let changes = appimage_changes(appimage_env(), &exe_inside());
        for key in [
            "PYTHONHOME",
            "PYTHONPATH",
            "LD_LIBRARY_PATH",
            "GTK_PATH",
            "GIO_MODULE_DIR",
            "APPDIR",
            "APPIMAGE",
            "ARGV0",
            "OWD",
        ] {
            assert_eq!(change(&changes, key), Some(&None), "{key}");
        }
        assert_eq!(
            change(&changes, "PATH"),
            Some(&Some("/home/u/.local/bin:/usr/bin".into()))
        );
        assert_eq!(
            change(&changes, "XDG_DATA_DIRS"),
            Some(&Some("/usr/share:/usr/local/share/".into()))
        );
        assert_eq!(change(&changes, "HOME"), None);
        assert_eq!(change(&changes, "LANG"), None);
    }

    #[test]
    fn outside_appimage_changes_nothing() {
        // `.deb` avviato da un processo che gira in un'AppImage: variabili ereditate.
        let changes = appimage_changes(appimage_env(), Path::new("/usr/bin/swapdeck"));
        assert!(changes.is_empty(), "{changes:?}");
        let plain = env(&[("PATH", "/usr/bin"), ("PYTHONHOME", "/opt/py")]);
        assert!(appimage_changes(plain, &exe_inside()).is_empty());
    }

    #[test]
    fn unusable_appdir_changes_nothing() {
        for appdir in ["", "relativa/mount", "/"] {
            let vars = env(&[("APPDIR", appdir), ("PYTHONHOME", "/x")]);
            let exe = Path::new("/usr/bin/swapdeck");
            assert!(appimage_changes(vars, exe).is_empty(), "{appdir:?}");
        }
    }

    #[test]
    fn prefix_match_is_by_path_component() {
        // `/tmp/.mount_SwapDeXY` non sta sotto `/tmp/.mount_SwapDeX`.
        let vars = env(&[
            ("APPDIR", APPDIR),
            ("PATH", "/tmp/.mount_SwapDeXY/bin:/usr/bin"),
        ]);
        let changes = appimage_changes(vars, &exe_inside());
        assert_eq!(change(&changes, "PATH"), None);
    }

    #[test]
    fn non_utf8_values_are_kept() {
        use std::os::unix::ffi::OsStringExt;
        let mut vars = env(&[("APPDIR", APPDIR)]);
        vars.push(("STRANA".into(), OsString::from_vec(vec![0xff, b'/'])));
        let changes = appimage_changes(vars, &exe_inside());
        assert_eq!(change(&changes, "STRANA"), None);
    }

    #[test]
    fn sanitize_outside_appimage_keeps_environment() {
        // Il processo dei test non gira in un'AppImage: il comando resta com'è.
        let mut cmd = Command::new("/bin/true");
        sanitize(&mut cmd);
        assert_eq!(cmd.get_envs().count(), 0);
    }
}
