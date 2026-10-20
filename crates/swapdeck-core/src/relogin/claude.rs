//! Claude Code per «Rifai login»: ricerca dell'eseguibile e processo di
//! `claude auth login --claudeai --email <email>` con stdin, stdout e stderr in pipe.
//!
//! Il codice incollato passa solo da stdin; l'URL di accesso (parametri OAuth) non
//! finisce mai nei log né nei messaggi d'errore.

use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::cswap::locate::{exe_name, find_in, is_executable};
use crate::cswap::process;
use crate::cswap::{CSWAP_OVERRIDE_ENV, CswapError, CswapErrorKind, SecretToken};
use crate::redact::{redact, strip_control};

/// Lunghezza massima di una riga letta da Claude Code.
const MAX_LINE_BYTES: u64 = 64 * 1024;
/// Lunghezza massima dell'URL di accesso accettato.
const MAX_URL_CHARS: usize = 8192;
/// Lunghezza massima del codice incollato.
pub const MAX_CODE_CHARS: usize = 4096;
/// Righe d'errore (redatte, senza URL) tenute per il messaggio di un login non riuscito.
const ERROR_TAIL_LINES: usize = 3;

/// Cartelle di Homebrew su macOS (un'app avviata dal Finder non eredita il `PATH` della shell).
const HOMEBREW_BIN_DIRS: [&str; 2] = ["/opt/homebrew/bin", "/usr/local/bin"];

/// Variabile delle prove: percorso del finto Claude Code, letta solo con `SWAPDECK_CSWAP`.
pub const CLAUDE_TEST_ENV: &str = "SWAPDECK_CLAUDE";

/// Ambiente della ricerca (separato per i test).
#[derive(Debug, Clone, Default)]
pub struct ClaudeSearch {
    pub path: Option<OsString>,
    pub home: Option<PathBuf>,
    /// `SWAPDECK_CSWAP` impostata (prove con il finto cswap): vale solo [`CLAUDE_TEST_ENV`],
    /// mai l'impostazione né la ricerca, così una prova non avvia mai il Claude Code reale.
    pub fake_cswap: bool,
    /// Valore di [`CLAUDE_TEST_ENV`].
    pub test_claude: Option<PathBuf>,
}

impl ClaudeSearch {
    /// Ambiente del processo corrente.
    pub fn from_process() -> Self {
        Self {
            path: std::env::var_os("PATH"),
            home: directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()),
            fake_cswap: std::env::var_os(CSWAP_OVERRIDE_ENV).is_some_and(|v| !v.is_empty()),
            test_claude: std::env::var_os(CLAUDE_TEST_ENV)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from),
        }
    }
}

/// `true` se `value` è un percorso accettabile per l'impostazione `claudeCodePath`.
pub fn valid_setting(value: &str) -> bool {
    !value.trim().is_empty()
        && value.trim() == value
        && !value.chars().any(char::is_control)
        && Path::new(value).is_absolute()
}

/// Trova Claude Code: percorso dell'impostazione (se presente vale solo quello), poi `PATH`,
/// `~/.local/bin`, Homebrew su macOS, poi l'estensione di VS Code con la versione più alta.
/// Con il finto cswap solo [`CLAUDE_TEST_ENV`] (assoluto ed eseguibile), altrimenti nessuno.
pub fn locate_claude(configured: Option<&str>, search: &ClaudeSearch) -> Option<PathBuf> {
    if search.fake_cswap {
        return search
            .test_claude
            .clone()
            .filter(|p| p.is_absolute() && is_executable(p));
    }
    if let Some(value) = configured.filter(|v| !v.is_empty()) {
        let path = PathBuf::from(value);
        return (valid_setting(value) && is_executable(&path)).then_some(path);
    }
    let name = exe_name("claude");
    let from_path = search
        .path
        .as_ref()
        .and_then(|p| find_in(std::env::split_paths(p), &name));
    from_path
        .or_else(|| {
            let mut dirs = Vec::new();
            if let Some(home) = &search.home {
                dirs.push(home.join(".local").join("bin"));
            }
            if cfg!(target_os = "macos") {
                dirs.extend(HOMEBREW_BIN_DIRS.iter().map(PathBuf::from));
            }
            find_in(dirs, &name)
        })
        .or_else(|| {
            search
                .home
                .as_ref()
                .and_then(|h| from_vscode(&h.join(".vscode").join("extensions"), &name))
        })
}

/// `anthropic.claude-code-<versione>[-piattaforma]/resources/native-binary/<name>`, versione più alta.
fn from_vscode(extensions: &Path, name: &str) -> Option<PathBuf> {
    const PREFIX: &str = "anthropic.claude-code-";
    let entries = std::fs::read_dir(extensions).ok()?;
    let mut best: Option<(Vec<u64>, PathBuf)> = None;
    for entry in entries.flatten() {
        // Niente link: solo cartelle vere.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let dir_name = entry.file_name();
        let Some(version) = dir_name
            .to_str()
            .and_then(|n| n.strip_prefix(PREFIX))
            .and_then(parse_version)
        else {
            continue;
        };
        let candidate = entry
            .path()
            .join("resources")
            .join("native-binary")
            .join(name);
        if !is_executable(&candidate) {
            continue;
        }
        if best.as_ref().is_none_or(|(v, _)| version > *v) {
            best = Some((version, candidate));
        }
    }
    best.map(|(_, p)| p)
}

/// `2.1.295-win32-x64` → `[2, 1, 295]`.
fn parse_version(text: &str) -> Option<Vec<u64>> {
    let core = text.split('-').next()?;
    let parts: Option<Vec<u64>> = core.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| !p.is_empty())
}

/// `true` se l'email si può passare come argomento (`--email <email>`).
pub(crate) fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    email.len() <= 254
        && !email.starts_with('-')
        && !local.is_empty()
        && !domain.is_empty()
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// URL di accesso in una riga di Claude Code (`If the browser didn't open, visit: <url>`).
pub(crate) fn login_url(line: &str) -> Option<String> {
    let clean = strip_control(line);
    let rest = clean
        .split_once("visit:")
        .map_or(clean.as_str(), |(_, r)| r);
    rest.split_whitespace()
        .find(|w| w.starts_with("https://"))
        .filter(|u| u.len() <= MAX_URL_CHARS && u.chars().all(|c| c.is_ascii_graphic()))
        .map(str::to_owned)
}

/// Codice accettabile: non vuoto, una riga, senza spazi interni né caratteri di controllo.
pub(crate) fn valid_code(code: &SecretToken) -> bool {
    let c = code.expose();
    !c.is_empty()
        && c.chars().count() <= MAX_CODE_CHARS
        && !c.chars().any(|ch| ch.is_whitespace() || ch.is_control())
}

/// Riga d'uscita di Claude Code.
pub(crate) enum Output {
    Line(String),
}

/// Processo di login in corso; chiuso (con i discendenti) al `Drop` se ancora vivo.
pub(crate) struct LoginProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Output>,
    errors: Vec<String>,
    exited: bool,
}

impl LoginProcess {
    pub(crate) fn start(program: &Path, email: &str) -> Result<Self, CswapError> {
        if !valid_email(email) {
            return Err(CswapError::new(
                CswapErrorKind::InvalidUsage,
                "email dell'utenza non utilizzabile per il login",
            ));
        }
        let args: Vec<OsString> = ["auth", "login", "--claudeai", "--email", email]
            .iter()
            .map(OsString::from)
            .collect();
        let mut cmd = process::command(program, &args);
        // Nessuna scheda del browser sul PC: il link lo apre la persona dell'utenza (da
        // confermare nella prova reale, `docs/PROVE-REALI.md` §7bis).
        cmd.env("BROWSER", "none");
        let mut child = process::spawn(cmd, program).map_err(|e| {
            if e.kind == CswapErrorKind::NotInstalled {
                CswapError::new(CswapErrorKind::NotInstalled, "Claude Code non trovato")
            } else {
                e
            }
        })?;
        let stdin = child.stdin.take();
        let (tx, rx) = mpsc::channel();
        if let Some(out) = child.stdout.take() {
            pump(out, tx.clone());
        }
        if let Some(err) = child.stderr.take() {
            pump(err, tx);
        }
        Ok(Self {
            child,
            stdin,
            lines: rx,
            errors: Vec::new(),
            exited: false,
        })
    }

    /// Processo di Claude Code (per il Job Object dell'app su Windows).
    pub(crate) fn child(&self) -> &Child {
        &self.child
    }

    /// Righe arrivate finora (senza attendere).
    pub(crate) fn drain(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(Output::Line(l)) = self.lines.try_recv() {
            if login_url(&l).is_none() && !l.contains("http") {
                let r = redact(&l);
                if !r.is_empty() {
                    self.errors.push(r);
                    if self.errors.len() > ERROR_TAIL_LINES {
                        self.errors.remove(0);
                    }
                }
            }
            out.push(l);
        }
        out
    }

    /// Scrive il codice seguito da `\n` su stdin (che resta aperto).
    pub(crate) fn send_code(&mut self, code: &SecretToken) -> bool {
        let Some(stdin) = self.stdin.as_mut() else {
            return false;
        };
        let mut line = zeroize::Zeroizing::new(Vec::with_capacity(code.expose().len() + 1));
        line.extend_from_slice(code.expose().as_bytes());
        line.push(b'\n');
        stdin.write_all(&line).and_then(|()| stdin.flush()).is_ok()
    }

    /// `Some(codice)` quando il processo è uscito.
    pub(crate) fn try_exit(&mut self) -> Option<Option<i32>> {
        match self.child.try_wait() {
            Ok(Some(status)) => {
                self.exited = true;
                self.stdin = None;
                Some(status.code())
            }
            Ok(None) => None,
            Err(_) => {
                self.kill();
                Some(None)
            }
        }
    }

    /// Ultime righe d'errore, redatte e senza URL.
    pub(crate) fn error_tail(&self) -> Option<String> {
        (!self.errors.is_empty()).then(|| self.errors.join(" · "))
    }

    pub(crate) fn kill(&mut self) {
        self.stdin = None;
        if !self.exited {
            process::kill_tree(&mut self.child);
            self.exited = true;
        }
    }
}

impl Drop for LoginProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

fn pump<R: Read + Send + 'static>(pipe: R, tx: Sender<Output>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader
                .by_ref()
                .take(MAX_LINE_BYTES)
                .read_until(b'\n', &mut buf)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf).trim().to_owned();
                    if tx.send(Output::Line(line)).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

/// Nome per i messaggi (mai il percorso completo, che contiene la home).
pub(crate) fn display_name(program: &Path) -> String {
    program.file_name().map_or_else(
        || "claude".into(),
        |n: &OsStr| n.to_string_lossy().into_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_exe(dir: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, b"x").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    }

    #[test]
    fn url_is_extracted_from_the_visit_line() {
        let url = "https://claude.ai/oauth/authorize?code=true&client_id=x&state=y";
        assert_eq!(
            login_url(&format!("If the browser didn't open, visit: {url}")).as_deref(),
            Some(url)
        );
        assert_eq!(login_url("Opening browser to sign in…"), None);
        assert_eq!(login_url("visit: http://insecure.example"), None);
        assert_eq!(login_url("Paste code here if prompted > "), None);
    }

    #[test]
    fn email_and_code_checks() {
        assert!(valid_email("a.b@example.com"));
        assert!(!valid_email("--help@x"));
        assert!(!valid_email("a b@example.com"));
        assert!(!valid_email("nobody"));
        assert!(valid_code(&SecretToken::new("  abc#DEF_123  ")));
        assert!(!valid_code(&SecretToken::new("")));
        assert!(!valid_code(&SecretToken::new("ab cd")));
        assert!(!valid_code(&SecretToken::new(
            "a".repeat(MAX_CODE_CHARS + 1)
        )));
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(parse_version("2.1.295-win32-x64"), Some(vec![2, 1, 295]));
        assert_eq!(parse_version("x"), None);
        assert!(parse_version("2.1.10") > parse_version("2.1.9"));
    }

    #[test]
    fn search_order() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("h");
        let in_path = tmp.path().join("p");
        let name = exe_name("claude");
        let ext = home.join(".vscode").join("extensions");
        let bin = |v: &str| {
            ext.join(format!("anthropic.claude-code-{v}-x"))
                .join("resources")
                .join("native-binary")
        };
        fake_exe(&bin("2.1.9"), &name);
        let newest = fake_exe(&bin("2.1.10"), &name);
        std::fs::create_dir_all(ext.join("anthropic.claude-code-9.0.0")).unwrap();
        let mut s = ClaudeSearch {
            path: Some(std::env::join_paths([PathBuf::from("rel"), in_path.clone()]).unwrap()),
            home: Some(home.clone()),
            fake_cswap: false,
            test_claude: None,
        };
        assert_eq!(
            locate_claude(None, &s),
            Some(newest),
            "estensione, più recente"
        );
        let local = fake_exe(&home.join(".local").join("bin"), &name);
        assert_eq!(locate_claude(None, &s), Some(local.clone()));
        let from_path = fake_exe(&in_path, &name);
        assert_eq!(locate_claude(None, &s), Some(from_path));
        let explicit = local.to_string_lossy().into_owned();
        assert_eq!(locate_claude(Some(&explicit), &s), Some(local.clone()));
        assert_eq!(locate_claude(Some("relative/claude"), &s), None);
        let missing = tmp.path().join("nope").to_string_lossy().into_owned();
        assert_eq!(locate_claude(Some(&missing), &s), None, "niente ripiego");
        s.fake_cswap = true;
        assert_eq!(
            locate_claude(Some(&explicit), &s),
            None,
            "con il finto cswap né impostazione né ricerca"
        );
        s.test_claude = Some(PathBuf::from("rel").join(&name));
        assert_eq!(locate_claude(None, &s), None, "solo percorso assoluto");
        s.test_claude = Some(local.clone());
        assert_eq!(locate_claude(Some(&missing), &s), Some(local));
        assert!(valid_setting(&explicit));
        assert!(!valid_setting(" /x"));
        assert!(!valid_setting("claude"));
    }
}
