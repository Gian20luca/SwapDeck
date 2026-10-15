//! API Rust tipizzata per ogni comando di cswap usato da SwapDeck (F0).
//!
//! Regole del contratto: sempre un sottocomando esplicito, `--json` per `list`,
//! `status` e `switch`, account per numero, prompt risposti via stdin, esiti
//! ambigui verificati rileggendo `list --json`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use zeroize::Zeroizing;

use super::action::{AutoOptions, CswapAction, RunOptions, SwitchStrategy};
use super::error::{
    CswapError, CswapErrorKind, CswapVersion, check_schema_version, classify_failure, from_envelope,
};
use super::locate::{SearchEnv, locate_cswap, locate_uv};
use super::model::{
    AutoEvent, ConfigGetResponse, ConfigListResponse, ErrorEnvelope, ListResponse, StatusResponse,
    SwitchResponse,
};
use super::process::{self, RawOutput};
use super::secret::SecretToken;

/// Tempi massimi dei comandi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// `list`, `status`, `switch` (possono fare fetch di rete e rinnovi token).
    pub data: Duration,
    /// Tutti gli altri comandi di cswap.
    pub other: Duration,
    /// `uv tool upgrade claude-swap` (scarica da PyPI).
    pub upgrade: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            data: Duration::from_secs(30),
            other: Duration::from_secs(10),
            upgrade: Duration::from_secs(180),
        }
    }
}

/// Comando da aprire in un terminale nuovo (`cswap run`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommandSpec {
    #[ts(type = "string")]
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Esito di un'azione riuscita.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ActionOutcome {
    /// Testo stampato da cswap (o descrizione dell'esito).
    pub message: String,
    /// Elenco riletto dopo l'azione, se l'azione cambia gli account.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub list: Option<ListResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub switch: Option<SwitchResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub command: Option<CommandSpec>,
}

impl ActionOutcome {
    fn text(message: String) -> Self {
        Self {
            message,
            list: None,
            switch: None,
            command: None,
        }
    }
}

/// Client di cswap.
#[derive(Debug, Clone)]
pub struct CswapClient {
    program: PathBuf,
    uv: Option<PathBuf>,
    timeouts: Timeouts,
}

type Res<T> = Result<T, CswapError>;

fn invalid(msg: impl AsRef<str>) -> CswapError {
    CswapError::new(CswapErrorKind::InvalidUsage, msg)
}

/// Argomenti da riga di comando, con controllo dei valori liberi.
#[derive(Default)]
struct Args(Vec<OsString>);

impl Args {
    fn new(cmd: &[&str]) -> Self {
        Self(cmd.iter().map(OsString::from).collect())
    }
    fn flag(mut self, f: &str) -> Self {
        self.0.push(f.into());
        self
    }
    fn flag_if(self, cond: bool, f: &str) -> Self {
        if cond { self.flag(f) } else { self }
    }
    fn num(mut self, n: u32) -> Res<Self> {
        if n == 0 {
            return Err(invalid("il numero di slot deve essere >= 1"));
        }
        self.0.push(n.to_string().into());
        Ok(self)
    }
    /// Valore libero: non vuoto, senza NUL né a capo, non deve sembrare un'opzione.
    fn value(mut self, v: &str) -> Res<Self> {
        if v.trim().is_empty() || v.starts_with('-') || v.contains(['\0', '\n', '\r']) {
            return Err(invalid(format!("valore non valido: {v:?}")));
        }
        self.0.push(v.into());
        Ok(self)
    }
    fn opt_value(self, f: &str, v: Option<&str>) -> Res<Self> {
        match v {
            Some(v) => self.flag(f).value(v),
            None => Ok(self),
        }
    }
    fn opt_num(self, f: &str, n: Option<u32>) -> Res<Self> {
        match n {
            Some(n) => self.flag(f).num(n),
            None => Ok(self),
        }
    }
    fn opt_f64(mut self, f: &str, v: Option<f64>) -> Res<Self> {
        if let Some(v) = v {
            if !v.is_finite() || v < 0.0 {
                return Err(invalid(format!("{f}: valore non valido")));
            }
            self.0.push(f.into());
            self.0.push(v.to_string().into());
        }
        Ok(self)
    }
}

fn models_arg(models: &[String]) -> Res<String> {
    if models.is_empty()
        || models
            .iter()
            .any(|m| m.trim().is_empty() || m.contains(',') || m.starts_with('-'))
    {
        return Err(invalid("--model: nomi di modello non validi"));
    }
    Ok(models.join(","))
}

/// Testo di uscita senza sequenze ANSI, ai bordi ripulito.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if c != '\r' {
            out.push(c);
        }
    }
    out.trim().to_owned()
}

/// `true` se cswap ha stampato `Cancelled` (prompt rifiutato, uscita 0).
fn was_cancelled(stdout: &str) -> bool {
    stdout.lines().any(|l| l.trim_end().ends_with("Cancelled"))
}

impl CswapClient {
    /// Client per un eseguibile già noto.
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            uv: None,
            timeouts: Timeouts::default(),
        }
    }

    /// Cerca cswap (e uv) nell'ambiente del processo.
    pub fn locate() -> Res<Self> {
        let env = SearchEnv::from_process();
        let program = locate_cswap(&env)?;
        Ok(Self::new(program).with_uv(locate_uv(&env)))
    }

    /// Imposta i tempi massimi.
    pub fn with_timeouts(mut self, timeouts: Timeouts) -> Self {
        self.timeouts = timeouts;
        self
    }

    /// Imposta l'eseguibile di uv per `upgrade`.
    pub fn with_uv(mut self, uv: Option<PathBuf>) -> Self {
        self.uv = uv;
        self
    }

    /// Percorso dell'eseguibile di cswap.
    pub fn program(&self) -> &Path {
        &self.program
    }

    fn exec(&self, args: Args, input: &[u8], timeout: Duration) -> Res<RawOutput> {
        process::run(
            &self.program,
            &args.0,
            Zeroizing::new(input.to_vec()),
            timeout,
        )
    }

    /// Esegue e restituisce l'uscita solo con codice 0.
    fn exec_ok(&self, args: Args, input: &[u8]) -> Res<RawOutput> {
        let out = self.exec(args, input, self.timeouts.other)?;
        if out.success() {
            Ok(out)
        } else {
            Err(classify_failure(out.code, &out.stdout, &out.stderr))
        }
    }

    fn exec_json<T: DeserializeOwned>(&self, args: Args, timeout: Duration) -> Res<T> {
        let out = self.exec(args, b"", timeout)?;
        if !out.success() {
            return Err(classify_failure(out.code, &out.stdout, &out.stderr));
        }
        parse_json(&out.stdout)
    }

    fn text(&self, args: Args) -> Res<String> {
        self.exec_ok(args, b"").map(|o| plain(&o.stdout))
    }

    /// Azione che cambia gli account: messaggio più elenco riletto (se leggibile).
    fn mutate(&self, args: Args, input: &[u8]) -> Res<ActionOutcome> {
        let out = self.exec_ok(args, input)?;
        if was_cancelled(&out.stdout) {
            return Err(CswapError::new(
                CswapErrorKind::Cancelled,
                "operazione annullata",
            ));
        }
        let mut outcome = ActionOutcome::text(plain(&out.stdout));
        outcome.list = self.list().ok();
        Ok(outcome)
    }

    // ---- lettura -------------------------------------------------------------

    /// `cswap --version`.
    pub fn version(&self) -> Res<CswapVersion> {
        let out = self.exec_ok(Args::new(&["--version"]), b"")?;
        CswapVersion::parse(&out.stdout).ok_or_else(|| {
            CswapError::new(
                CswapErrorKind::UnsupportedVersion,
                "versione di cswap illeggibile",
            )
        })
    }

    /// Versione, con errore `UnsupportedVersion` sotto la minima.
    pub fn check_version(&self) -> Res<CswapVersion> {
        let v = self.version()?;
        if v.is_supported() {
            Ok(v)
        } else {
            Err(CswapError::new(
                CswapErrorKind::UnsupportedVersion,
                format!(
                    "cswap {v} non supportato: serve almeno {}",
                    CswapVersion::MIN_SUPPORTED
                ),
            ))
        }
    }

    /// `cswap list --json`.
    pub fn list(&self) -> Res<ListResponse> {
        self.exec_json(Args::new(&["list", "--json"]), self.timeouts.data)
    }

    /// `cswap status --json`.
    pub fn status(&self) -> Res<StatusResponse> {
        self.exec_json(Args::new(&["status", "--json"]), self.timeouts.data)
    }

    /// `cswap alias` (testo).
    pub fn aliases(&self) -> Res<String> {
        self.text(Args::new(&["alias"]))
    }

    /// `cswap map` (testo).
    pub fn maps(&self) -> Res<String> {
        self.text(Args::new(&["map"]))
    }

    /// `cswap unclaimed` (testo).
    pub fn unclaimed(&self) -> Res<String> {
        self.text(Args::new(&["unclaimed"]))
    }

    /// `cswap config --json`.
    pub fn config_list(&self) -> Res<ConfigListResponse> {
        self.exec_json(
            Args::new(&["config", "list", "--json"]),
            self.timeouts.other,
        )
    }

    /// `cswap config get KEY --json`.
    pub fn config_get(&self, key: &str) -> Res<ConfigGetResponse> {
        self.exec_json(
            Args::new(&["config", "get"]).value(key)?.flag("--json"),
            self.timeouts.other,
        )
    }

    /// `cswap config path`.
    pub fn config_path(&self) -> Res<String> {
        self.text(Args::new(&["config", "path"]))
    }

    // ---- azioni --------------------------------------------------------------

    /// `cswap switch [<n>] [--strategy S [--model M]] [--force] --json`, verificato con `list`.
    pub fn switch(
        &self,
        target: Option<u32>,
        strategy: Option<SwitchStrategy>,
        models: Option<&[String]>,
        force: bool,
    ) -> Res<ActionOutcome> {
        if target.is_some() && strategy.is_some() {
            return Err(invalid("switch: account e strategia sono alternativi"));
        }
        if models.is_some() && strategy.is_none() {
            return Err(invalid("switch: --model richiede --strategy"));
        }
        if force && target.is_none() {
            return Err(invalid("switch: --force richiede un account"));
        }
        let mut args = Args::new(&["switch"]);
        if let Some(n) = target {
            args = args.num(n)?;
        }
        if let Some(s) = strategy {
            args = args.flag("--strategy").flag(s.as_arg());
        }
        if let Some(m) = models {
            args = args.flag("--model").value(&models_arg(m)?)?;
        }
        let args = args.flag_if(force, "--force").flag("--json");
        let resp: SwitchResponse = self.exec_json(args, self.timeouts.data)?;
        let list = self.list()?;
        if resp.switched {
            let expected = resp.to.as_ref().and_then(|t| t.number);
            if expected.is_some() && list.active_account_number != expected {
                return Err(CswapError::new(
                    CswapErrorKind::Failed,
                    "cambio di account non confermato da list --json",
                ));
            }
        }
        Ok(ActionOutcome {
            message: resp.message.clone(),
            list: Some(list),
            switch: Some(resp),
            command: None,
        })
    }

    /// `cswap add [--slot N] [--alias A]`; con slot occupato risponde `y` solo se `overwrite`.
    pub fn add(
        &self,
        slot: Option<u32>,
        alias: Option<&str>,
        overwrite: bool,
    ) -> Res<ActionOutcome> {
        let args = Args::new(&["add"])
            .opt_num("--slot", slot)?
            .opt_value("--alias", alias)?;
        self.mutate(args, if overwrite { b"y\n" } else { b"n\n" })
    }

    /// `cswap add-token - [--email E] [--slot N]`: il token passa solo da stdin.
    pub fn add_token(
        &self,
        token: &SecretToken,
        email: Option<&str>,
        slot: Option<u32>,
        overwrite: bool,
    ) -> Res<ActionOutcome> {
        if token.is_empty() || token.has_line_break() {
            return Err(invalid("token vuoto o su più righe"));
        }
        let args = Args::new(&["add-token", "-"])
            .opt_value("--email", email)?
            .opt_num("--slot", slot)?;
        let mut input = Zeroizing::new(Vec::with_capacity(token.expose().len() + 4));
        input.extend_from_slice(token.expose().as_bytes());
        input.extend_from_slice(if overwrite { b"\ny\n" } else { b"\nn\n" });
        let out = self.exec(args, &input, self.timeouts.other)?;
        if !out.success() {
            return Err(classify_failure(out.code, &out.stdout, &out.stderr));
        }
        if was_cancelled(&out.stdout) {
            return Err(CswapError::new(
                CswapErrorKind::Cancelled,
                "operazione annullata",
            ));
        }
        Ok(ActionOutcome {
            message: plain(&out.stdout),
            list: self.list().ok(),
            switch: None,
            command: None,
        })
    }

    /// `cswap remove <n>` con risposta `y`; verificato con `list`.
    pub fn remove(&self, account: u32) -> Res<ActionOutcome> {
        let out = self.exec_ok(Args::new(&["remove"]).num(account)?, b"y\n")?;
        let list = self.list()?;
        if list.accounts.iter().any(|a| a.number == account) {
            return Err(CswapError::new(
                CswapErrorKind::Cancelled,
                format!("Account-{account} non rimosso"),
            ));
        }
        Ok(ActionOutcome {
            message: plain(&out.stdout),
            list: Some(list),
            switch: None,
            command: None,
        })
    }

    fn set_disabled(&self, account: u32, disabled: bool) -> Res<ActionOutcome> {
        let cmd = if disabled { "disable" } else { "enable" };
        let out = self.exec_ok(Args::new(&[cmd]).num(account)?, b"")?;
        let list = self.list()?;
        match list.accounts.iter().find(|a| a.number == account) {
            Some(a) if a.is_disabled() == disabled => Ok(ActionOutcome {
                message: plain(&out.stdout),
                list: Some(list),
                switch: None,
                command: None,
            }),
            _ => Err(CswapError::new(
                CswapErrorKind::Failed,
                format!("{cmd} di Account-{account} non confermato da list --json"),
            )),
        }
    }

    /// `cswap disable <n>`, verificato con `list`.
    pub fn disable(&self, account: u32) -> Res<ActionOutcome> {
        self.set_disabled(account, true)
    }

    /// `cswap enable <n>`, verificato con `list`.
    pub fn enable(&self, account: u32) -> Res<ActionOutcome> {
        self.set_disabled(account, false)
    }

    /// `cswap alias <n> <name>`.
    pub fn set_alias(&self, account: u32, alias: &str) -> Res<ActionOutcome> {
        self.mutate(Args::new(&["alias"]).num(account)?.value(alias)?, b"")
    }

    /// `cswap alias <n> --unset`.
    pub fn unset_alias(&self, account: u32) -> Res<ActionOutcome> {
        self.mutate(Args::new(&["alias"]).num(account)?.flag("--unset"), b"")
    }

    /// `cswap swap <a> <b>`.
    pub fn swap(&self, a: u32, b: u32) -> Res<ActionOutcome> {
        if a == b {
            return Err(invalid("swap: i due account coincidono"));
        }
        self.mutate(Args::new(&["swap"]).num(a)?.num(b)?, b"")
    }

    /// `cswap move <n> <slot>`.
    pub fn move_account(&self, account: u32, slot: u32) -> Res<ActionOutcome> {
        self.mutate(Args::new(&["move"]).num(account)?.num(slot)?, b"")
    }

    /// `cswap map <n> <path>`.
    pub fn map(&self, account: u32, path: &str) -> Res<ActionOutcome> {
        let out = self.exec_ok(Args::new(&["map"]).num(account)?.value(path)?, b"")?;
        Ok(ActionOutcome::text(plain(&out.stdout)))
    }

    /// `cswap unmap <path>` (uscita 0 anche senza mappatura).
    pub fn unmap(&self, path: &str) -> Res<ActionOutcome> {
        let out = self.exec_ok(Args::new(&["unmap"]).value(path)?, b"")?;
        Ok(ActionOutcome::text(plain(&out.stdout)))
    }

    /// `cswap config set KEY VALUE` (cswap valida chiave e intervallo).
    pub fn config_set(&self, key: &str, value: &str) -> Res<ActionOutcome> {
        let out = self.exec_ok(Args::new(&["config", "set"]).value(key)?.value(value)?, b"")?;
        Ok(ActionOutcome::text(plain(&out.stdout)))
    }

    /// `cswap config unset KEY`.
    pub fn config_unset(&self, key: &str) -> Res<ActionOutcome> {
        let out = self.exec_ok(Args::new(&["config", "unset"]).value(key)?, b"")?;
        Ok(ActionOutcome::text(plain(&out.stdout)))
    }

    /// `cswap export <path> [--account <n>] [--full]`. Solo su file, mai su stdout.
    pub fn export(&self, path: &str, account: Option<u32>, full: bool) -> Res<ActionOutcome> {
        let args = Args::new(&["export"])
            .value(path)?
            .opt_num("--account", account)?
            .flag_if(full, "--full");
        let out = self.exec_ok(args, b"")?;
        Ok(ActionOutcome::text(plain(&out.stdout)))
    }

    /// `cswap import <path> [--force]`. Solo da file.
    pub fn import(&self, path: &str, force: bool) -> Res<ActionOutcome> {
        self.mutate(
            Args::new(&["import"])
                .value(path)?
                .flag_if(force, "--force"),
            b"",
        )
    }

    /// `cswap unclaimed --purge <id>`.
    pub fn unclaimed_purge(&self, id: &str) -> Res<ActionOutcome> {
        let out = self.exec_ok(Args::new(&["unclaimed", "--purge"]).value(id)?, b"")?;
        Ok(ActionOutcome::text(plain(&out.stdout)))
    }

    /// `cswap purge` con risposta `y`; verificato con `list` (nessun account rimasto).
    pub fn purge(&self) -> Res<ActionOutcome> {
        let out = self.exec_ok(Args::new(&["purge"]), b"y\n")?;
        let list = self.list()?;
        if !list.accounts.is_empty() {
            return Err(CswapError::new(
                CswapErrorKind::Cancelled,
                "purge non eseguito: gli account sono ancora presenti",
            ));
        }
        Ok(ActionOutcome {
            message: plain(&out.stdout),
            list: Some(list),
            switch: None,
            command: None,
        })
    }

    /// `uv tool upgrade claude-swap` (su Windows `cswap upgrade` non aggiorna).
    /// Va lanciato con cswap fermo; poi si rilegge la versione.
    pub fn upgrade(&self) -> Res<ActionOutcome> {
        let uv = self
            .uv
            .as_ref()
            .ok_or_else(|| CswapError::new(CswapErrorKind::NotInstalled, "uv non trovato"))?;
        let args: Vec<OsString> = ["tool", "upgrade", "claude-swap"]
            .iter()
            .map(OsString::from)
            .collect();
        let out = process::run(uv, &args, Zeroizing::new(Vec::new()), self.timeouts.upgrade)?;
        if !out.success() {
            return Err(classify_failure(out.code, &out.stdout, &out.stderr));
        }
        let version = self.check_version()?;
        let mut msg = plain(&format!("{}\n{}", out.stdout, out.stderr));
        msg.push_str(&format!("\ncswap {version}"));
        Ok(ActionOutcome::text(msg.trim().to_owned()))
    }

    /// Comando `cswap run ...` da aprire in un terminale nuovo (non lo esegue).
    pub fn run_command(&self, opts: &RunOptions) -> Res<CommandSpec> {
        if cfg!(windows) && opts.share_history {
            return Err(invalid("run --share-history non è supportato su Windows"));
        }
        let mut args = Args::new(&["run"]);
        if let Some(n) = opts.account {
            args = args.num(n)?;
        }
        let mut args = args
            .flag_if(opts.no_share, "--no-share")
            .flag_if(opts.share_history, "--share-history")
            .flag_if(opts.require_session, "--require-session");
        if !opts.claude_args.is_empty() {
            args = args.flag("--");
            for a in &opts.claude_args {
                if a.contains(['\0', '\n', '\r']) {
                    return Err(invalid("argomento di claude non valido"));
                }
                args.0.push(a.into());
            }
        }
        Ok(CommandSpec {
            program: self.program.clone(),
            args: args
                .0
                .into_iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect(),
        })
    }

    /// Avvia `cswap auto --json ...` come processo figlio.
    pub fn start_auto(&self, opts: &AutoOptions) -> Res<AutoProcess> {
        let mut args = Args::new(&["auto", "--json"])
            .flag_if(opts.once, "--once")
            .opt_f64("--interval", opts.interval_seconds)?
            .opt_f64("--threshold", opts.threshold)?
            .opt_f64("--cooldown", opts.cooldown_seconds)?;
        if let Some(m) = &opts.models {
            args = args.flag("--model").value(&models_arg(m)?)?;
        }
        args = match opts.include_api_key_accounts {
            Some(true) => args.flag("--include-api-key-accounts"),
            Some(false) => args.flag("--no-include-api-key-accounts"),
            None => args,
        };
        if let Some(s) = opts.strategy {
            args = args.flag("--strategy").flag(s.as_arg());
        }
        let args = args.flag_if(opts.dry_run, "--dry-run");
        AutoProcess::spawn(&self.program, &args.0)
    }

    /// Esegue un'azione dell'interfaccia (la conferma va chiesta prima, vedi
    /// [`CswapAction::confirmation`]). `AutoStart`/`AutoStop` li gestisce il chiamante.
    pub fn execute(&self, action: &CswapAction) -> Res<ActionOutcome> {
        use CswapAction as A;
        match action {
            A::Switch {
                target,
                strategy,
                models,
                force,
            } => self.switch(*target, *strategy, models.as_deref(), *force),
            A::Add {
                slot,
                alias,
                overwrite,
            } => self.add(*slot, alias.as_deref(), *overwrite),
            A::AddToken {
                token,
                email,
                slot,
                overwrite,
            } => self.add_token(token, email.as_deref(), *slot, *overwrite),
            A::Remove { account } => self.remove(*account),
            A::Disable { account } => self.disable(*account),
            A::Enable { account } => self.enable(*account),
            A::SetAlias {
                account,
                alias: Some(alias),
            } => self.set_alias(*account, alias),
            A::SetAlias {
                account,
                alias: None,
            } => self.unset_alias(*account),
            A::Swap { a, b } => self.swap(*a, *b),
            A::Move { account, slot } => self.move_account(*account, *slot),
            A::Map { account, path } => self.map(*account, path),
            A::Unmap { path } => self.unmap(path),
            A::ConfigSet { key, value } => self.config_set(key, value),
            A::ConfigUnset { key } => self.config_unset(key),
            A::Export {
                path,
                account,
                full,
            } => self.export(path, *account, *full),
            A::Import { path, force } => self.import(path, *force),
            A::UnclaimedPurge { id } => self.unclaimed_purge(id),
            A::Purge => self.purge(),
            A::Upgrade => self.upgrade(),
            A::Run { options } => {
                let cmd = self.run_command(options)?;
                Ok(ActionOutcome {
                    message: String::new(),
                    list: None,
                    switch: None,
                    command: Some(cmd),
                })
            }
            A::AutoStart { .. } | A::AutoStop => Err(invalid(
                "auto si avvia e si ferma con start_auto e AutoProcess::stop",
            )),
        }
    }
}

/// JSON di stdout: controlla prima `schemaVersion` e la busta d'errore.
fn parse_json<T: DeserializeOwned>(stdout: &str) -> Res<T> {
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|e| {
        CswapError::new(
            CswapErrorKind::Failed,
            format!("risposta JSON di cswap non valida: {e}"),
        )
    })?;
    let schema = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            CswapError::new(
                CswapErrorKind::UnsupportedVersion,
                "risposta di cswap senza schemaVersion",
            )
        })?;
    check_schema_version(u32::try_from(schema).unwrap_or(u32::MAX))?;
    if value.get("error").is_some() {
        if let Ok(env) = serde_json::from_value::<ErrorEnvelope>(value.clone()) {
            return Err(from_envelope(env, Some(0)));
        }
    }
    serde_json::from_value(value).map_err(|e| {
        CswapError::new(
            CswapErrorKind::Failed,
            format!("risposta JSON di cswap inattesa: {e}"),
        )
    })
}

/// Messaggio da `cswap auto --json`.
#[derive(Debug, Clone, PartialEq)]
pub enum AutoMessage {
    Event(AutoEvent),
    /// Busta d'errore finale (il processo sta per uscire).
    Error(CswapError),
}

/// `cswap auto` in esecuzione; si ferma con [`AutoProcess::stop`] o al rilascio.
pub struct AutoProcess {
    child: Option<Child>,
    events: Receiver<AutoMessage>,
    stderr_tail: Arc<Mutex<String>>,
    readers: Vec<JoinHandle<()>>,
}

const STDERR_TAIL_BYTES: usize = 16 * 1024;

impl AutoProcess {
    fn spawn(program: &Path, args: &[OsString]) -> Res<Self> {
        let mut child = process::spawn(process::command(program, args), program)?;
        drop(child.stdin.take());
        let (tx, rx) = mpsc::channel();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let mut readers = Vec::new();
        if let Some(stdout) = stdout {
            readers.push(process::line_reader(stdout, move |line| {
                if let Some(msg) = parse_auto_line(&line) {
                    let _ = tx.send(msg);
                }
            }));
        }
        let tail = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = stderr {
            let tail2 = Arc::clone(&tail);
            readers.push(std::thread::spawn(move || {
                use std::io::Read;
                let mut stderr = stderr;
                let mut chunk = [0u8; 4096];
                while let Ok(n) = stderr.read(&mut chunk) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut t) = tail2.lock() {
                        t.push_str(&String::from_utf8_lossy(&chunk[..n]));
                        if t.len() > STDERR_TAIL_BYTES {
                            let cut = t.len() - STDERR_TAIL_BYTES;
                            let cut = (cut..t.len())
                                .find(|i| t.is_char_boundary(*i))
                                .unwrap_or(t.len());
                            t.drain(..cut);
                        }
                    }
                }
            }));
        }
        Ok(Self {
            child: Some(child),
            events: rx,
            stderr_tail: tail,
            readers,
        })
    }

    /// Canale degli eventi (si chiude quando cswap chiude stdout).
    pub fn events(&self) -> &Receiver<AutoMessage> {
        &self.events
    }

    /// PID del processo figlio.
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    /// Codice di uscita se è terminato (`Some(None)` = terminato da un segnale).
    pub fn try_exit(&mut self) -> Option<Option<i32>> {
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => Some(status.code()),
            _ => None,
        }
    }

    /// Errore classificato di un'uscita non riuscita (da chiamare dopo `try_exit`).
    pub fn exit_error(&self, code: Option<i32>) -> Option<CswapError> {
        // auto --once: 0 cambiato, 2 nessuna azione, 3 bloccato: non sono errori.
        if matches!(code, Some(0 | 2 | 3)) {
            return None;
        }
        let tail = self
            .stderr_tail
            .lock()
            .map(|t| t.clone())
            .unwrap_or_default();
        Some(classify_failure(code, "", &tail))
    }

    /// Ferma cswap auto e i suoi discendenti.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            process::terminate_tree(&mut child, Duration::from_secs(3));
        }
        // I lettori finiscono da soli quando le pipe si chiudono: non si attendono,
        // così un discendente rimasto con la pipe aperta non blocca lo stop.
        self.readers.clear();
    }
}

impl Drop for AutoProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Una riga di `auto --json`: evento, busta d'errore o niente (righe vuote o sconosciute).
pub fn parse_auto_line(line: &str) -> Option<AutoMessage> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    if value.get("error").is_some() && value.get("event").is_none() {
        let env: ErrorEnvelope = serde_json::from_value(value).ok()?;
        return Some(AutoMessage::Error(from_envelope(env, None)));
    }
    match serde_json::from_value::<AutoEvent>(value).ok()? {
        AutoEvent::Unknown => None,
        ev => Some(AutoMessage::Event(ev)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_reject_option_like_values() {
        assert!(Args::default().value("--force").is_err());
        assert!(Args::default().value("").is_err());
        assert!(Args::default().value("a\nb").is_err());
        assert!(Args::default().num(0).is_err());
        assert!(Args::default().opt_f64("--x", Some(f64::NAN)).is_err());
        assert!(models_arg(&["a,b".into()]).is_err());
        assert_eq!(
            models_arg(&["Fable".into(), "all".into()]).unwrap(),
            "Fable,all"
        );
    }

    #[test]
    fn cancelled_detection_and_plain() {
        assert!(was_cancelled(
            "Slot 3 already occupied\nOverwrite slot 3? [y/N] Cancelled\n"
        ));
        assert!(!was_cancelled("Added Account-3"));
        assert_eq!(plain("\u{1b}[1mciao\u{1b}[0m\r\n"), "ciao");
    }

    #[test]
    fn run_command_is_built() {
        let c = CswapClient::new("/x/cswap");
        let spec = c
            .run_command(&RunOptions {
                account: Some(2),
                no_share: true,
                claude_args: vec!["--resume".into()],
                ..Default::default()
            })
            .unwrap();
        assert_eq!(spec.args, ["run", "2", "--no-share", "--", "--resume"]);
    }
}
