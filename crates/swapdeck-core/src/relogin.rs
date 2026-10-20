//! F21 «Rifai login» di un'utenza già registrata, senza passare dal terminale.
//!
//! Flusso ([`Relogin::run`]): motore di SwapDeck fermo per tutta l'operazione (il chiamante
//! ferma il proprio; qui si tiene `engine.lock`, o si verifica che lo tenga questo processo),
//! `cswap add` dell'utenza attiva P (ne salva le credenziali), `claude auth login --claudeai
//! --email <email>` con il link mostrato all'interfaccia e il codice scritto su stdin, verifica
//! con `cswap status --json` che l'email collegata sia quella dell'utenza, poi `cswap add` (che
//! aggiorna l'utenza sul posto) e `cswap switch <P>`. Con un'altra email collegata, o dopo
//! annulla, tempo scaduto o login non riuscito con le credenziali cambiate, P si ripristina con
//! `cswap switch <P> --force` (riscrive dal backup senza salvare il login estraneo).
//!
//! Il codice incollato e i token non vanno mai in argv, log, eventi o messaggi; l'URL di accesso
//! va solo nell'evento [`ReloginEvent::Link`] (nel log al massimo «link generato").

mod claude;

use std::path::PathBuf;
use std::process::Child;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use claude::{CLAUDE_TEST_ENV, ClaudeSearch, MAX_CODE_CHARS, locate_claude, valid_setting};

use crate::applog;
use crate::cswap::{
    Account, CswapClient, CswapError, CswapErrorKind, ListResponse, SecretToken, StatusResponse,
    SwitchReason, UsageStatus,
};
use crate::lock::{self, Acquire, LockGuard};
use crate::notify::NotificationConfig;
use crate::timeutil::parse_iso;
use claude::LoginProcess;

type Res<T> = Result<T, CswapError>;

/// Attesa massima del login, dall'avvio di Claude Code.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Intervallo dichiarato nel lock tenuto durante l'operazione (scadenza fra host diversi).
const LOCK_INTERVAL_SECS: u32 = 600;
const POLL: Duration = Duration::from_millis(100);

/// Una sola operazione alla volta nel processo (fra processi vale `engine.lock`).
static RUNNING: AtomicBool = AtomicBool::new(false);

/// `true` se l'utenza va riloggata: `relogin_required` o `token_expired`, oppure login che
/// scade entro i giorni dell'avviso di scadenza delle notifiche (`login.daysBefore`).
pub fn needs_relogin(account: &Account, days_before: u32, now: i64) -> bool {
    if matches!(
        account.usage_status,
        UsageStatus::ReloginRequired | UsageStatus::TokenExpired
    ) {
        return true;
    }
    account
        .login_expires_at
        .as_deref()
        .and_then(parse_iso)
        .is_some_and(|e| e - now <= i64::from(days_before) * 86_400)
}

/// Numeri delle utenze per cui mostrare «Rifai login».
pub fn relogin_accounts(
    list: &ListResponse,
    notifications: &NotificationConfig,
    now: i64,
) -> Vec<u32> {
    list.accounts
        .iter()
        .filter(|a| needs_relogin(a, notifications.login.days_before, now))
        .map(|a| a.number)
        .collect()
}

/// Comando dall'interfaccia durante il login.
#[derive(Debug)]
pub enum ReloginCommand {
    /// Codice incollato dall'utente (scritto su stdin di Claude Code).
    Code(SecretToken),
    Cancel,
}

/// Passo del flusso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReloginStep {
    /// `cswap add` dell'utenza attiva.
    SaveActive,
    /// Avvio di `claude auth login`.
    StartLogin,
    /// Link pronto: si attende il codice.
    WaitCode,
    /// Controllo dell'email collegata.
    Verify,
    /// `cswap add` dell'utenza riloggata.
    SaveAccount,
    /// Ritorno all'utenza di prima.
    RestoreActive,
}

/// Evento di avanzamento (riga JSON nella CLI, evento `relogin-progress` nell'app).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum ReloginEvent {
    StepStarted {
        step: ReloginStep,
    },
    /// Link da far aprire alla persona dell'utenza; `timeoutSeconds` dall'avvio del login.
    #[serde(rename_all = "camelCase")]
    Link {
        url: String,
        #[ts(type = "number")]
        timeout_seconds: u64,
    },
    /// Codice scritto su stdin di Claude Code.
    CodeSent,
    /// Codice vuoto, su più righe o troppo lungo: non inviato.
    CodeInvalid,
}

/// Come è finita l'operazione.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ReloginResult {
    /// Login fatto con l'email giusta e utenza aggiornata.
    Updated,
    /// Login fatto con un'altra email: utenza non aggiornata.
    WrongAccount,
    Cancelled,
    TimedOut,
    /// Claude Code uscito con errore.
    LoginFailed,
    /// Claude Code non trovato (impostazione, `PATH`, `~/.local/bin`, estensione di VS Code).
    ClaudeNotFound,
}

/// Esito dell'operazione.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ReloginOutcome {
    pub result: ReloginResult,
    pub account: u32,
    /// Utenza attiva prima dell'operazione (P), se gestita.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub previous: Option<u32>,
    /// `true` se alla fine è attiva l'utenza di prima (o l'utenza riloggata quando era lei P).
    pub restored: bool,
    /// Passo riuscito con riserva (es. ritorno a P non riuscito), già ripulito.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub warning: Option<String>,
    /// Righe d'errore di Claude Code (redatte, senza URL) per `loginFailed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
    /// Elenco riletto alla fine, se leggibile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub list: Option<ListResponse>,
}

#[derive(Debug)]
struct RunGuard;

impl RunGuard {
    fn take() -> Res<Self> {
        RUNNING
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map(|_| Self)
            .map_err(|_| CswapError::new(CswapErrorKind::Busy, "un «Rifai login» è già in corso"))
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::Release);
    }
}

/// Fine del login (prima delle verifiche).
enum LoginEnd {
    Exited(Option<i32>),
    Cancelled,
    TimedOut,
}

/// Chiamata appena avviato Claude Code (es. per metterlo nel Job Object dell'app su Windows).
#[derive(Clone)]
pub struct SpawnHook(Arc<dyn Fn(&Child) + Send + Sync>);

impl SpawnHook {
    pub fn new(f: impl Fn(&Child) + Send + Sync + 'static) -> Self {
        Self(Arc::new(f))
    }
}

impl std::fmt::Debug for SpawnHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SpawnHook")
    }
}

/// Operazione «Rifai login».
#[derive(Debug, Clone)]
pub struct Relogin {
    client: CswapClient,
    data_dir: Option<PathBuf>,
    claude: Option<PathBuf>,
    login_timeout: Duration,
    on_spawn: Option<SpawnHook>,
}

impl Relogin {
    /// `data_dir`: cartella di `engine.lock` (`None` nei test senza motore); `claude`: da
    /// [`locate_claude`] (`None` = Claude Code non trovato).
    pub fn new(client: CswapClient, data_dir: Option<PathBuf>, claude: Option<PathBuf>) -> Self {
        Self {
            client,
            data_dir,
            claude,
            login_timeout: LOGIN_TIMEOUT,
            on_spawn: None,
        }
    }

    /// Attesa massima del login (per i test).
    pub fn with_login_timeout(mut self, timeout: Duration) -> Self {
        self.login_timeout = timeout;
        self
    }

    /// Funzione chiamata con il processo di Claude Code appena avviato.
    pub fn with_spawn_hook(mut self, hook: SpawnHook) -> Self {
        self.on_spawn = Some(hook);
        self
    }

    /// Esegue il flusso per l'utenza `account`. I comandi arrivano da `commands` (canale chiuso =
    /// annulla); `progress` riceve gli eventi. Gli errori di cswap (`uncertain` compreso) sono
    /// `Err`; le altre fini sono nell'esito.
    pub fn run(
        &self,
        account: u32,
        commands: &Receiver<ReloginCommand>,
        progress: &mut dyn FnMut(ReloginEvent),
    ) -> Res<ReloginOutcome> {
        let _turn = RunGuard::take()?;
        let _lock = self.hold_engine()?;
        applog::info(&format!("rifai login: utenza {account}, avvio"));
        let result = self.flow(account, commands, progress);
        match &result {
            Ok(o) => applog::info(&format!(
                "rifai login: utenza {account}, esito {:?}{}",
                o.result,
                if o.warning.is_some() {
                    " con avviso"
                } else {
                    ""
                }
            )),
            Err(e) => applog::warn(&format!(
                "rifai login: utenza {account}, {}",
                e.log_summary()
            )),
        }
        result
    }

    /// Il motore di un altro processo non deve cambiare utenza durante il login.
    fn hold_engine(&self) -> Res<Option<LockGuard>> {
        let Some(dir) = &self.data_dir else {
            return Ok(None);
        };
        let busy = |e: String| CswapError::new(CswapErrorKind::Busy, e);
        crate::paths::create_private_dir(dir)
            .map_err(|e| CswapError::new(CswapErrorKind::Failed, format!("cartella dati: {e}")))?;
        match lock::try_acquire(dir, LOCK_INTERVAL_SECS, crate::timeutil::now_unix()) {
            Ok(Acquire::Acquired(guard)) => Ok(Some(guard)),
            // Motore di questo processo, già fermato dal chiamante.
            Ok(Acquire::Busy(Some(info))) if info.pid == std::process::id() => Ok(None),
            Ok(Acquire::Busy(Some(info))) => Err(busy(format!(
                "il motore di SwapDeck è attivo (PID {} su {}): fermalo in quella SwapDeck e riprova",
                info.pid, info.host
            ))),
            Ok(Acquire::Busy(None)) => Err(busy("il lock del motore è in uso: riprova".into())),
            Err(e) => Err(CswapError::new(
                CswapErrorKind::Failed,
                format!("lock del motore non disponibile: {e}"),
            )),
        }
    }

    fn flow(
        &self,
        account: u32,
        commands: &Receiver<ReloginCommand>,
        progress: &mut dyn FnMut(ReloginEvent),
    ) -> Res<ReloginOutcome> {
        let list = self.client.list()?;
        let target = list
            .accounts
            .iter()
            .find(|a| a.number == account)
            .ok_or_else(|| {
                CswapError::new(
                    CswapErrorKind::InvalidUsage,
                    format!("utenza {account} non trovata"),
                )
            })?;
        let email = target.email.clone();
        let previous = list.active_account_number;
        let previous_email = previous.and_then(|p| {
            list.accounts
                .iter()
                .find(|a| a.number == p)
                .map(|a| a.email.clone())
        });
        let mut outcome = ReloginOutcome {
            result: ReloginResult::ClaudeNotFound,
            account,
            previous,
            restored: previous.is_some(),
            warning: None,
            detail: None,
            list: None,
        };
        let Some(program) = self.claude.clone() else {
            return Ok(outcome);
        };

        if previous.is_some() {
            progress(ReloginEvent::StepStarted {
                step: ReloginStep::SaveActive,
            });
            self.client
                .add(None, None, false)
                .map_err(|e| context("salvataggio dell'utenza attiva non riuscito", e))?;
        } else {
            let status = self
                .client
                .status()
                .map_err(|e| context("verifica dell'account collegato non riuscita", e))?;
            refuse_unmanaged_login(&status)?;
        }

        progress(ReloginEvent::StepStarted {
            step: ReloginStep::StartLogin,
        });
        let end = match LoginProcess::start(&program, &email) {
            Ok(mut login) => {
                if let Some(hook) = &self.on_spawn {
                    (hook.0)(login.child());
                }
                applog::info(&format!(
                    "rifai login: {} avviato",
                    claude::display_name(&program)
                ));
                let end = self.wait_login(&mut login, commands, progress);
                outcome.detail = login.error_tail();
                end
            }
            Err(e) if e.kind == CswapErrorKind::NotInstalled => return Ok(outcome),
            Err(e) => return Err(e),
        };

        match end {
            LoginEnd::Exited(Some(0)) => {}
            other => {
                outcome.result = match other {
                    LoginEnd::Cancelled => ReloginResult::Cancelled,
                    LoginEnd::TimedOut => ReloginResult::TimedOut,
                    LoginEnd::Exited(_) => ReloginResult::LoginFailed,
                };
                if outcome.result != ReloginResult::LoginFailed {
                    outcome.detail = None;
                }
                self.restore_if_changed(previous_email.as_deref(), &mut outcome, progress);
                self.finish(&mut outcome);
                return Ok(outcome);
            }
        }
        outcome.detail = None;

        // Login riuscito: Claude Code ha le credenziali nuove. Fino ad `add` riuscito ogni
        // errore riporta a P (`switch P --force`) prima di uscire.
        progress(ReloginEvent::StepStarted {
            step: ReloginStep::Verify,
        });
        let status = match self.client.status() {
            Ok(s) => s,
            Err(e) => {
                let e = context("verifica dell'account collegato non riuscita", e);
                return Err(self.fail_back(previous, e, progress));
            }
        };
        let logged = status.active.as_ref().map(|a| a.email.as_str());
        if !logged.is_some_and(|l| l.eq_ignore_ascii_case(&email)) {
            outcome.result = ReloginResult::WrongAccount;
            if let Some(p) = previous {
                let back = self.go_back(p, true, progress);
                apply_back(&mut outcome, p, back);
            }
            self.finish(&mut outcome);
            return Ok(outcome);
        }

        progress(ReloginEvent::StepStarted {
            step: ReloginStep::SaveAccount,
        });
        let added = match self.client.add(None, None, false) {
            Ok(a) => a,
            Err(e) => {
                let e = context("login non salvato, rifallo", e);
                return Err(self.fail_back(previous, e, progress));
            }
        };
        outcome.result = ReloginResult::Updated;
        outcome.list = added.list;
        if let Some(p) = previous.filter(|&p| p != account) {
            let back = self.go_back(p, false, progress);
            apply_back(&mut outcome, p, back);
        }
        self.finish(&mut outcome);
        Ok(outcome)
    }

    fn wait_login(
        &self,
        login: &mut LoginProcess,
        commands: &Receiver<ReloginCommand>,
        progress: &mut dyn FnMut(ReloginEvent),
    ) -> LoginEnd {
        let deadline = Instant::now() + self.login_timeout;
        let mut link_sent = false;
        loop {
            for line in login.drain() {
                if link_sent {
                    continue;
                }
                if let Some(url) = claude::login_url(&line) {
                    link_sent = true;
                    applog::info("rifai login: link generato");
                    progress(ReloginEvent::Link {
                        url,
                        timeout_seconds: self.login_timeout.as_secs(),
                    });
                    progress(ReloginEvent::StepStarted {
                        step: ReloginStep::WaitCode,
                    });
                }
            }
            if let Some(code) = login.try_exit() {
                // Ultime righe dopo l'uscita (per il dettaglio dell'errore).
                std::thread::sleep(POLL);
                login.drain();
                return LoginEnd::Exited(code);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                login.kill();
                applog::info("rifai login: tempo scaduto");
                return LoginEnd::TimedOut;
            }
            match commands.recv_timeout(left.min(POLL)) {
                Ok(ReloginCommand::Code(code)) => {
                    if !claude::valid_code(&code) {
                        progress(ReloginEvent::CodeInvalid);
                    } else if login.send_code(&code) {
                        applog::info("rifai login: codice inviato");
                        progress(ReloginEvent::CodeSent);
                    }
                }
                Ok(ReloginCommand::Cancel) | Err(RecvTimeoutError::Disconnected) => {
                    login.kill();
                    applog::info("rifai login: annullato");
                    return LoginEnd::Cancelled;
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// Dopo un login non completato: se le credenziali di Claude Code non sono più quelle di P
    /// (codice già inviato prima di annullare) o non si possono leggere, P si riscrive dal
    /// backup appena salvato (`switch P --force`).
    fn restore_if_changed(
        &self,
        previous_email: Option<&str>,
        outcome: &mut ReloginOutcome,
        progress: &mut dyn FnMut(ReloginEvent),
    ) {
        let Some(p) = outcome.previous else {
            return;
        };
        let still_p = match self.client.status() {
            Ok(s) => s
                .active
                .is_some_and(|a| previous_email.is_some_and(|pe| a.email.eq_ignore_ascii_case(pe))),
            Err(e) => {
                applog::warn(&format!(
                    "rifai login: account collegato non verificato, ritorno forzato ({})",
                    e.log_summary()
                ));
                false
            }
        };
        if !still_p {
            let back = self.go_back(p, true, progress);
            apply_back(outcome, p, back);
        }
    }

    /// `cswap switch P [--force]`: fatto se cswap dice di aver cambiato o che P è già attiva
    /// (`already-active`, oppure `activated` dopo `--force` sullo stesso slot).
    fn go_back(&self, p: u32, force: bool, progress: &mut dyn FnMut(ReloginEvent)) -> Back {
        progress(ReloginEvent::StepStarted {
            step: ReloginStep::RestoreActive,
        });
        match self.client.switch(Some(p), None, None, force) {
            Ok(o) => {
                let done = o.switch.as_ref().is_some_and(|s| {
                    s.switched
                        || matches!(
                            s.reason,
                            SwitchReason::AlreadyActive | SwitchReason::Activated
                        )
                });
                let note = if done {
                    o.warning
                } else {
                    o.switch.map(|s| s.message).filter(|m| !m.is_empty())
                };
                Back {
                    done,
                    list: o.list,
                    note,
                }
            }
            Err(e) => Back {
                done: false,
                list: e.list.map(|l| *l),
                note: Some(e.message),
            },
        }
    }

    /// Errore dopo un login riuscito e prima di `add`: si torna a P e l'errore dice com'è andata.
    fn fail_back(
        &self,
        previous: Option<u32>,
        mut err: CswapError,
        progress: &mut dyn FnMut(ReloginEvent),
    ) -> CswapError {
        let Some(p) = previous else {
            return err;
        };
        let back = self.go_back(p, true, progress);
        let list = back.list.or_else(|| self.client.list().ok());
        if back.done && is_active(list.as_ref(), p) {
            err.message = format!("{}; è di nuovo attiva l'utenza {p}", err.message);
        } else {
            err.message = format!("{}. {}", err.message, not_restored(p, back.note.as_deref()));
        }
        if let (Some(l), true) = (list, err.list.is_some()) {
            err.list = Some(Box::new(l));
        }
        err
    }

    /// Elenco finale (riletto se manca) e conferma: `restored` solo con P attiva nell'elenco.
    fn finish(&self, outcome: &mut ReloginOutcome) {
        if outcome.list.is_none() {
            outcome.list = self.client.list().ok();
        }
        let Some(p) = outcome.previous else {
            outcome.restored = false;
            return;
        };
        if outcome.restored && !is_active(outcome.list.as_ref(), p) {
            outcome.restored = false;
            outcome.warning = Some(not_restored(p, outcome.warning.as_deref()));
        }
    }
}

/// Esito di [`Relogin::go_back`]: `note` è l'avviso di cswap, o la causa se non è fatto.
struct Back {
    done: bool,
    list: Option<ListResponse>,
    note: Option<String>,
}

fn apply_back(outcome: &mut ReloginOutcome, p: u32, back: Back) {
    if back.list.is_some() {
        outcome.list = back.list;
    }
    if !back.done {
        outcome.restored = false;
        outcome.warning = Some(not_restored(p, back.note.as_deref()));
    } else if back.note.is_some() {
        outcome.warning = back.note;
    }
}

fn is_active(list: Option<&ListResponse>, p: u32) -> bool {
    list.is_some_and(|l| l.active_account_number == Some(p))
}

/// Avviso visibile quando il ritorno a P non è confermato.
fn not_restored(p: u32, cause: Option<&str>) -> String {
    let base = format!("Non è stato possibile tornare all'utenza {p}: controlla con cswap list");
    match cause {
        Some(c) if !c.is_empty() => format!("{base} ({c})"),
        _ => base,
    }
}

/// Testo del rifiuto di [`refuse_unmanaged_login`].
pub const UNMANAGED_LOGIN_MESSAGE: &str = "In Claude Code è collegato un account non registrato \
     in cswap: registralo con Aggiungi account o scollegalo, poi riprova";

/// Senza un'utenza attiva gestita: con un login non registrato in cswap il «Rifai login» lo
/// sostituirebbe senza poterlo ripristinare, quindi si rifiuta prima di avviare il login.
fn refuse_unmanaged_login(status: &StatusResponse) -> Res<()> {
    match &status.active {
        Some(a) if !a.managed => Err(CswapError::new(
            CswapErrorKind::Failed,
            UNMANAGED_LOGIN_MESSAGE,
        )),
        _ => Ok(()),
    }
}

/// Errore con il passo in testa, stessa classe (e stesso elenco riletto per `uncertain`).
fn context(step: &str, mut e: CswapError) -> CswapError {
    e.message = format!("{step}: {}", e.message);
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeutil::format_iso;

    fn account(status: UsageStatus, expires: Option<i64>) -> Account {
        let list: ListResponse =
            serde_json::from_str(include_str!("../../../fixtures/cswap/list.json")).unwrap();
        let mut a = list.accounts[0].clone();
        a.usage_status = status;
        a.login_expires_at = expires.map(format_iso);
        a
    }

    #[test]
    fn needs_relogin_uses_status_and_expiry() {
        let now = 1_800_000_000;
        assert!(needs_relogin(
            &account(UsageStatus::ReloginRequired, None),
            3,
            now
        ));
        assert!(needs_relogin(
            &account(UsageStatus::TokenExpired, None),
            3,
            now
        ));
        assert!(!needs_relogin(&account(UsageStatus::Ok, None), 3, now));
        assert!(needs_relogin(
            &account(UsageStatus::Ok, Some(now + 2 * 86_400)),
            3,
            now
        ));
        assert!(needs_relogin(
            &account(UsageStatus::Ok, Some(now - 10)),
            3,
            now
        ));
        assert!(!needs_relogin(
            &account(UsageStatus::Ok, Some(now + 4 * 86_400)),
            3,
            now
        ));
    }

    #[test]
    fn one_run_at_a_time() {
        let first = RunGuard::take().unwrap();
        assert_eq!(RunGuard::take().unwrap_err().kind, CswapErrorKind::Busy);
        drop(first);
        drop(RunGuard::take().unwrap());
    }
}
