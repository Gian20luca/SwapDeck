//! Test di integrazione del client con il finto cswap (`tests/support/fake_cswap.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use swapdeck_core::cswap::{
    AUTO_IDLE_WAIT, AutoEvent, AutoMessage, AutoOptions, CswapAction, CswapClient, CswapErrorKind,
    RunOptions, SecretToken, SwitchReason, SwitchStrategy, Timeouts,
};
use tempfile::TempDir;

struct Fake {
    dir: TempDir,
    client: CswapClient,
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cswap")
}

fn exe(dir: &Path, base: &str) -> PathBuf {
    let p = dir.join(format!("{base}{}", std::env::consts::EXE_SUFFIX));
    // Collegamento fisico: niente nuova scansione antivirus a ogni test (fallback: copia).
    let src = env!("CARGO_BIN_EXE_fake-cswap");
    if fs::hard_link(src, &p).is_err() {
        fs::copy(src, &p).unwrap();
    }
    p
}

impl Fake {
    /// Finto cswap con lo stato iniziale di `state` (fixture) e la config `extra`.
    fn new(state: Option<&str>, extra: Value) -> Self {
        // Nella cartella temporanea del target: stesso file system dell'eseguibile, quindi
        // collegamento fisico e niente copia (su Linux una copia appena scritta può dare
        // ETXTBSY se un altro thread del test fa fork mentre il file è aperto).
        let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let program = exe(dir.path(), "cswap");
        let mut cfg = json!({
            "fixtures": fixtures(),
            "state": dir.path().join("state.json"),
            "log": dir.path().join("log.jsonl"),
        });
        for (k, v) in extra.as_object().unwrap() {
            cfg[k] = v.clone();
        }
        fs::write(dir.path().join("fake.json"), cfg.to_string()).unwrap();
        if let Some(s) = state {
            fs::copy(fixtures().join(s), dir.path().join("state.json")).unwrap();
        }
        let client = CswapClient::new(program).with_timeouts(Timeouts {
            data: Duration::from_secs(20),
            write: Duration::from_secs(20),
            other: Duration::from_secs(20),
            upgrade: Duration::from_secs(20),
        });
        Self { dir, client }
    }

    fn std() -> Self {
        Self::new(Some("list.json"), json!({}))
    }

    fn calls(&self) -> Vec<Value> {
        let text = fs::read_to_string(self.dir.path().join("log.jsonl")).unwrap_or_default();
        text.lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn call(&self, sub: &str) -> Value {
        self.calls()
            .into_iter()
            .find(|c| c["args"][0] == sub)
            .unwrap_or_else(|| panic!("nessuna chiamata {sub}"))
    }

    fn timeouts(mut self, t: Duration) -> Self {
        self.client = self.client.clone().with_timeouts(Timeouts {
            data: t,
            write: t,
            other: t,
            upgrade: t,
        });
        self
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        // Regola del contratto: mai cswap senza sottocomando, sempre NO_COLOR.
        if std::thread::panicking() {
            return;
        }
        for c in self.calls() {
            assert!(
                !c["args"].as_array().unwrap().is_empty(),
                "cswap senza argomenti"
            );
            assert_eq!(c["noColor"], true);
            assert_eq!(c["secureStorageEnv"], false);
        }
    }
}

fn args(c: &Value) -> Vec<String> {
    c["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap().to_owned())
        .collect()
}

// ---- lettura ----------------------------------------------------------------

#[test]
fn version_checks() {
    let f = Fake::std();
    assert_eq!(f.client.check_version().unwrap().to_string(), "0.27.2");
    let old = Fake::new(None, json!({"version": "cswap 0.25.3"}));
    assert_eq!(
        old.client.check_version().unwrap_err().kind,
        CswapErrorKind::UnsupportedVersion
    );
    let bad = Fake::new(None, json!({"version": "boh"}));
    assert_eq!(
        bad.client.version().unwrap_err().kind,
        CswapErrorKind::UnsupportedVersion
    );
}

#[test]
fn list_and_status() {
    let f = Fake::std();
    let l = f.client.list().unwrap();
    assert_eq!(l.accounts.len(), 5);
    assert_eq!(args(&f.call("list")), ["list", "--json"]);
    let s = f.client.status().unwrap();
    assert_eq!(s.active.unwrap().number, Some(6));
    assert_eq!(s.total_managed_accounts, Some(5));
    let empty = Fake::new(None, json!({}));
    assert!(empty.client.list().unwrap().accounts.is_empty());
    assert!(empty.client.status().unwrap().active.is_none());
}

#[test]
fn text_and_config_reads() {
    let f = Fake::std();
    assert!(f.client.aliases().unwrap().starts_with("Aliases:"));
    assert!(
        f.client
            .maps()
            .unwrap()
            .starts_with("No directory mappings yet.")
    );
    assert!(f.client.mappings().unwrap().is_empty());
    assert_eq!(
        f.client.unclaimed().unwrap(),
        "No unclaimed credential entries"
    );
    assert_eq!(f.client.config_list().unwrap().settings.len(), 9);
    assert_eq!(
        f.client.config_get("autoswitch.threshold").unwrap().key,
        "autoswitch.threshold"
    );
    let e = f.client.config_get("nope").unwrap_err();
    assert_eq!(e.error_type.as_deref(), Some("ConfigError"));
    assert!(f.client.config_path().unwrap().ends_with("settings.json"));
}

// ---- switch -----------------------------------------------------------------

#[test]
fn switch_direct_is_verified() {
    let f = Fake::std();
    let o = f.client.switch(Some(1), None, None, false).unwrap();
    assert!(o.switch.as_ref().unwrap().switched);
    assert_eq!(o.list.unwrap().active_account_number, Some(1));
    assert_eq!(args(&f.call("switch")), ["switch", "1", "--json"]);

    let o = f.client.switch(Some(1), None, None, false).unwrap();
    let s = o.switch.unwrap();
    assert!(!s.switched);
    assert_eq!(s.reason, SwitchReason::AlreadyActive);
}

#[test]
fn switch_with_strategy_and_models() {
    let f = Fake::std();
    let models = vec!["Fable".to_owned(), "all".to_owned()];
    let o = f
        .client
        .switch(None, Some(SwitchStrategy::Best), Some(&models), false)
        .unwrap();
    assert_eq!(o.switch.unwrap().reason, SwitchReason::AlreadyBest);
    assert_eq!(
        args(&f.call("switch")),
        [
            "switch",
            "--strategy",
            "best",
            "--model",
            "Fable,all",
            "--json"
        ]
    );
    let o = f
        .client
        .switch(None, Some(SwitchStrategy::NextAvailable), None, false)
        .unwrap();
    assert!(o.switch.unwrap().switched);
    // Combinazioni rifiutate prima di lanciare cswap.
    for e in [
        f.client
            .switch(Some(1), Some(SwitchStrategy::Best), None, false),
        f.client.switch(None, None, Some(&models), false),
        f.client.switch(None, None, None, true),
        f.client.switch(Some(0), None, None, false),
    ] {
        assert_eq!(e.unwrap_err().kind, CswapErrorKind::InvalidUsage);
    }
}

#[test]
fn switch_errors_and_unconfirmed() {
    let f = Fake::std();
    let e = f.client.switch(Some(9), None, None, false).unwrap_err();
    assert_eq!(e.error_type.as_deref(), Some("AccountNotFoundError"));
    assert_eq!(e.exit_code, Some(1));
    let empty = Fake::new(None, json!({}));
    let e = empty.client.switch(None, None, None, false).unwrap_err();
    assert_eq!(e.message, "No accounts are managed yet");
    // Cambio dichiarato ma non visto in list: esito di cswap con avviso, non un errore.
    let liar = Fake::new(Some("list.json"), json!({"ignoreSwitch": true}));
    let o = liar.client.switch(Some(1), None, None, false).unwrap();
    assert!(o.switch.unwrap().switched);
    assert!(o.list.is_some());
    assert!(o.warning.unwrap().contains("non confermato"));
    // Rilettura fallita: stesso esito, senza lista.
    let blind = Fake::new(
        Some("list.json"),
        json!({"overrides": {"list": {"exit": 1, "stderr": "Error: boom\n"}}}),
    );
    let o = blind.client.switch(Some(1), None, None, false).unwrap();
    assert!(o.switch.unwrap().switched);
    assert!(o.list.is_none());
    assert!(o.warning.unwrap().contains("non verificato"));
    let ok = Fake::std()
        .client
        .switch(Some(1), None, None, false)
        .unwrap();
    assert!(ok.warning.is_none());
}

#[test]
fn write_timeout_is_uncertain_with_list() {
    let f = Fake::new(
        Some("list.json"),
        json!({"overrides": {"alias": {"sleep": 60, "exit": 0}}}),
    );
    let client = f.client.clone().with_timeouts(Timeouts {
        write: Duration::from_secs(3),
        ..Timeouts::default()
    });
    let e = client.set_alias(1, "lavoro").unwrap_err();
    assert_eq!(e.kind, CswapErrorKind::Uncertain);
    assert!(e.list.unwrap().accounts.iter().any(|a| a.number == 1));
    assert_eq!(Timeouts::default().write, Duration::from_secs(30));
}

// ---- add, add-token, remove, purge (prompt su stdin) ------------------------

#[test]
fn add_answers_overwrite_prompt() {
    let f = Fake::std();
    let e = f.client.add(Some(1), None, false).unwrap_err();
    assert_eq!(e.kind, CswapErrorKind::Cancelled);
    assert_eq!(f.call("add")["stdin"], json!(["n"]));
    let o = f.client.add(Some(1), Some("nuovo"), true).unwrap();
    let a = o
        .list
        .unwrap()
        .accounts
        .into_iter()
        .find(|a| a.number == 1)
        .unwrap();
    assert_eq!(a.alias.as_deref(), Some("nuovo"));
    let o = f.client.add(None, None, false).unwrap();
    assert!(o.message.contains("Account-7"));
    let nologin = Fake::new(Some("list.json"), json!({"loggedIn": false}));
    assert_eq!(
        nologin.client.add(None, None, false).unwrap_err().kind,
        CswapErrorKind::ReloginRequired
    );
}

#[test]
fn add_token_uses_stdin_only() {
    let f = Fake::std();
    let token = SecretToken::new("sk-ant-oat01-FINTO-TOKEN-1234567890");
    let o = f.client.add_token(&token, None, Some(4), false).unwrap();
    assert!(o.message.contains("setup-token-4@token.local"));
    let c = f.call("add-token");
    assert_eq!(args(&c), ["add-token", "-", "--slot", "4"]);
    assert_eq!(c["stdin"][0], "sk-ant-oat01-FINTO-TOKEN-1234567890");
    // Slot occupato senza conferma: annullato.
    let e = f
        .client
        .add_token(&token, Some("x@example.com"), Some(1), false)
        .unwrap_err();
    assert_eq!(e.kind, CswapErrorKind::Cancelled);
    assert!(!format!("{e:?}").contains("FINTO"));
    // Token vuoto o multiriga: rifiutati prima di lanciare cswap.
    assert_eq!(
        f.client
            .add_token(&SecretToken::new(" "), None, None, false)
            .unwrap_err()
            .kind,
        CswapErrorKind::InvalidUsage
    );
    assert!(
        f.client
            .add_token(&SecretToken::new("a\nb"), None, None, false)
            .is_err()
    );
}

#[test]
fn add_token_error_never_leaks_token() {
    let f = Fake::new(
        Some("list.json"),
        json!({"overrides": {"add-token": {"exit": 1,
            "stderr": "Error: rejected sk-ant-oat01-SEGRETISSIMO for mario.rossi@example.com\n"}}}),
    );
    let e = f
        .client
        .add_token(
            &SecretToken::new("sk-ant-oat01-SEGRETISSIMO"),
            None,
            None,
            false,
        )
        .unwrap_err();
    let shown = format!("{e} {e:?} {}", serde_json::to_string(&e).unwrap());
    assert!(!shown.contains("SEGRETISSIMO"), "{shown}");
    assert!(!shown.contains("mario.rossi@"), "{shown}");
}

#[test]
fn remove_is_confirmed_and_verified() {
    let f = Fake::std();
    let o = f.client.remove(2).unwrap();
    assert!(o.list.unwrap().accounts.iter().all(|a| a.number != 2));
    assert_eq!(f.call("remove")["stdin"], json!(["y"]));
    assert_eq!(f.client.remove(2).unwrap_err().kind, CswapErrorKind::Failed);
    let liar = Fake::new(Some("list.json"), json!({"ignoreRemove": true}));
    assert_eq!(
        liar.client.remove(1).unwrap_err().kind,
        CswapErrorKind::Cancelled
    );
}

#[test]
fn purge_is_confirmed_and_verified() {
    let f = Fake::std();
    let o = f.client.purge().unwrap();
    assert!(o.list.unwrap().accounts.is_empty());
    assert_eq!(f.call("purge")["stdin"], json!(["y"]));
}

// ---- altre azioni -----------------------------------------------------------

#[test]
fn disable_enable_alias_swap_move() {
    let f = Fake::std();
    let o = f.client.disable(3).unwrap();
    assert!(
        o.list
            .unwrap()
            .accounts
            .iter()
            .any(|a| a.number == 3 && a.is_disabled())
    );
    let o = f.client.enable(3).unwrap();
    assert!(o.list.unwrap().accounts.iter().all(|a| !a.is_disabled()));
    assert_eq!(
        f.client.disable(9).unwrap_err().kind,
        CswapErrorKind::Failed
    );

    let o = f.client.set_alias(1, "lavoro").unwrap();
    assert!(
        o.list
            .unwrap()
            .accounts
            .iter()
            .any(|a| a.alias.as_deref() == Some("lavoro"))
    );
    let o = f.client.unset_alias(1).unwrap();
    assert!(
        o.list
            .unwrap()
            .accounts
            .iter()
            .any(|a| a.number == 1 && a.alias.is_none())
    );
    assert_eq!(
        f.client.set_alias(1, "--force").unwrap_err().kind,
        CswapErrorKind::InvalidUsage
    );

    let o = f.client.swap(1, 2).unwrap();
    assert!(o.message.contains("Swapped"));
    assert_eq!(
        f.client.swap(1, 1).unwrap_err().kind,
        CswapErrorKind::InvalidUsage
    );
    let o = f.client.move_account(5, 4).unwrap();
    assert!(o.list.unwrap().accounts.iter().any(|a| a.number == 4));
}

#[test]
fn map_config_export_import_unclaimed() {
    let f = Fake::std();
    let work = f.dir.path().join("work");
    let work = work.to_str().unwrap();
    assert!(
        f.client
            .map(1, work)
            .unwrap()
            .message
            .starts_with(&format!("Mapped {work}"))
    );
    let maps = f.client.mappings().unwrap();
    assert_eq!(maps.len(), 1);
    assert_eq!((maps[0].path.as_str(), maps[0].account), (work, Some(1)));
    assert!(
        f.client
            .unmap(work)
            .unwrap()
            .message
            .starts_with("Unmapped")
    );
    assert!(f.client.mappings().unwrap().is_empty());
    assert!(
        f.client
            .unmap(work)
            .unwrap()
            .message
            .starts_with("No mapping")
    );
    // Percorsi relativi rifiutati prima di lanciare cswap.
    for e in [
        f.client.map(1, "work"),
        f.client.unmap("./work"),
        f.client.export("out.json", None, false),
        f.client.import("in.json", false),
    ] {
        assert_eq!(e.unwrap_err().kind, CswapErrorKind::InvalidUsage);
    }
    assert!(!f.calls().iter().any(|c| c["args"][0] == "export"));
    assert_eq!(
        f.client
            .config_set("autoswitch.threshold", "95")
            .unwrap()
            .message,
        "autoswitch.threshold = 95"
    );
    assert!(f.client.config_set("bogus.key", "1").is_err());
    assert!(
        f.client
            .config_unset("autoswitch.threshold")
            .unwrap()
            .message
            .contains("unset")
    );

    let out = f.dir.path().join("export.json");
    f.client
        .export(out.to_str().unwrap(), Some(1), true)
        .unwrap();
    assert!(out.exists());
    assert_eq!(args(&f.call("export"))[2..], ["--account", "1", "--full"]);
    assert!(
        f.client.export("-", None, false).is_err(),
        "mai export su stdout"
    );
    f.client.import(out.to_str().unwrap(), true).unwrap();
    let missing = f.dir.path().join("missing.json");
    assert_eq!(
        f.client
            .import(missing.to_str().unwrap(), false)
            .unwrap_err()
            .kind,
        CswapErrorKind::Failed
    );

    assert_eq!(
        f.client.unclaimed_purge("abc").unwrap().message,
        "Purged abc"
    );
    let e = f.client.unclaimed_purge("zzz").unwrap_err();
    assert_eq!(e.message, "no unclaimed entry zzz");
}

#[test]
fn upgrade_uses_uv() {
    let f = Fake::std();
    assert_eq!(
        f.client.upgrade().unwrap_err().kind,
        CswapErrorKind::NotInstalled
    );
    let uv = exe(f.dir.path(), "uv");
    let client = f.client.clone().with_uv(Some(uv));
    let o = client.upgrade().unwrap();
    assert!(o.message.contains("Updated claude-swap"));
    assert!(o.message.contains("cswap 0.27.2"));
}

#[test]
fn upgrade_errors_are_not_usage_errors() {
    let cases = [
        (
            "error: Failed to upgrade claude-swap\n  Caused by: failed to remove file `cswap.exe`: The process cannot access the file because it is being used by another process. (os error 32)\n",
            CswapErrorKind::Busy,
        ),
        (
            "error: Failed to fetch: `https://pypi.org/simple/claude-swap/`\n",
            CswapErrorKind::Failed,
        ),
    ];
    for (stderr, kind) in cases {
        let f = Fake::new(
            Some("list.json"),
            json!({"overrides": {"uv": {"exit": 2, "stderr": stderr}}}),
        );
        let uv = exe(f.dir.path(), "uv");
        let e = f.client.clone().with_uv(Some(uv)).upgrade().unwrap_err();
        assert_eq!(e.kind, kind, "{}", e.message);
        assert_eq!(e.exit_code, Some(2));
        assert!(e.message.starts_with("Failed to"), "{}", e.message);
    }
}

#[test]
fn run_command_and_execute() {
    let f = Fake::std();
    let spec = f
        .client
        .run_command(&RunOptions {
            account: Some(3),
            require_session: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(spec.program, f.client.program());
    assert_eq!(spec.args, ["run", "3", "--require-session"]);

    let action: CswapAction =
        serde_json::from_str(r#"{"kind":"setAlias","account":1,"alias":null}"#).unwrap();
    f.client.execute(&action).unwrap();
    let action: CswapAction = serde_json::from_str(r#"{"kind":"switch","target":3}"#).unwrap();
    let o = f.client.execute(&action).unwrap();
    assert_eq!(o.list.unwrap().active_account_number, Some(3));
    let action: CswapAction =
        serde_json::from_str(r#"{"kind":"run","options":{"account":2}}"#).unwrap();
    assert!(f.client.execute(&action).unwrap().command.is_some());
    let action: CswapAction = serde_json::from_str(r#"{"kind":"autoStop"}"#).unwrap();
    assert!(f.client.execute(&action).is_err());
}

// ---- auto ---------------------------------------------------------------------

#[test]
fn auto_streams_events_and_stops() {
    let f = Fake::std();
    let mut auto = f
        .client
        .start_auto(&AutoOptions {
            interval_seconds: Some(60.0),
            models: Some(vec!["Fable".into()]),
            include_api_key_accounts: Some(false),
            ..Default::default()
        })
        .unwrap();
    let mut events = Vec::new();
    while events.len() < 6 {
        match auto.events().recv_timeout(Duration::from_secs(20)).unwrap() {
            AutoMessage::Event(e) => events.push(e),
            AutoMessage::Error(e) => panic!("errore inatteso {e}"),
        }
    }
    assert!(matches!(events[0], AutoEvent::Poll { .. }));
    assert!(matches!(events[5], AutoEvent::AllExhausted { .. }));
    assert!(auto.try_exit().is_none(), "auto deve restare attivo");
    // L'ultimo evento è `sleep`: chiusura subito, senza attendere AUTO_IDLE_WAIT.
    let deadline = Instant::now() + Duration::from_secs(20);
    while !matches!(
        auto.events().recv_timeout(Duration::from_secs(20)).unwrap(),
        AutoMessage::Event(AutoEvent::Sleep { .. })
    ) {
        assert!(Instant::now() < deadline);
    }
    let pid = auto.pid().unwrap().to_string();
    let start = Instant::now();
    auto.stop();
    assert!(
        // Sotto AUTO_IDLE_WAIT: nessuna attesa della pausa (taskkill sotto carico è lento).
        start.elapsed() < AUTO_IDLE_WAIT,
        "{:?}",
        start.elapsed()
    );
    assert!(auto.pid().is_none());
    wait_dead(&pid);
    let c = f.call("auto");
    assert_eq!(
        args(&c),
        [
            "auto",
            "--json",
            "--interval",
            "60",
            "--model",
            "Fable",
            "--no-include-api-key-accounts"
        ]
    );
}

#[test]
fn auto_once_exit_codes() {
    let f = Fake::std();
    let mut auto = f
        .client
        .start_auto(&AutoOptions {
            once: true,
            ..Default::default()
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let code = loop {
        if let Some(code) = auto.try_exit() {
            break code;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(code, Some(2));
    assert!(
        auto.pid().is_none(),
        "dopo l'uscita il processo non si tocca più"
    );
    assert_eq!(auto.try_exit(), Some(Some(2)));
    assert!(auto.exit_error(code).is_none());
    assert!(auto.exit_error(Some(3)).is_none());
    assert!(auto.exit_error(Some(1)).is_some());
    auto.stop();
}

fn wait_exit(auto: &mut swapdeck_core::cswap::AutoProcess) -> Option<i32> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(code) = auto.try_exit() {
            return code;
        }
        assert!(Instant::now() < deadline, "auto non è uscito");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn auto_exit_2_is_an_error_without_once_or_with_usage() {
    // Uso errato con --once: stderr di argparse letto tutto prima di classificare.
    let f = Fake::new(
        Some("list.json"),
        json!({"overrides": {"auto": {"exit": 2, "stderr": "usage: cswap auto [-h]\ncswap auto: error: argument --threshold: invalid float value: 'x'\n"}}}),
    );
    let once = AutoOptions {
        once: true,
        ..Default::default()
    };
    let mut auto = f.client.start_auto(&once).unwrap();
    let code = wait_exit(&mut auto);
    let e = auto.exit_error(code).unwrap();
    assert_eq!(e.kind, CswapErrorKind::InvalidUsage);
    assert!(e.message.contains("invalid float value"), "{}", e.message);

    // Senza --once le uscite 2 e 3 sono errori.
    let f = Fake::new(
        Some("list.json"),
        json!({"overrides": {"auto": {"exit": 3}}}),
    );
    let mut auto = f.client.start_auto(&AutoOptions::default()).unwrap();
    let code = wait_exit(&mut auto);
    assert_eq!(code, Some(3));
    assert!(auto.exit_error(code).is_some());
}

#[test]
fn auto_options_are_validated() {
    let f = Fake::std();
    for opts in [
        AutoOptions {
            threshold: Some(49.9),
            ..Default::default()
        },
        AutoOptions {
            threshold: Some(100.0),
            ..Default::default()
        },
        AutoOptions {
            interval_seconds: Some(14.0),
            ..Default::default()
        },
        AutoOptions {
            interval_seconds: Some(3601.0),
            ..Default::default()
        },
        AutoOptions {
            cooldown_seconds: Some(-1.0),
            ..Default::default()
        },
        AutoOptions {
            cooldown_seconds: Some(86401.0),
            ..Default::default()
        },
    ] {
        assert_eq!(
            f.client.start_auto(&opts).err().unwrap().kind,
            CswapErrorKind::InvalidUsage
        );
    }
    assert!(f.calls().is_empty(), "nessun cswap avviato");
    let mut auto = f
        .client
        .start_auto(&AutoOptions {
            once: true,
            threshold: Some(99.9),
            interval_seconds: Some(15.0),
            cooldown_seconds: Some(0.0),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(wait_exit(&mut auto), Some(2));
}

#[test]
fn auto_stop_is_forced_after_idle_wait() {
    let f = Fake::new(Some("list.json"), json!({"autoBusy": true}));
    let mut auto = f.client.start_auto(&AutoOptions::default()).unwrap();
    // Attende l'ultimo evento noto (giro in corso, nessun `sleep`).
    loop {
        if let AutoMessage::Event(AutoEvent::AllExhausted { .. }) =
            auto.events().recv_timeout(Duration::from_secs(20)).unwrap()
        {
            break;
        }
    }
    std::thread::sleep(Duration::from_millis(300));
    let pid = auto.pid().unwrap().to_string();
    let start = Instant::now();
    auto.stop_within(Duration::from_secs(2));
    let took = start.elapsed();
    assert!(
        took >= Duration::from_secs(2) && took < Duration::from_secs(25),
        "{took:?}"
    );
    wait_dead(&pid);
}

fn wait_dead(pid: &str) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while alive(pid) {
        assert!(Instant::now() < deadline, "processo {pid} ancora vivo");
        std::thread::sleep(Duration::from_millis(200));
    }
}

// ---- classi di errore ---------------------------------------------------------

#[test]
fn not_installed() {
    let c = CswapClient::new(std::env::temp_dir().join("nessun-cswap-qui.exe"));
    assert_eq!(c.list().unwrap_err().kind, CswapErrorKind::NotInstalled);
}

#[test]
fn timeout_kills_the_process_tree() {
    let f = Fake::new(
        Some("list.json"),
        json!({"overrides": {"list": {"sleep": 60, "spawnChild": true, "exit": 0}}}),
    )
    .timeouts(Duration::from_secs(8));
    let start = Instant::now();
    let e = f.client.list().unwrap_err();
    assert_eq!(e.kind, CswapErrorKind::Timeout);
    assert!(
        start.elapsed() < Duration::from_secs(40),
        "{:?}",
        start.elapsed()
    );
    let pid = fs::read_to_string(f.dir.path().join("grandchild.pid")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while alive(pid.trim()) {
        assert!(
            Instant::now() < deadline,
            "processo nipote {pid} ancora vivo"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn alive(pid: &str) -> bool {
    if cfg!(windows) {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
    } else if Path::new("/proc/self/stat").exists() {
        // Linux: uno zombie (non ancora raccolto) conta come terminato.
        fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| {
                s.rsplit(") ")
                    .next()
                    .is_some_and(|rest| !rest.starts_with('Z'))
            })
            .unwrap_or(false)
    } else {
        std::process::Command::new("kill")
            .args(["-0", pid])
            .status()
            .unwrap()
            .success()
    }
}

fn failing(sub: &str, over: Value) -> swapdeck_core::cswap::CswapError {
    let f = Fake::new(Some("list.json"), json!({"overrides": {sub: over}}));
    match sub {
        "list" => f.client.list().unwrap_err(),
        "remove" => f.client.remove(1).unwrap_err(),
        "switch" => f.client.switch(Some(1), None, None, false).unwrap_err(),
        _ => unreachable!(),
    }
}

#[test]
fn error_classes() {
    let usage = failing(
        "list",
        json!({"exit": 2, "stderr": "usage: cswap\ncswap: error: --token-status cannot be combined with --json\n"}),
    );
    assert_eq!(usage.kind, CswapErrorKind::InvalidUsage);
    assert_eq!(
        usage.message,
        "--token-status cannot be combined with --json"
    );

    assert_eq!(
        failing("list", json!({"exit": 130})).kind,
        CswapErrorKind::Interrupted
    );

    let tb = failing(
        "remove",
        json!({"exit": 1, "stderr": "Traceback (most recent call last):\n  File \"x\"\nEOFError: EOF when reading a line\n"}),
    );
    assert_eq!(tb.kind, CswapErrorKind::Failed);
    assert_eq!(tb.message, "EOFError: EOF when reading a line");

    let lock = failing(
        "switch",
        json!({"exit": 1, "stdout": "{\"schemaVersion\":1,\"error\":{\"type\":\"ClaudeCodeLockTimeout\",\"message\":\"Could not acquire x — Claude Code appears to be refreshing credentials. Retry in a few seconds.\"}}"}),
    );
    assert_eq!(lock.kind, CswapErrorKind::Busy);

    let relogin = failing(
        "switch",
        json!({"exit": 1, "stdout": "{\"schemaVersion\":1,\"error\":{\"type\":\"SwitchError\",\"message\":\"activating it would fail with invalid_grant\"}}"}),
    );
    assert_eq!(relogin.kind, CswapErrorKind::ReloginRequired);

    let generic = failing(
        "remove",
        json!({"exit": 1, "stderr": "Error: \u{1b}[31mqualcosa\u{1b}[0m è andato storto\nA newer version of claude-swap is available (9.9.9).\n"}),
    );
    assert_eq!(generic.kind, CswapErrorKind::Failed);
    assert_eq!(generic.message, "qualcosa è andato storto");

    let schema = failing(
        "list",
        json!({"exit": 0, "stdout": "{\"schemaVersion\":2,\"accounts\":[]}"}),
    );
    assert_eq!(schema.kind, CswapErrorKind::UnsupportedVersion);

    let garbage = failing("list", json!({"exit": 0, "stdout": "non json"}));
    assert_eq!(garbage.kind, CswapErrorKind::Failed);
}
