//! Ricerca degli eseguibili `cswap` e `uv`.
//!
//! Ordine per cswap: variabile `SWAPDECK_CSWAP`, poi `UV_TOOL_BIN_DIR` (cartella
//! dei binari di `uv tool` scelta dall'utente), poi `PATH`, poi le cartelle
//! predefinite di `uv tool` (`$XDG_BIN_HOME`, poi `~/.local/bin`).
//! Le voci relative del `PATH` si ignorano: niente eseguibili presi dalla cartella corrente.

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::error::{CswapError, CswapErrorKind};

/// Variabile che indica il percorso assoluto di cswap.
pub const CSWAP_OVERRIDE_ENV: &str = "SWAPDECK_CSWAP";

/// Ambiente usato dalla ricerca (separato per i test).
#[derive(Debug, Clone, Default)]
pub struct SearchEnv {
    pub override_path: Option<OsString>,
    pub uv_tool_bin_dir: Option<OsString>,
    pub path: Option<OsString>,
    pub xdg_bin_home: Option<OsString>,
    pub home: Option<PathBuf>,
}

impl SearchEnv {
    /// Ambiente del processo corrente.
    pub fn from_process() -> Self {
        Self {
            override_path: env::var_os(CSWAP_OVERRIDE_ENV),
            uv_tool_bin_dir: env::var_os("UV_TOOL_BIN_DIR"),
            path: env::var_os("PATH"),
            xdg_bin_home: env::var_os("XDG_BIN_HOME"),
            home: directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()),
        }
    }
}

/// Nome del file eseguibile per l'OS corrente (`cswap.exe` su Windows).
pub fn exe_name(base: &str) -> String {
    format!("{base}{}", env::consts::EXE_SUFFIX)
}

/// Trova cswap; errore `NotInstalled` se manca.
pub fn locate_cswap(search: &SearchEnv) -> Result<PathBuf, CswapError> {
    if let Some(value) = search.override_path.as_ref().filter(|v| !v.is_empty()) {
        let path = PathBuf::from(value);
        return if path.is_absolute() && is_executable(&path) {
            Ok(path)
        } else {
            Err(CswapError::new(
                CswapErrorKind::NotInstalled,
                format!("{CSWAP_OVERRIDE_ENV} non indica un eseguibile con percorso assoluto"),
            ))
        };
    }
    let from_uv_dir = search
        .uv_tool_bin_dir
        .as_ref()
        .filter(|d| !d.is_empty())
        .and_then(|d| find_in([PathBuf::from(d)], &exe_name("cswap")));
    from_uv_dir
        .or_else(|| locate("cswap", search))
        .ok_or_else(|| {
            CswapError::new(
                CswapErrorKind::NotInstalled,
                "cswap non trovato (UV_TOOL_BIN_DIR, PATH e ~/.local/bin)",
            )
        })
}

/// Trova `uv` (PATH, poi `~/.local/bin`, poi `~/.cargo/bin`, su macOS poi Homebrew).
pub fn locate_uv(search: &SearchEnv) -> Option<PathBuf> {
    locate("uv", search).or_else(|| {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(home) = &search.home {
            dirs.push(home.join(".cargo").join("bin"));
        }
        if cfg!(target_os = "macos") {
            dirs.extend(HOMEBREW_BIN_DIRS.iter().map(PathBuf::from));
        }
        find_in(dirs, &exe_name("uv"))
    })
}

/// Cartelle dei binari di Homebrew (Apple Silicon e Intel): un'app avviata dal
/// Finder non eredita il `PATH` della shell.
const HOMEBREW_BIN_DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

pub(crate) fn locate(base: &str, search: &SearchEnv) -> Option<PathBuf> {
    let name = exe_name(base);
    let from_path = search
        .path
        .as_ref()
        .and_then(|p| find_in(env::split_paths(p), &name));
    from_path.or_else(|| {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(x) = search.xdg_bin_home.as_ref().filter(|x| !x.is_empty()) {
            dirs.push(PathBuf::from(x));
        }
        if let Some(home) = &search.home {
            dirs.push(home.join(".local").join("bin"));
        }
        find_in(dirs, &name)
    })
}

/// Primo eseguibile `name` nelle cartelle assolute di `dirs`.
pub(crate) fn find_in(dirs: impl IntoIterator<Item = PathBuf>, name: &str) -> Option<PathBuf> {
    dirs.into_iter()
        .filter(|d| d.is_absolute())
        .map(|d| d.join(name))
        .find(|c| is_executable(c))
}
fn is_executable(path: &Path) -> bool {
    let Ok(meta) = path.metadata() else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_exe(dir: &Path, base: &str) -> PathBuf {
        let p = dir.join(exe_name(base));
        std::fs::write(&p, b"x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    }

    #[test]
    fn search_order() {
        let tmp = tempfile::tempdir().unwrap();
        let in_path = tmp.path().join("p");
        let home = tmp.path().join("h");
        let local_bin = home.join(".local").join("bin");
        std::fs::create_dir_all(&in_path).unwrap();
        std::fs::create_dir_all(&local_bin).unwrap();
        let from_home = fake_exe(&local_bin, "cswap");
        let mut env = SearchEnv {
            home: Some(home.clone()),
            path: Some(env::join_paths([PathBuf::from("rel"), in_path.clone()]).unwrap()),
            ..Default::default()
        };
        assert_eq!(locate_cswap(&env).unwrap(), from_home);
        let from_path = fake_exe(&in_path, "cswap");
        assert_eq!(locate_cswap(&env).unwrap(), from_path);
        let uv_dir = tmp.path().join("uvbin");
        std::fs::create_dir_all(&uv_dir).unwrap();
        env.uv_tool_bin_dir = Some(uv_dir.clone().into());
        assert_eq!(locate_cswap(&env).unwrap(), from_path, "cartella vuota");
        let from_uv = fake_exe(&uv_dir, "cswap");
        assert_eq!(
            locate_cswap(&env).unwrap(),
            from_uv,
            "UV_TOOL_BIN_DIR prima del PATH"
        );
        env.override_path = Some(from_home.clone().into());
        assert_eq!(locate_cswap(&env).unwrap(), from_home);
        env.override_path = Some("relative/cswap".into());
        assert_eq!(
            locate_cswap(&env).unwrap_err().kind,
            CswapErrorKind::NotInstalled
        );
        let empty = SearchEnv::default();
        assert_eq!(
            locate_cswap(&empty).unwrap_err().kind,
            CswapErrorKind::NotInstalled
        );
        if !cfg!(target_os = "macos") {
            assert!(locate_uv(&empty).is_none());
        }
        let cargo_bin = home.join(".cargo").join("bin");
        std::fs::create_dir_all(&cargo_bin).unwrap();
        let uv = fake_exe(&cargo_bin, "uv");
        assert_eq!(locate_uv(&env), Some(uv));
    }
}
