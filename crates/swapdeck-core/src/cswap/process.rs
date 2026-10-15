//! Esecuzione di cswap (e di uv) come processo figlio: pipe su stdin, stdout e
//! stderr, timeout con chiusura dell'albero dei processi, niente console su Windows.

use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Component, Path};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use zeroize::Zeroizing;

use super::error::{CswapError, CswapErrorKind};

/// Limite di byte letti da stdout e da stderr (il resto si scarta).
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
/// Attesa dei lettori dopo l'uscita del processo.
const READER_GRACE: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Uscita di un processo terminato.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawOutput {
    /// `None` se terminato da un segnale.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl RawOutput {
    /// `true` con uscita 0.
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// Prepara il comando con l'ambiente e le opzioni comuni.
pub(crate) fn command(program: &Path, args: &[OsString]) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NO_COLOR", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env_remove("CLAUDE_SECURESTORAGE_CONFIG_DIR");
    if std::env::var_os("CLAUDE_CONFIG_DIR").is_some_and(|v| is_session_profile(&v)) {
        cmd.env_remove("CLAUDE_CONFIG_DIR");
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd
}

/// `true` se il percorso sta in un profilo di sessione di `cswap run`
/// (`<backup>/sessions/...`): con quello ogni comando che modifica fallisce.
fn is_session_profile(dir: &OsStr) -> bool {
    let parts: Vec<String> = Path::new(dir)
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => Some(p.to_string_lossy().to_lowercase()),
            _ => None,
        })
        .collect();
    parts
        .windows(2)
        .any(|w| (w[0] == ".claude-swap-backup" || w[0] == "claude-swap") && w[1] == "sessions")
}

/// Avvia il processo; `NotFound` diventa `NotInstalled`.
pub(crate) fn spawn(mut cmd: Command, program: &Path) -> Result<Child, CswapError> {
    cmd.spawn().map_err(|e| {
        let kind = if e.kind() == std::io::ErrorKind::NotFound {
            CswapErrorKind::NotInstalled
        } else {
            CswapErrorKind::Failed
        };
        CswapError::new(
            kind,
            format!("avvio di {} non riuscito: {e}", file_name(program)),
        )
    })
}

fn file_name(program: &Path) -> String {
    program
        .file_name()
        .map_or_else(|| "cswap".into(), |n| n.to_string_lossy().into_owned())
}

/// Esegue `program args`, scrive `input` su stdin e lo chiude, attende al massimo `timeout`.
pub fn run(
    program: &Path,
    args: &[OsString],
    input: Zeroizing<Vec<u8>>,
    timeout: Duration,
) -> Result<RawOutput, CswapError> {
    let mut child = spawn(command(program, args), program)?;
    let stdin = child.stdin.take();
    let writer = thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            // BrokenPipe: il processo è uscito senza leggere; l'esito lo dice il codice.
            let _ = stdin.write_all(&input);
            let _ = stdin.flush();
        }
        drop(input);
    });
    let out_rx = reader(child.stdout.take());
    let err_rx = reader(child.stderr.take());

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                kill_tree(&mut child);
                let _ = writer.join();
                return Err(CswapError::new(
                    CswapErrorKind::Timeout,
                    format!(
                        "{} non ha risposto entro {} s",
                        file_name(program),
                        timeout.as_secs()
                    ),
                ));
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(e) => {
                kill_tree(&mut child);
                return Err(CswapError::new(
                    CswapErrorKind::Failed,
                    format!("attesa del processo non riuscita: {e}"),
                ));
            }
        }
    };
    let _ = writer.join();
    let stdout = out_rx.recv_timeout(READER_GRACE).unwrap_or_default();
    let stderr = err_rx.recv_timeout(READER_GRACE).unwrap_or_default();
    Ok(RawOutput {
        code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

/// Legge tutto da una pipe in un thread, fino a `MAX_OUTPUT_BYTES`.
fn reader<R: Read + Send + 'static>(pipe: Option<R>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut pipe) = pipe {
            let mut chunk = [0u8; 8192];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let room = MAX_OUTPUT_BYTES.saturating_sub(buf.len());
                        buf.extend_from_slice(&chunk[..n.min(room)]);
                    }
                }
            }
        }
        let _ = tx.send(buf);
    });
    rx
}

/// Chiude il processo e i suoi discendenti (trampolino uv e `python.exe` su Windows).
pub(crate) fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Percorso assoluto: niente taskkill preso dalla cartella corrente o dal PATH.
        let taskkill = std::env::var_os("SystemRoot")
            .map(|r| {
                std::path::PathBuf::from(r)
                    .join("System32")
                    .join("taskkill.exe")
            })
            .unwrap_or_else(|| "taskkill.exe".into());
        let _ = Command::new(taskkill)
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    #[cfg(unix)]
    {
        let pid = rustix::process::Pid::from_child(child);
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Termina con garbo (SIGTERM al gruppo su Unix), poi forza dopo `grace`.
pub(crate) fn terminate_tree(child: &mut Child, grace: Duration) {
    #[cfg(unix)]
    {
        let pid = rustix::process::Pid::from_child(child);
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::TERM);
        let deadline = Instant::now() + grace;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
    #[cfg(not(unix))]
    let _ = grace;
    kill_tree(child);
}

/// Righe di stdout lette in un thread e passate a `on_line` finché la pipe è aperta.
pub(crate) fn line_reader<F>(stdout: ChildStdout, mut on_line: F) -> thread::JoinHandle<()>
where
    F: FnMut(String) + Send + 'static,
{
    thread::spawn(move || {
        use std::io::BufRead;
        let mut reader = std::io::BufReader::new(stdout);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader
                .by_ref()
                .take(MAX_OUTPUT_BYTES as u64)
                .read_until(b'\n', &mut buf)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => on_line(String::from_utf8_lossy(&buf).trim().to_owned()),
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_profiles_are_detected() {
        assert!(is_session_profile(OsStr::new(
            "/home/u/.local/share/claude-swap/sessions/3"
        )));
        #[cfg(windows)]
        assert!(is_session_profile(OsStr::new(
            r"C:\Users\u\.claude-swap-backup\sessions\abc"
        )));
        assert!(!is_session_profile(OsStr::new("/home/u/.claude")));
    }
}
