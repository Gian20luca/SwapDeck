//! Ciclo di lettura: `cswap list --json` a ogni intervallo della config e a richiesta.
//! Un solo thread fa le letture, quindi due richieste vicine non lanciano due processi.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use swapdeck_core::cswap::{CswapError, CswapErrorKind, ListResponse};

use crate::api::{CswapApi, Res};

/// Attesa massima di una lettura a richiesta (il client ha già il suo timeout di 30 s).
pub const REFRESH_WAIT: Duration = Duration::from_secs(45);

enum Msg {
    Refresh(Option<Sender<Res<ListResponse>>>),
    Stop,
}

/// Maniglia del ciclo: si clona liberamente.
#[derive(Clone)]
pub struct Poller {
    tx: Sender<Msg>,
}

impl Poller {
    /// Avvia il ciclo: la prima lettura parte subito, poi una ogni `interval`.
    /// `on_result` riceve ogni esito, nel thread del ciclo.
    pub fn spawn(
        api: Arc<dyn CswapApi>,
        interval: Duration,
        on_result: impl Fn(&Res<ListResponse>) + Send + 'static,
    ) -> (Self, JoinHandle<()>) {
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name("swapdeck-poller".into())
            .spawn(move || run(api.as_ref(), interval, &rx, on_result))
            .expect("thread del ciclo di lettura");
        (Self { tx }, handle)
    }

    /// Chiede una lettura subito, senza aspettarla.
    pub fn refresh(&self) {
        let _ = self.tx.send(Msg::Refresh(None));
    }

    /// Chiede una lettura subito e ne aspetta l'esito.
    pub fn refresh_wait(&self, timeout: Duration) -> Res<ListResponse> {
        let (reply, rx) = mpsc::channel();
        self.tx
            .send(Msg::Refresh(Some(reply)))
            .map_err(|_| stopped())?;
        rx.recv_timeout(timeout).map_err(|e| match e {
            RecvTimeoutError::Timeout => {
                CswapError::new(CswapErrorKind::Timeout, "lettura di cswap in ritardo")
            }
            RecvTimeoutError::Disconnected => stopped(),
        })?
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Msg::Stop);
    }
}

fn stopped() -> CswapError {
    CswapError::new(CswapErrorKind::Cancelled, "ciclo di lettura fermato")
}

fn run(
    api: &dyn CswapApi,
    interval: Duration,
    rx: &Receiver<Msg>,
    on_result: impl Fn(&Res<ListResponse>),
) {
    let mut waiting: Vec<Sender<Res<ListResponse>>> = Vec::new();
    let mut next = Instant::now();
    loop {
        let wait = next.saturating_duration_since(Instant::now());
        match rx.recv_timeout(wait) {
            Ok(Msg::Refresh(reply)) => waiting.extend(reply),
            Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => {}
        }
        // Unisce le richieste già in coda alla stessa lettura.
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Refresh(reply) => waiting.extend(reply),
                Msg::Stop => return,
            }
        }
        let result = api.list();
        on_result(&result);
        for reply in waiting.drain(..) {
            let _ = reply.send(result.clone());
        }
        next = Instant::now() + interval;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::MockApi;
    use std::sync::Mutex;

    #[test]
    fn polls_at_start_on_interval_and_on_demand() {
        let api = Arc::new(MockApi::new());
        let seen = Arc::new(Mutex::new(0usize));
        let seen2 = Arc::clone(&seen);
        let (poller, handle) = Poller::spawn(
            api.clone(),
            Duration::from_millis(150),
            move |r: &Res<ListResponse>| {
                assert!(r.is_ok());
                *seen2.lock().unwrap() += 1;
            },
        );
        let list = poller.refresh_wait(REFRESH_WAIT).unwrap();
        assert_eq!(list.active_account_number, Some(6));
        // Dopo la lettura a richiesta ne arriva una periodica senza chiedere nulla.
        let after_refresh = api.calls();
        let deadline = Instant::now() + Duration::from_secs(10);
        while api.calls() <= after_refresh && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(api.calls() > after_refresh, "nessuna lettura periodica");
        poller.stop();
        handle.join().unwrap();
        assert_eq!(*seen.lock().unwrap(), api.calls());
    }

    #[test]
    fn errors_reach_callback_and_waiters() {
        let api = Arc::new(MockApi::new());
        *api.list.lock().unwrap() = Err(CswapError::new(CswapErrorKind::RateLimited, "429"));
        let (poller, handle) = Poller::spawn(
            api.clone(),
            Duration::from_secs(3600),
            |r: &Res<ListResponse>| {
                assert_eq!(r.as_ref().unwrap_err().kind, CswapErrorKind::RateLimited);
            },
        );
        let err = poller.refresh_wait(REFRESH_WAIT).unwrap_err();
        assert_eq!(err.kind, CswapErrorKind::RateLimited);
        poller.stop();
        handle.join().unwrap();
        assert!(
            api.calls() <= 2,
            "nessuna lettura periodica con intervallo di un'ora"
        );
        assert_eq!(
            poller.refresh_wait(REFRESH_WAIT).unwrap_err().kind,
            CswapErrorKind::Cancelled
        );
    }
}
