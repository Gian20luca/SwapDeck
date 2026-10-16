//! Client HTTP minimo (ureq + rustls): solo HTTPS (anche nei redirect), proxy dalle
//! variabili d'ambiente, certificati della piattaforma, timeout, risposta limitata e
//! ritentativi con attesa crescente.

use std::thread;
use std::time::Duration;

use ureq::Agent;
use ureq::tls::{RootCerts, TlsConfig, TlsProvider};

use crate::cswap::error::{CswapError, CswapErrorKind};

/// Tentativi totali per le richieste ritentabili (rete, 429, 5xx).
const ATTEMPTS: u32 = 3;
const FIRST_BACKOFF: Duration = Duration::from_millis(500);

pub(crate) fn agent(timeout: Duration) -> Agent {
    let tls = TlsConfig::builder()
        .provider(TlsProvider::Rustls)
        .root_certs(RootCerts::PlatformVerifier)
        .build();
    Agent::config_builder()
        .tls_config(tls)
        .timeout_global(Some(timeout))
        .http_status_as_error(false)
        .https_only(true)
        .user_agent(format!("SwapDeck/{}", crate::version()))
        .build()
        .new_agent()
}

/// `GET url` come testo, al massimo `limit` byte.
pub(crate) fn get_text(agent: &Agent, url: &str, limit: u64) -> Result<String, CswapError> {
    let mut wait = FIRST_BACKOFF;
    let mut attempt = 1;
    loop {
        match get_once(agent, url, limit) {
            Ok(text) => return Ok(text),
            Err((_, true)) if attempt < ATTEMPTS => {
                thread::sleep(wait);
                wait *= 2;
                attempt += 1;
            }
            Err((err, _)) => return Err(err),
        }
    }
}

/// Un tentativo; l'errore dice se ha senso ritentare.
fn get_once(agent: &Agent, url: &str, limit: u64) -> Result<String, (CswapError, bool)> {
    let mut resp = agent.get(url).call().map_err(|e| {
        let kind = if matches!(e, ureq::Error::Timeout(_)) {
            CswapErrorKind::Timeout
        } else {
            CswapErrorKind::Failed
        };
        (
            CswapError::new(kind, format!("richiesta a {} non riuscita: {e}", host(url))),
            true,
        )
    })?;
    let status = resp.status().as_u16();
    if status != 200 {
        let retry = status == 429 || status >= 500;
        return Err((
            CswapError::new(
                CswapErrorKind::Failed,
                format!("{} ha risposto {status}", host(url)),
            ),
            retry,
        ));
    }
    resp.body_mut()
        .with_config()
        .limit(limit)
        .read_to_string()
        .map_err(|e| {
            (
                CswapError::new(
                    CswapErrorKind::Failed,
                    format!("risposta di {} non leggibile: {e}", host(url)),
                ),
                true,
            )
        })
}

fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    rest.split(['/', '?']).next().unwrap_or(rest)
}

#[cfg(test)]
mod tests {
    #[test]
    fn host_of_url() {
        assert_eq!(super::host("https://pypi.org/pypi/x/json"), "pypi.org");
        assert_eq!(super::host("astral.sh"), "astral.sh");
    }
}
