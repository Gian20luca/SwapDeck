//! Primo avvio e manutenzione di cswap (F5): diagnosi, installazione per utente di uv e
//! claude-swap (senza admin), controllo della versione su PyPI, aggiornamento.
//!
//! Le operazioni lunghe riportano l'avanzamento a passi ([`SetupProgress`]) con l'output
//! dei processi riga per riga, ripulito da email e token. Chi chiama l'aggiornamento deve
//! prima fermare i propri processi cswap (motore, `auto`): su Windows `cswap.exe` in uso
//! blocca `uv tool upgrade` (errore `busy`).

mod http;
mod stream;
pub mod update;

use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cswap::error::{CswapError, CswapErrorKind, CswapVersion, classify_uv_failure};
use crate::cswap::locate::{self, SearchEnv, exe_name, locate_cswap, locate_uv};
use crate::cswap::model::SUPPORTED_SCHEMA_VERSION;
use crate::cswap::process::RawOutput;

pub use update::{CACHE_TTL, PYPI_URL, UpdateInfo};

/// Script ufficiale di installazione di uv per Windows (PowerShell).
pub const UV_INSTALL_PS1: &str = "https://astral.sh/uv/install.ps1";
/// Script ufficiale di installazione di uv per Linux e macOS (sh).
pub const UV_INSTALL_SH: &str = "https://astral.sh/uv/install.sh";
const MAX_SCRIPT_BYTES: u64 = 2 * 1024 * 1024;

/// Stato complessivo di cswap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SetupStatus {
    /// Installato, versione e `schemaVersion` supportati.
    Ready,
    /// cswap non trovato: proporre l'installazione.
    CswapMissing,
    /// Versione sotto la minima: proporre l'aggiornamento.
    UnsupportedVersion,
    /// `schemaVersion` diverso da quello supportato: serve una SwapDeck più recente.
    UnsupportedSchema,
    /// cswap presente ma non risponde come previsto (vedi `error`).
    Error,
}

/// Eseguibile trovato.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub path: String,
    /// Versione letta da `--version`, se leggibile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub version: Option<String>,
}

/// Diagnosi di cswap e uv.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetupDiagnosis {
    pub status: SetupStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cswap: Option<ToolInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub uv: Option<ToolInfo>,
    /// Versione minima supportata di cswap.
    pub min_version: String,
    /// `schemaVersion` di `list --json`, se letto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub schema_version: Option<u32>,
    /// `schemaVersion` supportato da questa SwapDeck.
    pub supported_schema_version: u32,
    /// Account gestiti da cswap (0: guidare a `/login` e «Aggiungi account corrente»).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub account_count: Option<u32>,
    /// Problema rilevato, se lo stato non è `ready`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<CswapError>,
}

/// Passo di installazione o aggiornamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SetupStep {
    /// Scaricamento dello script ufficiale di uv.
    DownloadUv,
    /// Esecuzione dello script di uv (per utente).
    InstallUv,
    /// `uv tool install claude-swap`.
    InstallCswap,
    /// `uv tool upgrade claude-swap`.
    UpgradeCswap,
    /// `cswap --version` e `list --json` (`schemaVersion`).
    VerifyCswap,
}

/// Evento di avanzamento.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum SetupProgress {
    StepStarted {
        step: SetupStep,
    },
    StepDone {
        step: SetupStep,
    },
    StepSkipped {
        step: SetupStep,
    },
    StepFailed {
        step: SetupStep,
        error: CswapError,
    },
    /// Riga di output (o nota di SwapDeck, come l'URL dello script), già ripulita.
    Line {
        step: SetupStep,
        text: String,
    },
}

/// Esito di installazione o aggiornamento riuscito.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SetupOutcome {
    /// Diagnosi finale (stato `ready`).
    pub diagnosis: SetupDiagnosis,
    /// uv è stato installato in questa operazione.
    pub uv_installed: bool,
    /// Versione di cswap prima dell'operazione, se c'era.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub previous_version: Option<String>,
}

/// Tempi massimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupTimeouts {
    /// `cswap --version`, `cswap list --json`, `uv --version`.
    pub probe: Duration,
    /// Script di uv, `uv tool install|upgrade` (scaricano Python e pacchetti).
    pub install: Duration,
    /// Richieste HTTP (PyPI, script di uv).
    pub http: Duration,
}

impl Default for SetupTimeouts {
    fn default() -> Self {
        Self {
            probe: Duration::from_secs(30),
            install: Duration::from_secs(600),
            http: Duration::from_secs(15),
        }
    }
}

/// Diagnosi, installazione e aggiornamento di cswap per l'utente corrente.
#[derive(Debug, Clone)]
pub struct Setup {
    search: SearchEnv,
    child_env: Vec<(OsString, OsString)>,
    data_dir: PathBuf,
    timeouts: SetupTimeouts,
    pypi_url: String,
}

type Res<T> = Result<T, CswapError>;
type Progress<'a> = &'a mut dyn FnMut(SetupProgress);

impl Setup {
    /// Ambiente del processo; la cache del controllo versione sta in `data_dir`.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            search: SearchEnv::from_process(),
            child_env: Vec::new(),
            data_dir: data_dir.into(),
            timeouts: SetupTimeouts::default(),
            pypi_url: PYPI_URL.to_owned(),
        }
    }

    /// Ambiente di ricerca degli eseguibili (test, profili isolati).
    pub fn with_search(mut self, search: SearchEnv) -> Self {
        self.search = search;
        self
    }

    /// Variabili aggiunte a ogni processo figlio (per esempio `UV_TOOL_DIR` di un profilo
    /// isolato, o `UV_NO_MODIFY_PATH`). Valgono anche per la ricerca di uv installato.
    pub fn with_child_env(mut self, env: Vec<(OsString, OsString)>) -> Self {
        self.child_env = env;
        self
    }

    pub fn with_timeouts(mut self, timeouts: SetupTimeouts) -> Self {
        self.timeouts = timeouts;
        self
    }

    // ---- diagnosi --------------------------------------------------------------

    /// cswap e uv: presenza, percorso, versione, `schemaVersion` e numero di account.
    /// Solo comandi in lettura (`--version`, `list --json`).
    pub fn diagnose(&self) -> SetupDiagnosis {
        let uv = self.find_uv().map(|path| ToolInfo {
            version: self.uv_version(&path),
            path: path.to_string_lossy().into_owned(),
        });
        let mut diag = SetupDiagnosis {
            status: SetupStatus::CswapMissing,
            cswap: None,
            uv,
            min_version: CswapVersion::MIN_SUPPORTED.to_string(),
            schema_version: None,
            supported_schema_version: SUPPORTED_SCHEMA_VERSION,
            account_count: None,
            error: None,
        };
        let Ok(cswap) = locate_cswap(&self.search) else {
            return diag;
        };
        let version = self.probe(&cswap, &["--version"]).and_then(|out| {
            CswapVersion::parse(&out.stdout).ok_or_else(|| {
                CswapError::new(
                    CswapErrorKind::UnsupportedVersion,
                    "versione di cswap illeggibile",
                )
            })
        });
        diag.cswap = Some(ToolInfo {
            path: cswap.to_string_lossy().into_owned(),
            version: version.as_ref().ok().map(ToString::to_string),
        });
        let version = match version {
            Ok(v) => v,
            Err(e) => return diag.fail(e),
        };
        if !version.is_supported() {
            let e = CswapError::new(
                CswapErrorKind::UnsupportedVersion,
                format!(
                    "cswap {version} non supportato: serve almeno {}",
                    CswapVersion::MIN_SUPPORTED
                ),
            );
            return diag.fail(e);
        }
        match self.probe(&cswap, &["list", "--json"]) {
            Ok(out) => diag.read_list(&out),
            Err(e) => diag.fail(e),
        }
    }

    // ---- installazione ---------------------------------------------------------

    /// Installa uv (se manca, con lo script ufficiale, per utente) e claude-swap
    /// (`uv tool install claude-swap`), poi verifica `--version` e `list --json`.
    pub fn install(&self, progress: Progress<'_>) -> Res<SetupOutcome> {
        let previous = self.installed_version();
        let (uv, uv_installed) = match self.find_uv() {
            Some(uv) => {
                progress(SetupProgress::StepSkipped {
                    step: SetupStep::DownloadUv,
                });
                progress(SetupProgress::StepSkipped {
                    step: SetupStep::InstallUv,
                });
                (uv, false)
            }
            None => (self.install_uv(progress)?, true),
        };
        let step = SetupStep::InstallCswap;
        self.uv_tool(&uv, "install", step, progress)?;
        let diagnosis = self.verify(progress)?;
        Ok(SetupOutcome {
            diagnosis,
            uv_installed,
            previous_version: previous,
        })
    }

    /// `uv tool upgrade claude-swap`, poi verifica di versione e `schemaVersion`
    /// (errore `unsupportedSchema` se è cambiato in modo incompatibile).
    pub fn upgrade(&self, progress: Progress<'_>) -> Res<SetupOutcome> {
        let uv = self.find_uv().ok_or_else(|| {
            CswapError::new(
                CswapErrorKind::NotInstalled,
                "uv non trovato: claude-swap non si può aggiornare",
            )
        })?;
        let previous = self.installed_version();
        self.uv_tool(&uv, "upgrade", SetupStep::UpgradeCswap, progress)?;
        let diagnosis = self.verify(progress)?;
        Ok(SetupOutcome {
            diagnosis,
            uv_installed: false,
            previous_version: previous,
        })
    }

    // ---- versione su PyPI ------------------------------------------------------

    /// Confronta la versione installata con l'ultima su PyPI (cache di 6 ore nella
    /// cartella dati; `force` la ignora).
    pub fn check_update(&self, force: bool) -> Res<UpdateInfo> {
        let installed = self
            .installed_version()
            .and_then(|v| update::parse_stable(&v));
        let store = update::UpdateStore::new(&self.data_dir);
        let (latest, checked_at, dismissed) = store.latest(force, update::now_secs(), || {
            update::fetch_latest(&http::agent(self.timeouts.http), &self.pypi_url)
        })?;
        Ok(update::info(installed, latest, checked_at, dismissed))
    }

    /// Ricorda che l'avviso per `version` è stato chiuso.
    pub fn dismiss_update(&self, version: &str) -> Res<()> {
        if update::parse_stable(version).is_none() {
            return Err(CswapError::new(
                CswapErrorKind::InvalidUsage,
                "versione non valida",
            ));
        }
        update::UpdateStore::new(&self.data_dir).dismiss(version)
    }

    // ---- interni ---------------------------------------------------------------

    fn installed_version(&self) -> Option<String> {
        let cswap = locate_cswap(&self.search).ok()?;
        let out = self.probe(&cswap, &["--version"]).ok()?;
        CswapVersion::parse(&out.stdout).map(|v| v.to_string())
    }

    /// Variabile per i processi figli: prima `child_env`, poi l'ambiente del processo.
    fn env_var(&self, key: &str) -> Option<OsString> {
        self.child_env
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .or_else(|| env::var_os(key))
            .filter(|v| !v.is_empty())
    }

    /// uv: prima le cartelle scelte con `UV_UNMANAGED_INSTALL` o `UV_INSTALL_DIR`
    /// (anche `<dir>/bin`, come fa lo script), poi la ricerca standard.
    fn find_uv(&self) -> Option<PathBuf> {
        let dirs: Vec<PathBuf> = ["UV_UNMANAGED_INSTALL", "UV_INSTALL_DIR"]
            .iter()
            .filter_map(|k| self.env_var(k))
            .flat_map(|d| {
                let d = PathBuf::from(d);
                [d.join("bin"), d]
            })
            .collect();
        locate::find_in(dirs, &exe_name("uv")).or_else(|| locate_uv(&self.search))
    }

    fn uv_version(&self, uv: &Path) -> Option<String> {
        let out = self.probe(uv, &["--version"]).ok()?;
        out.success()
            .then(|| out.stdout.split_whitespace().nth(1).map(str::to_owned))
            .flatten()
    }

    /// Comando breve in lettura, senza output passato a nessuno.
    fn probe(&self, program: &Path, args: &[&str]) -> Res<RawOutput> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        stream::run_streaming(
            program,
            &args,
            &self.child_env,
            self.timeouts.probe,
            &mut |_| {},
        )
    }

    /// Esegue un processo lungo passando le righe come `Line` del passo.
    fn stream(
        &self,
        program: &Path,
        args: &[OsString],
        step: SetupStep,
        progress: Progress<'_>,
    ) -> Res<RawOutput> {
        stream::run_streaming(
            program,
            args,
            &self.child_env,
            self.timeouts.install,
            &mut |text| progress(SetupProgress::Line { step, text }),
        )
    }

    /// `uv tool <verb> claude-swap` con classificazione degli errori di uv.
    fn uv_tool(&self, uv: &Path, verb: &str, step: SetupStep, progress: Progress<'_>) -> Res<()> {
        progress(SetupProgress::StepStarted { step });
        let args: Vec<OsString> = ["tool", verb, "claude-swap"]
            .iter()
            .map(OsString::from)
            .collect();
        let result = self.stream(uv, &args, step, progress).and_then(|out| {
            if out.success() {
                Ok(())
            } else {
                Err(uv_error(&out))
            }
        });
        finish(step, result, progress)
    }

    /// Verifica finale: deve risultare `ready`.
    fn verify(&self, progress: Progress<'_>) -> Res<SetupDiagnosis> {
        let step = SetupStep::VerifyCswap;
        progress(SetupProgress::StepStarted { step });
        let diag = self.diagnose();
        let result = match (diag.status, &diag.error) {
            (SetupStatus::Ready, _) => Ok(()),
            (SetupStatus::CswapMissing, _) => Err(CswapError::new(
                CswapErrorKind::NotInstalled,
                "claude-swap installato ma cswap non trovato (UV_TOOL_BIN_DIR, PATH, ~/.local/bin)",
            )),
            (_, Some(e)) => Err(e.clone()),
            (_, None) => Err(CswapError::new(
                CswapErrorKind::Failed,
                "verifica non riuscita",
            )),
        };
        if let (Ok(()), Some(c)) = (&result, &diag.cswap) {
            let text = format!(
                "cswap {} · schemaVersion {}",
                c.version.as_deref().unwrap_or("?"),
                diag.schema_version.unwrap_or_default()
            );
            progress(SetupProgress::Line { step, text });
        }
        finish(step, result, progress).map(|()| diag)
    }

    /// Scarica lo script ufficiale e lo esegue (uv per utente, senza admin).
    fn install_uv(&self, progress: Progress<'_>) -> Res<PathBuf> {
        let step = SetupStep::DownloadUv;
        progress(SetupProgress::StepStarted { step });
        let url = if cfg!(windows) {
            UV_INSTALL_PS1
        } else {
            UV_INSTALL_SH
        };
        progress(SetupProgress::Line {
            step,
            text: url.to_owned(),
        });
        let dir = tempfile::Builder::new()
            .prefix("swapdeck-uv-")
            .tempdir()
            .map_err(|e| {
                CswapError::new(
                    CswapErrorKind::Failed,
                    format!("cartella temporanea non creata: {e}"),
                )
            })?;
        let script = dir.path().join(if cfg!(windows) {
            "install.ps1"
        } else {
            "install.sh"
        });
        let downloaded = self.download_script(url, &script, progress);
        finish(step, downloaded, progress)?;

        let step = SetupStep::InstallUv;
        progress(SetupProgress::StepStarted { step });
        let (program, args) = script_command(&script);
        let result = self
            .stream(&program, &args, step, progress)
            .and_then(|out| {
                if out.success() {
                    Ok(())
                } else {
                    Err(failure("installazione di uv non riuscita", &out))
                }
            })
            .and_then(|()| {
                self.find_uv().ok_or_else(|| {
                    CswapError::new(
                        CswapErrorKind::NotInstalled,
                        "uv installato ma non trovato (~/.local/bin)",
                    )
                })
            });
        finish(step, result, progress)
    }

    /// Scarica con il client HTTP interno; se non riesce (per esempio un proxy che solo
    /// il sistema conosce) ripiega su PowerShell (Windows) o su curl e poi wget.
    fn download_script(&self, url: &str, dest: &Path, progress: Progress<'_>) -> Res<()> {
        let step = SetupStep::DownloadUv;
        let first = http::get_text(&http::agent(self.timeouts.http), url, MAX_SCRIPT_BYTES)
            .and_then(|text| write_private(dest, text.as_bytes()));
        let Err(first) = first else {
            return Ok(());
        };
        progress(SetupProgress::Line {
            step,
            text: first.message.clone(),
        });
        for (program, args, extra) in fallback_downloaders(&self.search, url, dest) {
            let env: Vec<_> = self.child_env.iter().cloned().chain(extra).collect();
            let run =
                stream::run_streaming(&program, &args, &env, self.timeouts.install, &mut |text| {
                    progress(SetupProgress::Line { step, text })
                });
            if let Ok(out) = run {
                if out.success() && dest.metadata().is_ok_and(|m| m.len() > 0) {
                    return Ok(());
                }
            }
        }
        Err(first)
    }
}

impl SetupDiagnosis {
    fn fail(mut self, e: CswapError) -> Self {
        self.status = match e.kind {
            CswapErrorKind::UnsupportedVersion => SetupStatus::UnsupportedVersion,
            CswapErrorKind::UnsupportedSchema => SetupStatus::UnsupportedSchema,
            CswapErrorKind::NotInstalled => SetupStatus::CswapMissing,
            _ => SetupStatus::Error,
        };
        self.error = Some(e);
        self
    }

    /// Legge `schemaVersion` e il numero di account da `list --json` (anche dalla busta
    /// d'errore, che porta comunque `schemaVersion`).
    fn read_list(mut self, out: &RawOutput) -> Self {
        let value: Option<serde_json::Value> = serde_json::from_str(out.stdout.trim()).ok();
        let Some(value) = value else {
            let e = if out.success() {
                CswapError::new(CswapErrorKind::Failed, "risposta di cswap non valida")
            } else {
                crate::cswap::error::classify_failure(out.code, &out.stdout, &out.stderr)
            };
            return self.fail(e);
        };
        let schema = value
            .get("schemaVersion")
            .and_then(serde_json::Value::as_u64)
            .map(|s| u32::try_from(s).unwrap_or(u32::MAX));
        self.schema_version = schema;
        match schema {
            Some(s) if s == SUPPORTED_SCHEMA_VERSION => {}
            Some(s) => {
                return self.fail(CswapError::new(
                    CswapErrorKind::UnsupportedSchema,
                    format!(
                        "schemaVersion {s} non supportato (atteso {SUPPORTED_SCHEMA_VERSION}): serve una versione più recente di SwapDeck"
                    ),
                ));
            }
            None => {
                return self.fail(CswapError::new(
                    CswapErrorKind::UnsupportedSchema,
                    "risposta di cswap senza schemaVersion",
                ));
            }
        }
        if !out.success() || value.get("error").is_some() {
            let e = crate::cswap::error::classify_failure(
                out.code.or(Some(1)),
                &out.stdout,
                &out.stderr,
            );
            return self.fail(e);
        }
        self.account_count = value
            .get("accounts")
            .and_then(serde_json::Value::as_array)
            .map(|a| u32::try_from(a.len()).unwrap_or(u32::MAX));
        self.status = SetupStatus::Ready;
        self
    }
}

/// Chiude un passo con `StepDone` o `StepFailed`.
fn finish<T>(step: SetupStep, result: Res<T>, progress: Progress<'_>) -> Res<T> {
    match result {
        Ok(value) => {
            progress(SetupProgress::StepDone { step });
            Ok(value)
        }
        Err(error) => {
            progress(SetupProgress::StepFailed {
                step,
                error: error.clone(),
            });
            Err(error)
        }
    }
}

/// Errore di `uv tool`: `busy` con la spiegazione su cosa fare.
fn uv_error(out: &RawOutput) -> CswapError {
    let mut e = classify_uv_failure(out.code, &out.stdout, &out.stderr);
    if e.kind == CswapErrorKind::Busy {
        e.message = "un file di cswap è in uso: chiudi i processi cswap (cswap auto, la TUI, \
                     altre finestre) e riprova"
            .to_owned();
    }
    e
}

fn failure(what: &str, out: &RawOutput) -> CswapError {
    let detail = out
        .stderr
        .lines()
        .chain(out.stdout.lines())
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .map(crate::cswap::error::clean_message)
        .unwrap_or_default();
    let code = out.code.map_or("?".into(), |c| c.to_string());
    let mut e = CswapError::new(
        CswapErrorKind::Failed,
        if detail.is_empty() {
            format!("{what} (uscita {code})")
        } else {
            format!("{what}: {detail}")
        },
    );
    e.exit_code = out.code;
    e
}

fn write_private(dest: &Path, bytes: &[u8]) -> Res<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(dest)
        .and_then(|mut f| f.write_all(bytes).and_then(|()| f.sync_all()))
        .map_err(|e| {
            CswapError::new(
                CswapErrorKind::Failed,
                format!("script di uv non salvato: {e}"),
            )
        })
}

/// Interprete dello script, con percorso assoluto.
fn script_command(script: &Path) -> (PathBuf, Vec<OsString>) {
    if cfg!(windows) {
        let args = [
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ]
        .iter()
        .map(OsString::from)
        .chain([script.as_os_str().to_owned()])
        .collect();
        (windows_powershell(), args)
    } else {
        (
            PathBuf::from("/bin/sh"),
            vec![script.as_os_str().to_owned()],
        )
    }
}

fn windows_powershell() -> PathBuf {
    env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
}

/// Programma, argomenti e variabili d'ambiente di uno scaricatore.
type Downloader = (PathBuf, Vec<OsString>, Vec<(OsString, OsString)>);

/// Scaricatori di ripiego, nell'ordine.
fn fallback_downloaders(search: &SearchEnv, url: &str, dest: &Path) -> Vec<Downloader> {
    let os = |s: &str| OsString::from(s);
    let dest = dest.as_os_str().to_owned();
    if cfg!(windows) {
        // URL e destinazione passano per l'ambiente: niente interpolazione nel comando.
        let cmd = "$ProgressPreference = 'SilentlyContinue'; \
                   Invoke-RestMethod -Uri $env:SWAPDECK_DL_URL -OutFile $env:SWAPDECK_DL_OUT";
        return vec![(
            windows_powershell(),
            vec![
                os("-NoProfile"),
                os("-NonInteractive"),
                os("-Command"),
                os(cmd),
            ],
            vec![
                (os("SWAPDECK_DL_URL"), os(url)),
                (os("SWAPDECK_DL_OUT"), dest),
            ],
        )];
    }
    let mut list = Vec::new();
    if let Some(curl) = locate::locate("curl", search) {
        let args = ["-LsSf", "--max-time", "120", url, "-o"].map(os);
        list.push((
            curl,
            [args.to_vec(), vec![dest.clone()]].concat(),
            Vec::new(),
        ));
    }
    if let Some(wget) = locate::locate("wget", search) {
        let mut args = ["-q", "-T", "120", "-O"].map(os).to_vec();
        args.extend([dest, os(url)]);
        list.push((wget, args, Vec::new()));
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_upgrade_explains_what_to_do() {
        let out = RawOutput {
            code: Some(2),
            stdout: String::new(),
            stderr: "error: failed to remove file: Il processo non può accedere al file perché è \
                     utilizzato da un altro processo. (os error 32)"
                .into(),
        };
        let e = uv_error(&out);
        assert_eq!(e.kind, CswapErrorKind::Busy);
        assert!(e.message.contains("chiudi i processi cswap"));
        let other = RawOutput {
            code: Some(2),
            stdout: String::new(),
            stderr: "error: No network".into(),
        };
        assert_eq!(uv_error(&other).kind, CswapErrorKind::Failed);
    }

    #[test]
    fn script_commands_use_absolute_interpreters() {
        let (program, args) = script_command(Path::new("x"));
        assert!(program.is_absolute());
        assert_eq!(args.last().unwrap(), "x");
        for (p, _, _) in fallback_downloaders(&SearchEnv::default(), "https://e", Path::new("o")) {
            assert!(p.is_absolute());
        }
    }

    #[test]
    fn progress_serializes_with_event_tag() {
        let p = SetupProgress::Line {
            step: SetupStep::InstallCswap,
            text: "ok".into(),
        };
        assert_eq!(
            serde_json::to_string(&p).unwrap(),
            r#"{"event":"line","step":"installCswap","text":"ok"}"#
        );
    }
}
