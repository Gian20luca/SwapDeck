//! Ciclo di lettura: `cswap list --json` a ogni intervallo della config e a richiesta.
//! Un solo thread fa le letture, quindi due richieste vicine non lanciano due processi.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use swapdeck_core::cswap::{CswapError, CswapErrorKind, ListResponse};

use crate::api::Res;

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
    /// `on_result` riceve ogni esito, nel thread del ciclo. Solo lettura, senza motore (test).
    #[cfg(test)]
    pub fn spawn(
        api: std::sync::Arc<dyn crate::api::CswapApi>,
        interval: Duration,
        on_result: impl Fn(&Res<ListResponse>) + Send + 'static,
    ) -> (Self, JoinHandle<()>) {
        Self::spawn_with(interval, on_result, move |lp| lp.run(|| api.list()))
    }

    /// Come [`Poller::spawn`], con la lettura preparata da `body` nel thread del ciclo
    /// (il motore vive nel thread e prende in prestito le sue dipendenze).
    pub fn spawn_with(
        interval: Duration,
        on_result: impl Fn(&Res<ListResponse>) + Send + 'static,
        body: impl FnOnce(Loop) + Send + 'static,
    ) -> (Self, JoinHandle<()>) {
        let (tx, rx) = mpsc::channel();
        let lp = Loop {
            interval,
            rx,
            on_result: Box::new(on_result),
        };
        let handle = std::thread::Builder::new()
            .name("swapdeck-poller".into())
            .spawn(move || body(lp))
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

type OnResult = Box<dyn Fn(&Res<ListResponse>) + Send>;

/// Ciclo pronto a partire: [`Loop::run`] gira finché il [`Poller`] non viene fermato.
pub struct Loop {
    interval: Duration,
    rx: Receiver<Msg>,
    on_result: OnResult,
}

/// Una lettura: esito e, se serve, attesa diversa dall'intervallo prima della prossima.
pub type Reading = (Res<ListResponse>, Option<Duration>);

impl Loop {
    #[cfg(test)]
    pub fn run(self, mut read: impl FnMut() -> Res<ListResponse>) {
        self.run_paced(move || (read(), None));
    }

    /// Come [`Loop::run`], con la lettura che può anticipare la successiva.
    pub fn run_paced(self, mut read: impl FnMut() -> Reading) {
        run(&mut read, self.interval, &self.rx, &*self.on_result);
    }
}

fn run(
    read: &mut dyn FnMut() -> Reading,
    interval: Duration,
    rx: &Receiver<Msg>,
    on_result: &dyn Fn(&Res<ListResponse>),
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
        let (result, next_wait) = read();
        on_result(&result);
        for reply in waiting.drain(..) {
            let _ = reply.send(result.clone());
        }
        next = Instant::now() + next_wait.unwrap_or(interval);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::mock::MockApi;
    use std::sync::Arc;
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

    #[test]
    fn paced_reading_brings_the_next_one_forward() {
        let reads = Arc::new(Mutex::new(0usize));
        let counter = Arc::clone(&reads);
        let (poller, handle) = Poller::spawn_with(
            Duration::from_secs(3600),
            |_| {},
            move |lp| {
                lp.run_paced(|| {
                    let mut n = counter.lock().unwrap();
                    *n += 1;
                    let next = (*n == 1).then_some(Duration::ZERO);
                    (Ok(crate::api::mock::fixture_list()), next)
                });
            },
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while *reads.lock().unwrap() < 2 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_millis(100));
        poller.stop();
        handle.join().unwrap();
        assert_eq!(
            *reads.lock().unwrap(),
            2,
            "seconda lettura subito, poi l'intervallo"
        );
    }
}
