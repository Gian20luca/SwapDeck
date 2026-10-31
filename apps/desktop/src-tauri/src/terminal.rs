//! Apre `cswap run ...` in un terminale nuovo, senza passare da una shell (niente quoting da
//! interpretare: programma e argomenti vanno al sistema così come sono).

use swapdeck_core::cswap::{CswapError, CswapErrorKind};

use crate::api::{CommandSpec, Res};

fn failed(msg: impl AsRef<str>) -> CswapError {
    CswapError::new(CswapErrorKind::Failed, msg)
}

#[cfg(windows)]
pub fn open(cmd: &CommandSpec) -> Res<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    Command::new(&cmd.program)
        .args(&cmd.args)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map(drop)
        .map_err(|e| failed(format!("terminale non aperto: {e}")))
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn open(cmd: &CommandSpec) -> Res<()> {
    use std::process::{Command, Stdio};
    for term in ["x-terminal-emulator", "xterm"] {
        let spawned = Command::new(term)
            .arg("-e")
            .arg(&cmd.program)
            .args(&cmd.args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match spawned {
            Ok(mut child) => {
                // Raccoglie il processo quando termina, senza bloccare.
                std::thread::spawn(move || child.wait());
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(failed(format!("terminale non aperto: {e}"))),
        }
    }
    Err(failed(
        "nessun emulatore di terminale trovato (x-terminal-emulator, xterm)",
    ))
}

/// macOS: Terminal non riceve argomenti separati, quindi script `.command` temporaneo (0700,
/// nella cartella temporanea dell'utente) aperto con `open`, che si cancella appena parte.
#[cfg(target_os = "macos")]
pub fn open(cmd: &CommandSpec) -> Res<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::process::{Command, Stdio};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "swapdeck-run-{}-{nanos}.command",
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o700)
        .open(&path)
        .map_err(|e| failed(format!("terminale non aperto: {e}")))?;
    file.write_all(command_script(cmd).as_bytes())
        .map_err(|e| failed(format!("terminale non aperto: {e}")))?;
    drop(file);
    let status = Command::new("/usr/bin/open")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => Ok(()),
        other => {
            let _ = std::fs::remove_file(&path);
            Err(failed(match other {
                Ok(s) => format!("terminale non aperto: open è uscito con {s}"),
                Err(e) => format!("terminale non aperto: {e}"),
            }))
        }
    }
}

/// Script che si cancella e poi esegue il comando, ogni parola tra apici singoli.
#[cfg(any(target_os = "macos", test))]
fn command_script(cmd: &CommandSpec) -> String {
    let words: Vec<String> = std::iter::once(cmd.program.to_string_lossy().into_owned())
        .chain(cmd.args.iter().cloned())
        .map(|w| sh_quote(&w))
        .collect();
    format!("#!/bin/sh\nrm -f -- \"$0\"\nexec {}\n", words.join(" "))
}

/// Tra apici singoli la shell non interpreta nulla; l'apice stesso diventa `'\''`.
#[cfg(any(target_os = "macos", test))]
fn sh_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_quotes_every_word() {
        let cmd = CommandSpec {
            program: "/Users/a b/.local/bin/cswap".into(),
            args: vec!["run".into(), "--".into(), "it's $HOME `x` \"y\"".into()],
        };
        assert_eq!(
            command_script(&cmd),
            "#!/bin/sh\nrm -f -- \"$0\"\nexec '/Users/a b/.local/bin/cswap' 'run' '--' \
             'it'\\''s $HOME `x` \"y\"'\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn script_runs_with_arguments_intact_and_removes_itself() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prova.command");
        let words = ["a b", "it's", "$HOME", "`id`", "\"q\"", "*", ";", "\\", "è"];
        let cmd = CommandSpec {
            program: "/usr/bin/printf".into(),
            args: std::iter::once("%s|")
                .chain(words)
                .map(String::from)
                .collect(),
        };
        std::fs::write(&path, command_script(&cmd)).unwrap();
        let out = std::process::Command::new("/bin/sh")
            .arg(&path)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            words.join("|") + "|"
        );
        assert!(!path.exists(), "lo script si cancella");
    }
}
