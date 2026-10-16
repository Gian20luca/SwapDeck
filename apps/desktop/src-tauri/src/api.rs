//! Interfaccia sottile verso cswap: l'app parla solo con [`CswapApi`], così i comandi, il ciclo di
//! lettura e i test non dipendono da come il client esegue il processo. L'implementazione reale è
//! [`ClientApi`], sopra `CswapClient` di swapdeck-core.

use std::sync::Mutex;

pub use swapdeck_core::cswap::{ActionOutcome as Outcome, CommandSpec};
use swapdeck_core::cswap::{
    AutoProcess, ConfigGetResponse, ConfigListResponse, CswapAction, CswapClient, CswapError,
    ListResponse, StatusResponse,
};

use crate::state::lock;

pub type Res<T> = Result<T, CswapError>;

/// Operazioni di cswap usate dall'app. Le implementazioni serializzano da sole le chiamate se serve.
pub trait CswapApi: Send + Sync {
    fn list(&self) -> Res<ListResponse>;
    fn status(&self) -> Res<StatusResponse>;
    fn config_list(&self) -> Res<ConfigListResponse>;
    fn config_get(&self, key: &str) -> Res<ConfigGetResponse>;
    /// Esegue un'azione di F0 (la conferma si chiede prima, con `CswapAction::confirmation`).
    fn execute(&self, action: &CswapAction) -> Res<Outcome>;
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
    auto: Mutex<Option<AutoProcess>>,
}

impl ClientApi {
    pub fn new() -> Self {
        Self {
            client: Mutex::new(None),
            gate: Mutex::new(()),
            auto: Mutex::new(None),
        }
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
    fn execute(&self, action: &CswapAction) -> Res<Outcome> {
        match action {
            CswapAction::AutoStart { options } => {
                let process = self.with(|c| c.start_auto(options))?;
                // Un solo `cswap auto` alla volta: il precedente si ferma (anche al drop).
                if let Some(mut old) = lock(&self.auto).replace(process) {
                    old.stop();
                }
                Ok(text_outcome("cswap auto avviato"))
            }
            CswapAction::AutoStop => {
                if let Some(mut p) = lock(&self.auto).take() {
                    p.stop();
                }
                Ok(text_outcome("cswap auto fermato"))
            }
            _ => self.with(|c| c.execute(action)),
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
    }

    impl MockApi {
        pub fn new() -> Self {
            Self {
                list: Mutex::new(Ok(fixture_list())),
                list_calls: AtomicUsize::new(0),
                actions: Mutex::new(Vec::new()),
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
}
