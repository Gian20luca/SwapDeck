//! Processo figlio con l'output passato riga per riga (installazione e aggiornamento).

use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::cswap::error::{CswapError, CswapErrorKind, clean_message};
use crate::cswap::process::{self, RawOutput};

/// Limite dell'output conservato per stream (le righe oltre si passano ma non si tengono).
const MAX_KEPT_BYTES: usize = 1024 * 1024;
/// Limite di una singola riga.
const MAX_LINE_BYTES: usize = 8 * 1024;
const POLL: Duration = Duration::from_millis(50);
/// Attesa dei lettori dopo l'uscita del processo (un nipote può tenere aperta la pipe).
const READER_GRACE: Duration = Duration::from_secs(2);

enum Msg {
    Line(String),
    Done(bool, Vec<u8>),
}

/// Esegue `program args` con le variabili `env` in più: stdin chiuso subito, ogni riga di
/// stdout e stderr (separate da `\n` o `\r`) passa ripulita da `on_line`; allo scadere
/// di `timeout` chiude l'albero dei processi ed esce con `Timeout`.
pub(crate) fn run_streaming(
    program: &Path,
    args: &[OsString],
    env: &[(OsString, OsString)],
    timeout: Duration,
    on_line: &mut dyn FnMut(String),
) -> Result<RawOutput, CswapError> {
    let mut cmd = process::command(program, args);
    // Cartella di lavoro assoluta e neutra: niente `uv.toml` del progetto da cui parte
    // l'app e nessuna scrittura relativa nella cartella corrente (per esempio la cache dei
    // moduli di Windows PowerShell quando la cartella dati locale non esiste).
    cmd.current_dir(std::env::temp_dir());
    // Windows PowerShell 5.1 avviata da PowerShell 7 ne eredita `PSModulePath` e non carica
    // i suoi moduli (Get-ExecutionPolicy): senza la variabile ricostruisce quella predefinita.
    if program
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("powershell.exe"))
    {
        cmd.env_remove("PSModulePath");
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = process::spawn(cmd, program)?;
    drop(child.stdin.take());
    let (tx, rx) = mpsc::channel();
    spawn_reader(child.stdout.take(), false, tx.clone());
    spawn_reader(child.stderr.take(), true, tx);

    let deadline = Instant::now() + timeout;
    let mut kept = [Vec::new(), Vec::new()];
    let mut open = 2;
    let mut exited: Option<(std::process::ExitStatus, Instant)> = None;
    loop {
        match rx.recv_timeout(POLL) {
            Ok(Msg::Line(line)) => {
                let clean = clean_message(&line);
                if !clean.is_empty() {
                    on_line(clean);
                }
            }
            Ok(Msg::Done(is_err, buf)) => {
                kept[usize::from(is_err)] = buf;
                open -= 1;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => open = 0,
        }
        if exited.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => exited = Some((status, Instant::now())),
                Ok(None) if Instant::now() >= deadline => {
                    process::kill_tree(&mut child);
                    return Err(CswapError::new(
                        CswapErrorKind::Timeout,
                        format!(
                            "{} non ha finito entro {} s",
                            display_name(program),
                            timeout.as_secs()
                        ),
                    ));
                }
                Ok(None) => {}
                Err(e) => {
                    process::kill_tree(&mut child);
                    return Err(CswapError::new(
                        CswapErrorKind::Failed,
                        format!("attesa del processo non riuscita: {e}"),
                    ));
                }
            }
        }
        if let Some((status, at)) = exited {
            if open == 0 || at.elapsed() >= READER_GRACE {
                let [out, err] = kept;
                return Ok(RawOutput {
                    code: status.code(),
                    stdout: String::from_utf8_lossy(&out).into_owned(),
                    stderr: String::from_utf8_lossy(&err).into_owned(),
                });
            }
        }
    }
}

fn display_name(program: &Path) -> String {
    program
        .file_name()
        .map_or_else(|| "processo".into(), |n| n.to_string_lossy().into_owned())
}

/// Legge una pipe in un thread: manda ogni riga e, alla fine, quanto conservato.
fn spawn_reader<R: Read + Send + 'static>(pipe: Option<R>, is_err: bool, tx: mpsc::Sender<Msg>) {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut line = Vec::new();
        if let Some(mut pipe) = pipe {
            let mut chunk = [0u8; 4096];
            loop {
                let n = match pipe.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let room = MAX_KEPT_BYTES.saturating_sub(kept.len());
                kept.extend_from_slice(&chunk[..n.min(room)]);
                for &b in &chunk[..n] {
                    if b == b'\n' || b == b'\r' {
                        flush(&mut line, &tx);
                    } else if line.len() < MAX_LINE_BYTES {
                        line.push(b);
                    }
                }
            }
        }
        flush(&mut line, &tx);
        let _ = tx.send(Msg::Done(is_err, kept));
    });
}

fn flush(line: &mut Vec<u8>, tx: &mpsc::Sender<Msg>) {
    if !line.is_empty() {
        let text = String::from_utf8_lossy(line).into_owned();
        let _ = tx.send(Msg::Line(text));
        line.clear();
    }
}
