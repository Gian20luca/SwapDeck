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

#[cfg(target_os = "macos")]
pub fn open(_cmd: &CommandSpec) -> Res<()> {
    Err(failed(
        "apertura del terminale non ancora supportata su macOS",
    ))
}
