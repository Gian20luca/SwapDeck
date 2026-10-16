//! Controllo periodico di `cswap auto` avviato dall'app: svuota gli eventi, rileva i cambi di
//! account e la fine del processo (riuscita o in errore), così UI, icona e notifiche si aggiornano.

use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::api::{AutoPoll, CswapApi};

/// Intervallo del controllo: costa un lock e un `try_wait`.
pub const AUTO_WATCH_PERIOD: Duration = Duration::from_secs(2);

/// Maniglia del controllo; [`AutoWatcher::stop`] lo ferma.
pub struct AutoWatcher {
    tx: Sender<()>,
}

impl AutoWatcher {
    /// `on_poll` riceve ogni esito, nel thread del controllo.
    pub fn spawn(
        api: Arc<dyn CswapApi>,
        period: Duration,
        on_poll: impl Fn(&AutoPoll) + Send + 'static,
    ) -> (Self, JoinHandle<()>) {
        let (tx, rx) = mpsc::channel::<()>();
        let handle = std::thread::Builder::new()
            .name("swapdeck-auto-watch".into())
            .spawn(move || {
                while let Err(RecvTimeoutError::Timeout) = rx.recv_timeout(period) {
                    on_poll(&api.auto_poll());
                }
            })
            .expect("thread del controllo di cswap auto");
        (Self { tx }, handle)
    }

    pub fn stop(&self) {
        let _ = self.tx.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::MockApi;
    use std::sync::Mutex;
    use swapdeck_core::cswap::{CswapError, CswapErrorKind};

    #[test]
    fn reports_polls_until_stopped() {
        let api = Arc::new(MockApi::new());
        let ended = AutoPoll::Ended {
            code: Some(1),
            error: Some(CswapError::new(CswapErrorKind::Failed, "finto")),
        };
        api.auto_polls
            .lock()
            .unwrap()
            .extend([AutoPoll::Running { switched: true }, ended.clone()]);
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let (w, h) = AutoWatcher::spawn(api, Duration::from_millis(5), move |p| {
            sink.lock().unwrap().push(p.clone());
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while seen.lock().unwrap().len() < 3 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        w.stop();
        h.join().unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0], AutoPoll::Running { switched: true });
        assert_eq!(seen[1], ended);
        assert_eq!(seen[2], AutoPoll::NotRunning);
    }
}
