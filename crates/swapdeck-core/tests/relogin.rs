//! F21 «Rifai login» con il finto cswap e il finto Claude Code (stesso binario di test,
//! `tests/support/fake_cswap.rs`, chiamato `claude`): niente cswap né claude reali.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

use serde_json::{Value, json};
use swapdeck_core::cswap::{CswapClient, CswapErrorKind, SecretToken, Timeouts};
use swapdeck_core::relogin::{
    Relogin, ReloginCommand, ReloginEvent, ReloginOutcome, ReloginResult, ReloginStep,
};
use tempfile::TempDir;

struct Env {
    dir: TempDir,
    cswap: PathBuf,
    claude: PathBuf,
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cswap")
}

fn link(dir: &Path, base: &str) -> PathBuf {
    let p = dir.join(format!("{base}{}", std::env::consts::EXE_SUFFIX));
    let src = env!("CARGO_BIN_EXE_fake-cswap");
    if fs::hard_link(src, &p).is_err() {
        fs::copy(src, &p).unwrap();
    }
    p
}

impl Env {
    /// Stato `list.json` (attiva 6), login simulato di Claude Code su user6, finto claude con `claude`.
    fn new(claude: Value) -> Self {
        Self::with_overrides(claude, json!({}))
    }

    /// Come [`Env::new`], con le sostituzioni del finto cswap (`overrides`).
    fn with_overrides(claude: Value, overrides: Value) -> Self {
        let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let cfg = json!({
            "fixtures": fixtures(),
            "state": dir.path().join("state.json"),
            "log": dir.path().join("log.jsonl"),
            "claude": claude,
            "overrides": overrides,
        });
        fs::write(dir.path().join("fake.json"), cfg.to_string()).unwrap();
        fs::copy(fixtures().join("list.json"), dir.path().join("state.json")).unwrap();
        fs::write(
            dir.path().join("state.login.json"),
            json!({"email": "user6@example.com"}).to_string(),
        )
        .unwrap();
        let cswap = link(dir.path(), "cswap");
        let claude = link(dir.path(), "claude");
        Self { dir, cswap, claude }
    }

    fn relogin(&self) -> Relogin {
        let client = CswapClient::new(&self.cswap).with_timeouts(Timeouts {
            data: Duration::from_secs(20),
            write: Duration::from_secs(20),
            other: Duration::from_secs(20),
            upgrade: Duration::from_secs(20),
        });
        Relogin::new(
            client,
            Some(self.dir.path().join("data")),
            Some(self.claude.clone()),
        )
    }

    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.dir.path().join("log.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    /// Sottocomandi di cswap chiamati, in ordine (`auth` = claude).
    fn subcommands(&self) -> Vec<String> {
        self.calls()
            .iter()
            .map(|c| {
                let args: Vec<&str> = c["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap())
                    .collect();
                match args.as_slice() {
                    ["switch", n, "--force", ..] => format!("switch {n} --force"),
                    ["switch", n, ..] => format!("switch {n}"),
                    [first, ..] => (*first).to_owned(),
                    [] => String::new(),
                }
            })
            .collect()
    }

    /// Nessuna utenza attiva in cswap; login di Claude Code `login` (`None` = nessuno).
    fn without_active(&self, login: Option<&str>) {
        let path = self.dir.path().join("state.json");
        let mut state: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        state["activeAccountNumber"] = Value::Null;
        for a in state["accounts"].as_array_mut().unwrap() {
            a["active"] = json!(false);
        }
        fs::write(&path, state.to_string()).unwrap();
        let login_path = self.dir.path().join("state.login.json");
        match login {
            Some(e) => fs::write(login_path, json!({ "email": e }).to_string()).unwrap(),
            None => fs::remove_file(login_path).unwrap(),
        }
    }

    fn login(&self) -> String {
        let text = fs::read_to_string(self.dir.path().join("state.login.json")).unwrap();
        serde_json::from_str::<Value>(&text).unwrap()["email"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

/// Una sola operazione per processo: i test del flusso girano uno alla volta.
static SERIAL: Mutex<()> = Mutex::new(());

type Run = (
    Result<ReloginOutcome, swapdeck_core::cswap::CswapError>,
    Vec<ReloginEvent>,
);

/// Esegue il flusso: alla comparsa del link invia i comandi `on_link`; poi il canale resta
/// aperto (senza comandi si arriva al tempo massimo).
fn run(relogin: &Relogin, account: u32, on_link: Vec<ReloginCommand>) -> Run {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (tx, rx) = mpsc::channel();
    let mut events = Vec::new();
    let mut pending = on_link;
    let result = relogin.run(account, &rx, &mut |e| {
        if matches!(e, ReloginEvent::Link { .. }) {
            for cmd in pending.drain(..) {
                tx.send(cmd).unwrap();
            }
        }
        events.push(e);
    });
    (result, events)
}

fn code(text: &str) -> Vec<ReloginCommand> {
    vec![ReloginCommand::Code(SecretToken::new(text))]
}

fn steps(events: &[ReloginEvent]) -> Vec<ReloginStep> {
    events
        .iter()
        .filter_map(|e| match e {
            ReloginEvent::StepStarted { step } => Some(*step),
            _ => None,
        })
        .collect()
}

#[test]
fn right_email_updates_and_returns_to_previous() {
    let env = Env::new(json!({}));
    let (result, events) = run(&env.relogin(), 2, code("good-code"));
    let o = result.unwrap();
    assert_eq!(o.result, ReloginResult::Updated);
    assert_eq!((o.previous, o.restored), (Some(6), true));
    assert_eq!(o.list.unwrap().active_account_number, Some(6));
    assert_eq!(
        steps(&events),
        [
            ReloginStep::SaveActive,
            ReloginStep::StartLogin,
            ReloginStep::WaitCode,
            ReloginStep::Verify,
            ReloginStep::SaveAccount,
            ReloginStep::RestoreActive
        ]
    );
    assert!(events.contains(&ReloginEvent::CodeSent));
    assert!(events.iter().any(
        |e| matches!(e, ReloginEvent::Link { url, timeout_seconds: 300 }
        if url.starts_with("https://claude.ai/oauth/authorize"))
    ));
    assert_eq!(
        env.subcommands(),
        [
            "list", "add", "list", "auth", "status", "add", "list", "switch 6", "list"
        ]
    );
    // Il codice passa da stdin, mai negli argomenti.
    let claude = env
        .calls()
        .into_iter()
        .find(|c| c["args"][0] == "auth")
        .unwrap();
    assert_eq!(claude["args"][4], "user2@example.com");
    assert!(!claude["args"].to_string().contains("good-code"));
    assert_eq!(claude["stdin"], json!(["good-code"]));
    assert_eq!(
        claude["browser"], "none",
        "nessuna scheda del browser sul PC"
    );
    assert_eq!(env.login(), "user6@example.com");
    // Utenza aggiornata sul posto (nessuna riga nuova).
    let state: Value =
        serde_json::from_str(&fs::read_to_string(env.dir.path().join("state.json")).unwrap())
            .unwrap();
    assert_eq!(state["accounts"].as_array().unwrap().len(), 5);
    assert!(
        !env.dir.path().join("data").join("engine.lock").exists(),
        "lock rilasciato"
    );
}

#[test]
fn url_on_stderr_is_found_too() {
    let env = Env::new(json!({"urlStream": "stderr"}));
    let (result, _) = run(&env.relogin(), 2, code("good-code"));
    assert_eq!(result.unwrap().result, ReloginResult::Updated);
}

#[test]
fn other_email_restores_previous_without_add() {
    let env = Env::new(json!({"loginEmail": "intruso@example.com"}));
    let (result, _) = run(&env.relogin(), 2, code("good-code"));
    let o = result.unwrap();
    assert_eq!(o.result, ReloginResult::WrongAccount);
    assert!(o.restored);
    assert_eq!(
        env.subcommands(),
        [
            "list",
            "add",
            "list",
            "auth",
            "status",
            "switch 6 --force",
            "list"
        ]
    );
    assert_eq!(env.login(), "user6@example.com");
}

#[test]
fn cancel_kills_login_and_keeps_credentials() {
    let env = Env::new(json!({"mode": "hang"}));
    let (result, _) = run(&env.relogin(), 2, vec![ReloginCommand::Cancel]);
    let o = result.unwrap();
    assert_eq!((o.result, o.restored), (ReloginResult::Cancelled, true));
    // Il finto claude chiuso non registra la chiamata.
    assert_eq!(env.subcommands(), ["list", "add", "list", "status", "list"]);
    assert_eq!(env.login(), "user6@example.com");
}

#[test]
fn timeout_kills_login() {
    let env = Env::new(json!({"mode": "hang"}));
    let relogin = env.relogin().with_login_timeout(Duration::from_secs(2));
    let (result, _) = run(&relogin, 2, Vec::new());
    assert_eq!(result.unwrap().result, ReloginResult::TimedOut);
}

#[test]
fn login_error_and_invalid_code() {
    let env = Env::new(json!({"mode": "fail"}));
    let (result, _) = run(&env.relogin(), 2, Vec::new());
    let o = result.unwrap();
    assert_eq!(o.result, ReloginResult::LoginFailed);
    let detail = o.detail.unwrap();
    assert!(detail.contains("OAuth error"), "{detail}");
    assert!(!detail.contains("https://"));

    let env = Env::new(json!({}));
    let mut cmds = code("bad code");
    cmds.push(ReloginCommand::Cancel);
    let (result, events) = run(&env.relogin(), 2, cmds);
    // Codice con uno spazio: non inviato; poi annulla.
    assert!(events.contains(&ReloginEvent::CodeInvalid));
    assert_eq!(result.unwrap().result, ReloginResult::Cancelled);
}

#[test]
fn missing_claude_and_unknown_account() {
    let env = Env::new(json!({}));
    let client = CswapClient::new(&env.cswap);
    let relogin = Relogin::new(client, None, None);
    let (result, _) = run(&relogin, 2, Vec::new());
    assert_eq!(result.unwrap().result, ReloginResult::ClaudeNotFound);
    assert_eq!(
        env.subcommands(),
        ["list"],
        "nessuna scrittura senza Claude Code"
    );
    let (result, _) = run(&env.relogin(), 99, Vec::new());
    assert_eq!(result.unwrap_err().kind, CswapErrorKind::InvalidUsage);
}

#[test]
fn engine_lock_of_another_process_is_busy() {
    let env = Env::new(json!({}));
    let data = env.dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let lock = json!({
        "pid": 1, "host": "altro-host", "instance": "x", "processStart": null, "acquiredAt": 4_000_000_000_i64,
        "heartbeatAt": 4_000_000_000_i64, "intervalSecs": 60
    });
    fs::write(data.join("engine.lock"), lock.to_string()).unwrap();
    let (result, _) = run(&env.relogin(), 2, code("good-code"));
    assert_eq!(result.unwrap_err().kind, CswapErrorKind::Busy);
    assert!(env.calls().is_empty(), "nessuna chiamata a cswap");
}

#[test]
fn add_failing_after_login_returns_to_previous() {
    let env = Env::with_overrides(
        json!({}),
        json!({"add": {"fromCall": 2, "exit": 1, "stderr": "Error: boom\n"}}),
    );
    let (result, _) = run(&env.relogin(), 2, code("good-code"));
    let e = result.unwrap_err();
    assert!(
        e.message.starts_with("login non salvato, rifallo"),
        "{}",
        e.message
    );
    assert!(
        e.message.contains("è di nuovo attiva l'utenza 6"),
        "{}",
        e.message
    );
    assert_eq!(
        env.subcommands(),
        [
            "list",
            "add",
            "list",
            "auth",
            "status",
            "add",
            "switch 6 --force",
            "list"
        ]
    );
    assert_eq!(env.login(), "user6@example.com");
}

#[test]
fn status_failing_after_login_returns_to_previous() {
    let env = Env::with_overrides(
        json!({}),
        json!({"status": {"exit": 1, "stderr": "Error: boom\n"}}),
    );
    let (result, _) = run(&env.relogin(), 2, code("good-code"));
    let e = result.unwrap_err();
    assert!(
        e.message.starts_with("verifica dell'account collegato"),
        "{}",
        e.message
    );
    assert!(
        e.message.contains("è di nuovo attiva l'utenza 6"),
        "{}",
        e.message
    );
    assert_eq!(
        env.subcommands(),
        [
            "list",
            "add",
            "list",
            "auth",
            "status",
            "switch 6 --force",
            "list"
        ]
    );
    assert_eq!(env.login(), "user6@example.com");
}

#[test]
fn unmanaged_login_without_active_is_refused() {
    let env = Env::new(json!({}));
    env.without_active(Some("estraneo@example.com"));
    let (result, _) = run(&env.relogin(), 2, code("good-code"));
    let e = result.unwrap_err();
    assert_eq!(e.message, swapdeck_core::relogin::UNMANAGED_LOGIN_MESSAGE);
    assert_eq!(env.subcommands(), ["list", "status"], "login non avviato");
    assert_eq!(env.login(), "estraneo@example.com");
}

#[test]
fn no_login_without_active_proceeds_without_return() {
    let env = Env::new(json!({}));
    env.without_active(None);
    let (result, events) = run(&env.relogin(), 2, code("good-code"));
    let o = result.unwrap();
    assert_eq!(o.result, ReloginResult::Updated);
    assert_eq!((o.previous, o.restored), (None, false));
    assert!(!steps(&events).contains(&ReloginStep::RestoreActive));
    assert_eq!(
        env.subcommands(),
        ["list", "status", "auth", "status", "add", "list"]
    );
}

#[test]
fn return_not_done_or_not_confirmed_is_not_restored() {
    // cswap non cambia (`switched:false`, motivo non accettato).
    let env = Env::with_overrides(
        json!({}),
        json!({"switch": {"exit": 0, "stdoutFixture": "switch-unmanaged.json"}}),
    );
    let (result, _) = run(&env.relogin(), 2, code("good-code"));
    let o = result.unwrap();
    assert_eq!((o.result, o.restored), (ReloginResult::Updated, false));
    let w = o.warning.unwrap();
    assert!(
        w.starts_with("Non è stato possibile tornare all'utenza 6: controlla con cswap list"),
        "{w}"
    );

    // cswap dice di aver cambiato, ma l'elenco riletto ha un'altra utenza attiva.
    let env = Env::with_overrides(
        json!({}),
        json!({"switch": {"exit": 0, "stdoutFixture": "switch-switched.json"}}),
    );
    let (result, _) = run(&env.relogin(), 2, code("good-code"));
    let o = result.unwrap();
    assert_eq!(o.list.as_ref().unwrap().active_account_number, Some(2));
    assert!(!o.restored);
    assert!(o.warning.unwrap().contains("tornare all'utenza 6"));
}

#[test]
fn cancel_with_unreadable_status_forces_return() {
    let env = Env::with_overrides(
        json!({"mode": "hang"}),
        json!({
            "status": {"exit": 1, "stderr": "Error: boom\n"},
            "switch": {"exit": 0, "stdoutFixture": "switch-unmanaged.json"}
        }),
    );
    let (result, _) = run(&env.relogin(), 2, vec![ReloginCommand::Cancel]);
    let o = result.unwrap();
    assert_eq!((o.result, o.restored), (ReloginResult::Cancelled, false));
    assert!(o.warning.unwrap().contains("tornare all'utenza 6"));
    assert_eq!(
        env.subcommands(),
        ["list", "add", "list", "status", "switch 6 --force", "list"]
    );
}
