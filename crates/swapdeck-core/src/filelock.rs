//! Lock esclusivo fra processi su un file accanto a quello da proteggere.
//!
//! Lock del sistema operativo (`fs4`: `flock` su Unix, `LockFileEx` su Windows; stessa API di
//! `File::try_lock` della libreria standard, che richiederebbe Rust 1.89): si libera da solo se
//! il processo termina, quindi nessun lock orfano. Il file di lock resta sul disco (vuoto) e non
//! si cancella mai: cancellarlo aprirebbe una corsa fra chi lo ha appena aperto e chi lo ricrea.

use std::fs::{File, OpenOptions};

use fs4::{FileExt, TryLockError};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// Lock tenuto finché il valore vive.
#[derive(Debug)]
pub struct FileLock {
    file: File,
}

/// Esito di `acquire`.
#[derive(Debug)]
pub enum LockError {
    /// Lock tenuto da un altro (processo o handle) oltre il tempo concesso.
    Timeout,
    Io(io::Error),
}

impl From<io::Error> for LockError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// Apre (o crea, 0600) `path` senza seguire un link finale e prende il lock esclusivo,
/// ritentando ogni pochi millisecondi fino a `timeout`.
pub fn acquire(path: &Path, timeout: Duration) -> Result<FileLock, LockError> {
    let file = open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "il lock non è un file regolare",
        )
        .into());
    }
    let start = Instant::now();
    let mut pause = Duration::from_millis(5);
    loop {
        // Chiamate esplicite al trait: i metodi omonimi di `File` esistono solo da Rust 1.89.
        match FileExt::try_lock(&file) {
            Ok(()) => return Ok(FileLock { file }),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
        if start.elapsed() >= timeout {
            return Err(LockError::Timeout);
        }
        std::thread::sleep(pause.min(timeout.saturating_sub(start.elapsed())));
        pause = (pause * 2).min(Duration::from_millis(50));
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // Si libera comunque alla chiusura dello handle; qui subito, senza aspettare.
        let _ = FileExt::unlock(&self.file);
    }
}

/// `FILE_FLAG_OPEN_REPARSE_POINT` di Win32.
#[cfg(windows)]
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

fn open(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use rustix::fs::OFlags;
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
        // Niente link finale; NONBLOCK evita di restare appesi su una FIFO messa al suo posto.
        opts.custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK).bits() as i32);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Un reparse point (link) si apre come tale, senza seguirlo.
        opts.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    opts.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_holder_waits_and_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".x.lock");
        let held = acquire(&path, Duration::from_millis(100)).unwrap();
        let start = Instant::now();
        assert!(matches!(
            acquire(&path, Duration::from_millis(150)),
            Err(LockError::Timeout)
        ));
        assert!(start.elapsed() >= Duration::from_millis(150));
        drop(held);
        let again = acquire(&path, Duration::from_millis(100)).unwrap();
        drop(again);
        assert!(path.is_file(), "il file di lock resta");
    }

    #[test]
    fn released_lock_is_taken_by_waiter() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".x.lock");
        let held = acquire(&path, Duration::from_millis(100)).unwrap();
        let p = path.clone();
        let waiter = std::thread::spawn(move || acquire(&p, Duration::from_secs(5)).is_ok());
        std::thread::sleep(Duration::from_millis(80));
        drop(held);
        assert!(waiter.join().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"").unwrap();
        let path = dir.path().join(".x.lock");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(matches!(
            acquire(&path, Duration::from_millis(50)),
            Err(LockError::Io(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn lock_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".x.lock");
        drop(acquire(&path, Duration::from_millis(50)).unwrap());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0);
    }
}
