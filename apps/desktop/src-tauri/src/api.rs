//! Interfaccia sottile verso cswap: l'app parla solo con [`CswapApi`], così i comandi, il ciclo di
//! lettura e i test non dipendono da come il client esegue il processo. L'implementazione reale è
//! [`ClientApi`], sopra `CswapClient` di swapdeck-core.

use std::sync::Mutex;
use std::time::Duration;

pub use swapdeck_core::cswap::{ActionOutcome as Outcome, CommandSpec};
use swapdeck_core::cswap::{
    AutoEvent, AutoMessage, AutoProcess, ConfigGetResponse, ConfigListResponse, CswapAction,
    CswapClient, CswapError, CswapErrorKind, DirectoryMapping, ListResponse, StatusResponse,
};

use crate::state::lock;

pub type Res<T> = Result<T, CswapError>;

/// Operazioni di cswap usate dall'app. Le implementazioni serializzano da sole le chiamate se serve.
pub trait CswapApi: Send + Sync {
    fn list(&self) -> Res<ListResponse>;
    fn status(&self) -> Res<StatusResponse>;
    fn config_list(&self) -> Res<ConfigListResponse>;
    fn config_get(&self, key: &str) -> Res<ConfigGetResponse>;
    /// Cartelle mappate (`cswap map`, testo interpretato dal core).
    fn mappings(&self) -> Res<Vec<DirectoryMapping>> {
        Err(CswapError::new(CswapErrorKind::Failed, "cartelle mappate non disponibili"))
    }
    /// Esegue un'azione di F0 (la conferma si chiede prima, con `CswapAction::confirmation`).
    fn execute(&self, action: &CswapAction) -> Res<Outcome>;
    /// Stato di `cswap auto` avviato dall'app; svuota gli eventi arrivati nel frattempo.
    fn auto_poll(&self) -> AutoPoll {
        AutoPoll::NotRunning
    }
    /// All'uscita dell'app: ferma `cswap auto` con un'attesa breve.
    fn shutdown(&self) {}
}

/// Attesa della pausa tra due giri di `cswap auto` quando l'app si chiude.
pub const SHUTDOWN_AUTO_WAIT: Duration = Duration::from_secs(2);

/// Esito di [`CswapApi::auto_poll`].
#[derive(Debug, Clone, PartialEq)]
pub enum AutoPoll {
    NotRunning,
    /// In esecuzione; `switched` se dall'ultimo controllo ha cambiato account.
    Running {
        switched: bool,
    },
    /// Terminato da solo: `error` se l'uscita non è riuscita o cswap ha mandato una busta d'errore.
    Ended {
        code: Option<i32>,
        error: Option<CswapError>,
    },
}

/// `cswap auto` avviato dall'app, con l'ultima busta d'errore ricevuta.
struct RunningAuto {
    process: AutoProcess,
    error: Option<CswapError>,
    /// Messo nel Job Object dell'app (solo Windows).
    #[cfg_attr(not(test), allow(dead_code))]
    in_job: bool,
}

impl RunningAuto {
    fn poll(&mut self) -> AutoPoll {
        let mut switched = false;
        while let Ok(msg) = self.process.events().try_recv() {
            match msg {
                AutoMessage::Event(AutoEvent::Switch { .. }) => switched = true,
                AutoMessage::Error(e) => self.error = Some(e),
                AutoMessage::Event(_) => {}
            }
        }
        match self.process.try_exit() {
            None => AutoPoll::Running { switched },
            Some(code) => {
                let exit_error = self.process.exit_error(code);
                AutoPoll::Ended {
                    code,
                    error: self.error.take().or(exit_error),
                }
            }
        }
    }
}

/// Job Object con `KILL_ON_JOB_CLOSE`: se l'app termina senza passare da
/// [`CswapApi::shutdown`] (arresto forzato, crash) Windows chiude anche `cswap auto`.
#[cfg(windows)]
fn kill_on_close_job() -> Option<win32job::Job> {
    let mut info = win32job::ExtendedLimitInfo::new();
    info.limit_kill_on_job_close();
    match win32job::Job::create_with_limit_info(&info) {
        Ok(job) => Some(job),
        Err(e) => {
            eprintln!("Job Object non creato: {e}");
            None
        }
    }
}

fn text_outcome(message: &str) -> Outcome {
    Outcome {
        message: message.to_owned(),
        list: None,
        switch: None,
        command: None,
        warning: None,
    }
}

/// Client reale. Cerca cswap alla prima chiamata e di nuovo finché non lo trova (può essere
/// installato mentre l'app è aperta); una chiamata alla volta, così il lock di cswap non si contende.
pub struct ClientApi {
    client: Mutex<Option<CswapClient>>,
    gate: Mutex<()>,
    auto: Mutex<Option<RunningAuto>>,
    #[cfg(windows)]
    job: Option<win32job::Job>,
}

impl ClientApi {
    pub fn new() -> Self {
        Self {
            client: Mutex::new(None),
            gate: Mutex::new(()),
            auto: Mutex::new(None),
            #[cfg(windows)]
            job: kill_on_close_job(),
        }
    }

    #[cfg(windows)]
    fn adopt(&self, process: &AutoProcess) -> bool {
        let (Some(job), Some(handle)) = (&self.job, process.raw_handle()) else {
            return false;
        };
        match job.assign_process(handle as isize) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("cswap auto non messo nel Job Object: {e}");
                false
            }
        }
    }

    #[cfg(not(windows))]
    fn adopt(&self, _process: &AutoProcess) -> bool {
        false
    }

    fn with<T>(&self, f: impl FnOnce(&CswapClient) -> Res<T>) -> Res<T> {
        let client = {
            let mut slot = lock(&self.client);
            match slot.as_ref() {
                Some(c) => c.clone(),
                None => {
                    let c = CswapClient::locate()?;
                    c.check_version()?;
                    slot.insert(c).clone()
                }
            }
        };
        let _turn = lock(&self.gate);
        f(&client)
    }
}

impl CswapApi for ClientApi {
    fn list(&self) -> Res<ListResponse> {
        self.with(CswapClient::list)
    }
    fn status(&self) -> Res<StatusResponse> {
        self.with(CswapClient::status)
    }
    fn config_list(&self) -> Res<ConfigListResponse> {
        self.with(CswapClient::config_list)
    }
    fn config_get(&self, key: &str) -> Res<ConfigGetResponse> {
        self.with(|c| c.config_get(key))
    }
    fn mappings(&self) -> Res<Vec<DirectoryMapping>> {
        self.with(CswapClient::mappings)
    }
    fn execute(&self, action: &CswapAction) -> Res<Outcome> {
        match action {
            CswapAction::AutoStart { options } => {
                let process = self.with(|c| c.start_auto(options))?;
                let running = RunningAuto {
                    in_job: self.adopt(&process),
                    process,
                    error: None,
                };
                // Un solo `cswap auto` alla volta: il precedente si ferma, fuori dal lock.
                let old = lock(&self.auto).replace(running);
                if let Some(mut old) = old {
                    old.process.stop();
                }
                Ok(text_outcome("cswap auto avviato"))
            }
            CswapAction::AutoStop => {
                let old = lock(&self.auto).take();
                if let Some(mut old) = old {
                    old.process.stop();
                }
                Ok(text_outcome("cswap auto fermato"))
            }
            _ => self.with(|c| c.execute(action)),
        }
    }

    fn auto_poll(&self) -> AutoPoll {
        let mut slot = lock(&self.auto);
        let Some(running) = slot.as_mut() else {
            return AutoPoll::NotRunning;
        };
        let poll = running.poll();
        if matches!(poll, AutoPoll::Ended { .. }) {
            *slot = None;
        }
        poll
    }

    fn shutdown(&self) {
        let old = lock(&self.auto).take();
        if let Some(mut old) = old {
            old.process.stop_within(SHUTDOWN_AUTO_WAIT);
        }
    }
}

#[cfg(test)]
pub mod mock {
    //! Finto [`CswapApi`] per i test: risposte dalle fixture, conteggio delle chiamate.

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use swapdeck_core::cswap::CswapErrorKind;

    pub fn fixture_list() -> ListResponse {
        let text = include_str!("../../../../fixtures/cswap/list.json");
        serde_json::from_str(text).expect("fixture list.json")
    }

    fn missing<T>() -> Res<T> {
        Err(CswapError::new(CswapErrorKind::NotInstalled, "finto"))
    }

    pub struct MockApi {
        pub list: Mutex<Res<ListResponse>>,
        pub list_calls: AtomicUsize,
        pub actions: Mutex<Vec<String>>,
        /// Esiti di `auto_poll` in ordine; finiti questi, `NotRunning`.
        pub auto_polls: Mutex<std::collections::VecDeque<AutoPoll>>,
    }

    impl MockApi {
        pub fn new() -> Self {
            Self {
                list: Mutex::new(Ok(fixture_list())),
                list_calls: AtomicUsize::new(0),
                actions: Mutex::new(Vec::new()),
                auto_polls: Mutex::new(std::collections::VecDeque::new()),
            }
        }
        pub fn calls(&self) -> usize {
            self.list_calls.load(Ordering::SeqCst)
        }
    }

    impl CswapApi for MockApi {
        fn list(&self) -> Res<ListResponse> {
            self.list_calls.fetch_add(1, Ordering::SeqCst);
            self.list.lock().unwrap().clone()
        }
        fn status(&self) -> Res<StatusResponse> {
            missing()
        }
        fn config_list(&self) -> Res<ConfigListResponse> {
            missing()
        }
        fn config_get(&self, _key: &str) -> Res<ConfigGetResponse> {
            missing()
        }
        fn execute(&self, action: &CswapAction) -> Res<Outcome> {
            self.actions.lock().unwrap().push(format!("{action:?}"));
            Ok(Outcome {
                list: Some(fixture_list()),
                ..text_outcome("ok")
            })
        }
        fn auto_poll(&self) -> AutoPoll {
            self.auto_polls
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(AutoPoll::NotRunning)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swapdeck_core::cswap::CswapErrorKind;

    /// Finto cswap del core (bin `fake-cswap`), se è già stato compilato nella target.
    fn fake_cswap() -> Option<std::path::PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?.parent()?;
        let name = format!("fake-cswap{}", std::env::consts::EXE_SUFFIX);
        let path = dir.join(name);
        path.is_file().then_some(path)
    }

    #[test]
    fn client_api_reads_and_switches_with_fake_cswap() {
        let Some(fake) = fake_cswap() else {
            eprintln!("fake-cswap non compilato: cargo build -p swapdeck-core --bin fake-cswap");
            return;
        };
        // Copia in una cartella temporanea: il finto salva lo stato accanto all'eseguibile.
        let dir = tempfile::tempdir().unwrap();
        let copy = dir.path().join(fake.file_name().unwrap());
        std::fs::copy(&fake, &copy).unwrap();
        let api = ClientApi::new();
        *lock(&api.client) = Some(CswapClient::new(copy));
        let list = api.list().expect("list dal finto cswap");
        assert!(!list.accounts.is_empty());
        let target = list
            .accounts
            .iter()
            .find(|a| !a.active && a.disabled != Some(true))
            .map(|a| a.number);
        if let Some(n) = target {
            let action: CswapAction =
                serde_json::from_str(&format!(r#"{{"kind":"switch","target":{n}}}"#)).unwrap();
            match api.execute(&action) {
                Ok(o) => assert!(o.switch.is_some()),
                Err(e) => assert_ne!(e.kind, CswapErrorKind::NotInstalled),
            }
        }
        assert!(api.execute(&CswapAction::AutoStop).is_ok());
    }

    fn api_on_fake(dir: &tempfile::TempDir) -> Option<ClientApi> {
        let fake = fake_cswap()?;
        let copy = dir.path().join(fake.file_name().unwrap());
        std::fs::copy(&fake, &copy).unwrap();
        let api = ClientApi::new();
        *lock(&api.client) = Some(CswapClient::new(copy));
        Some(api)
    }

    fn start_auto(api: &ClientApi, once: bool) {
        use swapdeck_core::cswap::AutoOptions;
        let options = AutoOptions {
            once,
            ..AutoOptions::default()
        };
        api.execute(&CswapAction::AutoStart { options })
            .expect("auto avviato sul finto");
    }

    /// Attende un esito diverso da `Running { switched: false }`.
    fn next_poll(api: &ClientApi) -> AutoPoll {
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            let p = api.auto_poll();
            if p != (AutoPoll::Running { switched: false }) || std::time::Instant::now() > deadline
            {
                return p;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn auto_end_is_detected_once() {
        let dir = tempfile::tempdir().unwrap();
        let Some(api) = api_on_fake(&dir) else { return };
        // `--once` del finto esce con 2 (nessuna azione): non è un errore.
        start_auto(&api, true);
        let mut p = next_poll(&api);
        if p == (AutoPoll::Running { switched: true }) {
            p = next_poll(&api);
        }
        assert_eq!(
            p,
            AutoPoll::Ended {
                code: Some(2),
                error: None
            }
        );
        assert_eq!(api.auto_poll(), AutoPoll::NotRunning);
    }

    #[test]
    fn shutdown_stops_auto_quickly() {
        let dir = tempfile::tempdir().unwrap();
        let Some(api) = api_on_fake(&dir) else { return };
        start_auto(&api, false);
        assert_eq!(next_poll(&api), AutoPoll::Running { switched: true });
        if cfg!(windows) {
            assert!(lock(&api.auto).as_ref().is_some_and(|r| r.in_job));
        }
        let started = std::time::Instant::now();
        api.shutdown();
        assert!(started.elapsed() < SHUTDOWN_AUTO_WAIT + Duration::from_secs(5));
        assert_eq!(api.auto_poll(), AutoPoll::NotRunning);
    }
}
