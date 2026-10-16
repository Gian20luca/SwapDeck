//! Test di integrazione di `swapdeck-cli` con il finto cswap (`swapdeck-fake-cswap`) e una
//! cartella dati temporanea (`SWAPDECK_HOME`): niente cswap reale, niente dati dell'utente.

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

struct Env {
    dir: tempfile::TempDir,
    cswap: PathBuf,
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cswap")
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let cswap = dir
            .path()
            .join(format!("cswap{}", std::env::consts::EXE_SUFFIX));
        let src = env!("CARGO_BIN_EXE_swapdeck-fake-cswap");
        if fs::hard_link(src, &cswap).is_err() {
            fs::copy(src, &cswap).unwrap();
        }
        let cfg = json!({
            "fixtures": fixtures(),
            "state": dir.path().join("state.json"),
            "log": dir.path().join("log.jsonl"),
        });
        fs::write(dir.path().join("fake.json"), cfg.to_string()).unwrap();
        fs::copy(fixtures().join("list.json"), dir.path().join("state.json")).unwrap();
        Self { dir, cswap }
    }

    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_swapdeck-cli"));
        cmd.args(args)
            .env("SWAPDECK_CSWAP", &self.cswap)
            .env("SWAPDECK_HOME", self.home())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    fn run(&self, args: &[&str]) -> (i32, Value) {
        parse(self.command(args).output().unwrap())
    }

    fn run_stdin(&self, args: &[&str], input: &str) -> (i32, Value) {
        let mut child = self.command(args).stdin(Stdio::piped()).spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        parse(child.wait_with_output().unwrap())
    }

    fn fake_calls(&self) -> String {
        fs::read_to_string(self.dir.path().join("log.jsonl")).unwrap_or_default()
    }
}

fn parse(out: Output) -> (i32, Value) {
    let text = String::from_utf8(out.stdout).unwrap();
    let value = serde_json::from_str(text.trim()).unwrap_or_else(|e| {
        panic!(
            "stdout non JSON ({e}): {text}\nstderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    });
    (out.status.code().unwrap_or(-1), value)
}

#[test]
fn version_and_usage() {
    let env = Env::new();
    let (code, v) = env.run(&["version"]);
    assert_eq!(code, 0);
    assert_eq!(v["name"], "swapdeck-cli");
    assert_eq!(v["protocol"], 1);
    let (code, v) = env.run(&["nope"]);
    assert_eq!(code, 2);
    assert_eq!(v["error"]["kind"], "invalidUsage");
}

#[test]
fn state_switch_and_confirm() {
    let env = Env::new();
    let (code, s) = env.run(&["state"]);
    assert_eq!(code, 0, "{s}");
    assert_eq!(s["list"]["activeAccountNumber"], 6);
    assert!(s["error"].is_null());
    assert_eq!(s["engine"]["running"], false);
    assert_eq!(s["engine"]["enabled"], false);

    let disable6 = r#"{"kind":"disable","account":6}"#;
    let (code, c) = env.run_stdin(&["confirm", "-"], disable6);
    assert_eq!((code, c["kind"].as_str()), (0, Some("simple")));
    let (code, c) = env.run_stdin(&["confirm", "-", "--active", "3"], disable6);
    assert_eq!((code, c["kind"].as_str()), (0, Some("none")));

    let (code, out) = env.run_stdin(&["action", "-"], r#"{"kind":"switch","target":3}"#);
    assert_eq!(code, 0, "{out}");
    assert!(out["switch"].is_object(), "{out}");
    let (_, s) = env.run(&["state"]);
    assert_eq!(s["list"]["activeAccountNumber"], 3);
}

#[test]
fn action_errors_are_json() {
    let env = Env::new();
    let (code, v) = env.run_stdin(&["action", "-"], "{not json");
    assert_eq!(code, 2);
    assert_eq!(v["error"]["kind"], "invalidUsage");
    let (code, v) = env.run(&["action", r#"{"kind":"switch","target":3}"#]);
    assert_eq!(
        (code, v["error"]["kind"].as_str()),
        (2, Some("invalidUsage"))
    );
    let (code, v) = env.run_stdin(&["action", "-"], r#"{"kind":"switch","target":42}"#);
    assert_eq!(code, 1, "{v}");
    assert!(v["error"]["message"].is_string());
    let (code, v) = env.run_stdin(&["action", "-"], r#"{"kind":"autoStop"}"#);
    assert_eq!(
        (code, v["error"]["kind"].as_str()),
        (2, Some("invalidUsage"))
    );
}

#[test]
fn add_token_reads_stdin_and_never_echoes_it() {
    let env = Env::new();
    let token = "sk-ant-oat01-TESTTOKEN-not-real";
    let input = json!({"kind": "addToken", "token": token, "slot": 9}).to_string();
    let (code, out) = env.run_stdin(&["action", "-"], &input);
    assert_eq!(code, 0, "{out}");
    assert!(!out.to_string().contains(token));
    assert!(env.fake_calls().contains("add-token"));
}

#[test]
fn run_returns_the_command_to_open() {
    let env = Env::new();
    let (code, out) = env.run_stdin(
        &["action", "-"],
        r#"{"kind":"run","options":{"account":3}}"#,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out["command"]["program"].is_string(), "{out}");
    assert!(out["command"]["args"].is_array());
}

#[test]
fn reads_and_cswap_config() {
    let env = Env::new();
    let (code, v) = env.run(&["status"]);
    assert_eq!(code, 0, "{v}");
    let (code, v) = env.run(&["cswap-config"]);
    assert_eq!(code, 0, "{v}");
}

#[test]
fn config_get_and_set() {
    let env = Env::new();
    let (code, v) = env.run(&["config", "get"]);
    assert_eq!(code, 0);
    assert_eq!(v["fromFile"], false);
    let (code, v) = env.run(&["config", "set", r#"{"pollIntervalSeconds":120}"#]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["config"]["pollIntervalSeconds"], 120);
    let (_, v) = env.run(&["config", "get"]);
    assert_eq!(v["fromFile"], true);
    assert_eq!(v["config"]["pollIntervalSeconds"], 120);
    let (code, v) = env.run(&["config", "set", r#"{"pollIntervalSeconds":"x"}"#]);
    assert_eq!(code, 2, "{v}");
    let (code, _) = env.run(&["config", "set", r#"{"engine":{"fiveHourThreshold":"x"}}"#]);
    assert_eq!(code, 2);
    let (code, _) = env.run(&["config", "set", "[1]"]);
    assert_eq!(code, 2);
    let (_, v) = env.run(&["config", "get"]);
    assert_eq!(v["config"]["pollIntervalSeconds"], 120, "file intatto");
}

#[test]
fn missing_cswap() {
    let env = Env::new();
    let missing = env.dir.path().join("missing").join("cswap.exe");
    let out = env
        .command(&["state"])
        .env("SWAPDECK_CSWAP", &missing)
        .output()
        .unwrap();
    let (code, s) = parse(out);
    assert_eq!(code, 0);
    assert!(s["list"].is_null());
    assert_eq!(s["error"]["kind"], "notInstalled");
    let out = env
        .command(&["status"])
        .env("SWAPDECK_CSWAP", &missing)
        .output()
        .unwrap();
    let (code, v) = parse(out);
    assert_eq!(
        (code, v["error"]["kind"].as_str()),
        (1, Some("notInstalled"))
    );
}

#[test]
fn headless_once_writes_history() {
    let env = Env::new();
    let (code, h) = env.run(&["history"]);
    assert_eq!(code, 0);
    assert_eq!(h, json!([]));
    let out = env
        .command(&["headless", "--once", "--dry-run"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let (code, h) = env.run(&["history"]);
    assert_eq!(code, 0);
    let samples = h
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["type"] == "sample")
        .count();
    assert_eq!(samples, 5, "{h}");
    let (_, h) = env.run(&["history", "--account", "3"]);
    assert!(
        h.as_array()
            .unwrap()
            .iter()
            .all(|r| r["type"] != "sample" || r["account"] == 3)
    );
    let (code, _) = env.run(&["history", "--from", "10", "--to", "5"]);
    assert_eq!(code, 2);
    let (_, e) = env.run(&["engine", "status"]);
    assert_eq!(e["running"], false, "lock rilasciato: {e}");
}

#[test]
fn headless_stops_when_stdin_closes() {
    let env = Env::new();
    let mut child = env
        .command(&["headless", "--dry-run", "--stop-on-stdin-eof"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    // Aspetta il primo giro (lock preso), poi chiude stdin.
    let lock = env.home().join("data").join("engine.lock");
    wait_for(|| lock.exists(), Duration::from_secs(30));
    let (_, e) = env.run(&["engine", "status"]);
    assert_eq!(e["running"], true, "{e}");
    stdin.flush().unwrap();
    drop(stdin);
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "headless non si è fermato"
        );
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(status.code(), Some(0));
    let mut log = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut log)
        .unwrap();
    assert!(!lock.exists(), "lock rilasciato");
}

fn wait_for(cond: impl Fn() -> bool, max: Duration) {
    let start = Instant::now();
    while !cond() {
        assert!(start.elapsed() < max, "attesa scaduta");
        std::thread::sleep(Duration::from_millis(100));
    }
}
