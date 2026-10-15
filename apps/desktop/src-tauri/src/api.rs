//! Interfaccia sottile verso cswap: l'app parla solo con [`CswapApi`], così i comandi, il ciclo di
//! lettura e i test non dipendono da come il client esegue il processo.

use serde::Serialize;
use swapdeck_core::cswap::{
    ConfigGetResponse, ConfigListResponse, CswapAction, CswapError, CswapErrorKind, ListResponse,
    StatusResponse, SwitchResponse,
};

/// Comando da aprire in un terminale nuovo (`cswap run`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandSpec {
    pub program: std::path::PathBuf,
    pub args: Vec<String>,
}

/// Esito di un'azione riuscita (specchio di `ActionOutcome` del client).
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub message: String,
    /// Elenco riletto dopo l'azione, se l'azione cambia gli account.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<ListResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub switch: Option<SwitchResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandSpec>,
}

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

/// Segnaposto finché il client di swapdeck-core non è disponibile: ogni chiamata fallisce.
pub struct MissingClient;

fn missing<T>() -> Res<T> {
    Err(CswapError::new(
        CswapErrorKind::NotInstalled,
        "client di cswap non disponibile in questa build",
    ))
}

impl CswapApi for MissingClient {
    fn list(&self) -> Res<ListResponse> {
        missing()
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
    fn execute(&self, _action: &CswapAction) -> Res<Outcome> {
        missing()
    }
}

#[cfg(test)]
pub mod mock {
    //! Finto [`CswapApi`] per i test: risposte dalle fixture, conteggio delle chiamate.

    use super::*;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub fn fixture_list() -> ListResponse {
        let text = include_str!("../../../../fixtures/cswap/list.json");
        serde_json::from_str(text).expect("fixture list.json")
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
                message: "ok".into(),
                list: Some(fixture_list()),
                ..Outcome::default()
            })
        }
    }
}
