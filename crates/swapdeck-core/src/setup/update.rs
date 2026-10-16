//! Ultima versione di claude-swap su PyPI, con cache di 6 ore e scelta «non ricordarmelo»
//! per una versione, nel file `cswap-update.json` della cartella dati.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cswap::error::{CswapError, CswapErrorKind, CswapVersion};

/// Pagina JSON di PyPI per claude-swap.
pub const PYPI_URL: &str = "https://pypi.org/pypi/claude-swap/json";
/// Durata della cache dell'ultima versione.
pub const CACHE_TTL: Duration = Duration::from_secs(6 * 3600);
/// Nome del file di cache nella cartella dati.
pub const CACHE_FILE: &str = "cswap-update.json";
const MAX_CACHE_BYTES: u64 = 64 * 1024;
const MAX_PYPI_BYTES: u64 = 8 * 1024 * 1024;

/// Esito del controllo della versione.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    /// Versione installata (`cswap --version`), se leggibile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub installed: Option<String>,
    /// Ultima versione stabile su PyPI.
    pub latest: String,
    /// `true` se `latest` è maggiore di `installed`.
    pub update_available: bool,
    /// L'utente ha chiuso l'avviso per `latest`.
    pub dismissed: bool,
    /// Istante del controllo su PyPI, in secondi Unix.
    #[ts(type = "number")]
    pub checked_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheFile {
    #[serde(default)]
    checked_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    latest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dismissed: Option<String>,
}

#[derive(Deserialize)]
struct PypiPage {
    info: PypiInfo,
}

#[derive(Deserialize)]
struct PypiInfo {
    version: String,
}

/// Versione `X.Y.Z` stabile (niente suffissi di pre-release).
pub fn parse_stable(version: &str) -> Option<CswapVersion> {
    let v = version.trim();
    let mut parts = v.split('.');
    let mut num = || -> Option<u32> {
        let p = parts.next()?;
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        p.parse().ok()
    };
    let parsed = CswapVersion {
        major: num()?,
        minor: num()?,
        patch: num()?,
    };
    parts.next().is_none().then_some(parsed)
}

/// Ultima versione dalla risposta JSON di PyPI.
pub fn parse_pypi(json: &str) -> Result<String, CswapError> {
    let page: PypiPage = serde_json::from_str(json).map_err(|e| {
        CswapError::new(
            CswapErrorKind::Failed,
            format!("risposta di PyPI non valida: {e}"),
        )
    })?;
    let version = page.info.version.trim().to_owned();
    if version.is_empty() || version.len() > 64 {
        return Err(CswapError::new(
            CswapErrorKind::Failed,
            "versione di PyPI non valida",
        ));
    }
    Ok(version)
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub(crate) struct UpdateStore {
    path: PathBuf,
}

impl UpdateStore {
    pub(crate) fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(CACHE_FILE),
        }
    }

    fn load(&self) -> CacheFile {
        let Ok(file) = fs::File::open(&self.path) else {
            return CacheFile::default();
        };
        let mut text = String::new();
        if file
            .take(MAX_CACHE_BYTES)
            .read_to_string(&mut text)
            .is_err()
        {
            return CacheFile::default();
        }
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Scrittura atomica: file temporaneo nella stessa cartella, poi rinomina.
    fn save(&self, cache: &CacheFile) -> Result<(), CswapError> {
        let io = |e: std::io::Error| {
            CswapError::new(
                CswapErrorKind::Failed,
                format!("scrittura di {CACHE_FILE} non riuscita: {e}"),
            )
        };
        let dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        crate::paths::create_private_dir(dir).map_err(io)?;
        let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(io)?;
        let text = serde_json::to_vec_pretty(cache).unwrap_or_default();
        tmp.write_all(&text).map_err(io)?;
        tmp.as_file().sync_all().map_err(io)?;
        tmp.persist(&self.path).map_err(|e| io(e.error))?;
        Ok(())
    }

    /// Ultima versione: dalla cache se ha meno di `CACHE_TTL` (e `force` è falso),
    /// altrimenti da `fetch`, che poi aggiorna la cache.
    pub(crate) fn latest(
        &self,
        force: bool,
        now: u64,
        fetch: impl FnOnce() -> Result<String, CswapError>,
    ) -> Result<(String, u64, Option<String>), CswapError> {
        let mut cache = self.load();
        let fresh = cache.latest.is_some()
            && now >= cache.checked_at
            && now - cache.checked_at < CACHE_TTL.as_secs();
        if let (false, true, Some(latest)) = (force, fresh, cache.latest.clone()) {
            return Ok((latest, cache.checked_at, cache.dismissed));
        }
        let latest = fetch()?;
        cache.latest = Some(latest.clone());
        cache.checked_at = now;
        // La cache è solo un'ottimizzazione: se non si scrive, il controllo vale lo stesso.
        let _ = self.save(&cache);
        Ok((latest, now, cache.dismissed))
    }

    pub(crate) fn dismiss(&self, version: &str) -> Result<(), CswapError> {
        let mut cache = self.load();
        cache.dismissed = Some(version.trim().to_owned());
        self.save(&cache)
    }
}

/// Costruisce l'esito confrontando installata e ultima.
pub(crate) fn info(
    installed: Option<CswapVersion>,
    latest: String,
    checked_at: u64,
    dismissed: Option<String>,
) -> UpdateInfo {
    let newer = parse_stable(&latest).is_some_and(|l| installed.is_some_and(|i| l > i));
    UpdateInfo {
        installed: installed.map(|v| v.to_string()),
        dismissed: dismissed.as_deref() == Some(latest.as_str()),
        update_available: newer,
        latest,
        checked_at,
    }
}

pub(crate) fn fetch_latest(agent: &ureq::Agent, url: &str) -> Result<String, CswapError> {
    parse_pypi(&super::http::get_text(agent, url, MAX_PYPI_BYTES)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pypi/claude-swap.json"),
        )
        .unwrap()
    }

    #[test]
    fn pypi_fixture_is_parsed() {
        assert_eq!(parse_pypi(&fixture()).unwrap(), "0.27.2");
        assert!(parse_pypi("{}").is_err());
        assert!(parse_pypi("non json").is_err());
    }

    #[test]
    fn stable_versions_only() {
        assert_eq!(parse_stable("0.27.2").unwrap().to_string(), "0.27.2");
        assert!(parse_stable("0.28.0b1").is_none());
        assert!(parse_stable("1.2").is_none());
        assert!(parse_stable("1.2.3.4").is_none());
    }

    #[test]
    fn update_info_compares_versions() {
        let v = |s: &str| parse_stable(s);
        assert!(info(v("0.27.2"), "0.28.0".into(), 1, None).update_available);
        assert!(!info(v("0.27.2"), "0.27.2".into(), 1, None).update_available);
        assert!(!info(v("0.27.2"), "0.28.0rc1".into(), 1, None).update_available);
        assert!(!info(None, "0.28.0".into(), 1, None).update_available);
        let d = info(v("0.27.2"), "0.28.0".into(), 1, Some("0.28.0".into()));
        assert!(d.dismissed && d.update_available);
        assert!(!info(v("0.27.2"), "0.28.1".into(), 1, Some("0.28.0".into())).dismissed);
    }

    #[test]
    fn cache_is_used_for_six_hours() {
        let tmp = tempfile::tempdir().unwrap();
        let store = UpdateStore::new(&tmp.path().join("data"));
        let t0 = 1_000_000;
        let (l, at, _) = store.latest(false, t0, || Ok("0.28.0".into())).unwrap();
        assert_eq!((l.as_str(), at), ("0.28.0", t0));
        let never = || -> Result<String, CswapError> { panic!("niente rete con la cache fresca") };
        let (l, at, _) = store.latest(false, t0 + 3600, never).unwrap();
        assert_eq!((l.as_str(), at), ("0.28.0", t0));
        // Scaduta, forzata o con l'orologio tornato indietro: si richiede.
        let later = t0 + CACHE_TTL.as_secs();
        assert_eq!(
            store
                .latest(false, later, || Ok("0.29.0".into()))
                .unwrap()
                .0,
            "0.29.0"
        );
        assert_eq!(
            store.latest(true, later, || Ok("0.29.1".into())).unwrap().0,
            "0.29.1"
        );
        assert_eq!(
            store.latest(false, 5, || Ok("0.29.2".into())).unwrap().0,
            "0.29.2"
        );
        // Errore di rete: si propaga, la cache resta.
        assert!(
            store
                .latest(true, later, || Err(CswapError::new(
                    CswapErrorKind::Timeout,
                    "x"
                )))
                .is_err()
        );
        store.dismiss("0.29.2").unwrap();
        let (_, _, dismissed) = store.latest(false, 6, never).unwrap();
        assert_eq!(dismissed.as_deref(), Some("0.29.2"));
        // File rovinato: si riparte da zero.
        fs::write(tmp.path().join("data").join(CACHE_FILE), "{rotto").unwrap();
        assert_eq!(
            store.latest(false, 7, || Ok("0.30.0".into())).unwrap().0,
            "0.30.0"
        );
    }
}
