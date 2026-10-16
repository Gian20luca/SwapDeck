//! Test del modulo `setup` con il finto cswap e il finto uv (`tests/support/fake_cswap.rs`),
//! più una prova reale d'installazione in un profilo isolato (`--ignored`).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use swapdeck_core::cswap::{CswapErrorKind, SearchEnv};
use swapdeck_core::setup::{
    Setup, SetupProgress, SetupStatus, SetupStep, SetupTimeouts, update::CACHE_FILE,
};
use tempfile::TempDir;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cswap")
}

fn exe_path(dir: &Path, base: &str) -> PathBuf {
    dir.join(format!("{base}{}", std::env::consts::EXE_SUFFIX))
}

fn link_fake(dest: &Path) {
    let src = env!("CARGO_BIN_EXE_fake-cswap");
    if fs::hard_link(src, dest).is_err() {
        fs::copy(src, dest).unwrap();
    }
}

struct Env {
    dir: TempDir,
}

impl Env {
    /// Cartella `bin` con il finto uv (se `uv`) e il finto cswap (se `cswap`), config `extra`.
    fn new(uv: bool, cswap: bool, state: Option<&str>, extra: Value) -> Self {
        let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let mut cfg = json!({
            "fixtures": fixtures(),
            "state": dir.path().join("state.json"),
            "installTo": exe_path(&bin, "cswap"),
        });
        for (k, v) in extra.as_object().unwrap() {
            cfg[k] = v.clone();
        }
        fs::write(bin.join("fake.json"), cfg.to_string()).unwrap();
        if let Some(s) = state {
            fs::copy(fixtures().join(s), dir.path().join("state.json")).unwrap();
        }
        if uv {
            link_fake(&exe_path(&bin, "uv"));
        }
        if cswap {
            link_fake(&exe_path(&bin, "cswap"));
        }
        Self { dir }
    }

    fn setup(&self) -> Setup {
        let search = SearchEnv {
            path: Some(self.dir.path().join("bin").into_os_string()),
            home: Some(self.dir.path().join("home")),
            ..SearchEnv::default()
        };
        Setup::new(self.dir.path().join("data"))
            .with_search(search)
            .with_timeouts(SetupTimeouts {
                probe: Duration::from_secs(20),
                install: Duration::from_secs(20),
                http: Duration::from_secs(5),
            })
    }
}

fn collect(events: &mut Vec<SetupProgress>) -> impl FnMut(SetupProgress) + '_ {
    move |e| events.push(e)
}

#[test]
fn diagnose_ready() {
    let env = Env::new(true, true, Some("list.json"), json!({}));
    let d = env.setup().diagnose();
    assert_eq!(d.status, SetupStatus::Ready, "{d:?}");
    assert_eq!(d.cswap.unwrap().version.as_deref(), Some("0.27.2"));
    assert_eq!(d.uv.unwrap().version.as_deref(), Some("0.9.9"));
    assert_eq!((d.schema_version, d.account_count), (Some(1), Some(5)));
    assert_eq!(d.min_version, "0.26.0");
}

#[test]
fn diagnose_without_accounts_or_tools() {
    let empty = Env::new(false, true, None, json!({}));
    let d = empty.setup().diagnose();
    assert_eq!((d.status, d.account_count), (SetupStatus::Ready, Some(0)));
    assert!(d.uv.is_none());

    let missing = Env::new(true, false, None, json!({}));
    let d = missing.setup().diagnose();
    assert_eq!(d.status, SetupStatus::CswapMissing);
    assert!(d.cswap.is_none() && d.uv.is_some());
}

#[test]
fn diagnose_unsupported_version_and_schema() {
    let old = Env::new(
        true,
        true,
        Some("list.json"),
        json!({"version": "cswap 0.25.9"}),
    );
    let d = old.setup().diagnose();
    assert_eq!(d.status, SetupStatus::UnsupportedVersion);
    assert_eq!(d.error.unwrap().kind, CswapErrorKind::UnsupportedVersion);

    let schema = Env::new(true, true, None, json!({}));
    fs::write(
        schema.dir.path().join("state.json"),
        r#"{"schemaVersion":2,"accounts":[]}"#,
    )
    .unwrap();
    let d = schema.setup().diagnose();
    assert_eq!(
        (d.status, d.schema_version),
        (SetupStatus::UnsupportedSchema, Some(2))
    );
    assert_eq!(d.error.unwrap().kind, CswapErrorKind::UnsupportedSchema);

    let broken = Env::new(
        true,
        true,
        None,
        json!({"overrides": {"list": {"exit": 1, "stderr": "Traceback (most recent call last)\nboom"}}}),
    );
    let d = broken.setup().diagnose();
    assert_eq!(d.status, SetupStatus::Error);
}

#[test]
fn install_with_existing_uv_streams_clean_lines() {
    let env = Env::new(
        true,
        false,
        None,
        json!({"uvStderr": "Downloading for mario.rossi@example.com sk-ant-oat01-SEGRETO123\n"}),
    );
    let mut events = Vec::new();
    let outcome = env.setup().install(&mut collect(&mut events)).unwrap();
    assert_eq!(outcome.diagnosis.status, SetupStatus::Ready);
    assert!(!outcome.uv_installed && outcome.previous_version.is_none());
    assert_eq!(outcome.diagnosis.account_count, Some(0));

    let steps: Vec<String> = events
        .iter()
        .filter(|e| !matches!(e, SetupProgress::Line { .. }))
        .map(|e| serde_json::to_value(e).unwrap())
        .map(|v| {
            format!(
                "{}:{}",
                v["event"].as_str().unwrap(),
                v["step"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        steps,
        [
            "stepSkipped:downloadUv",
            "stepSkipped:installUv",
            "stepStarted:installCswap",
            "stepDone:installCswap",
            "stepStarted:verifyCswap",
            "stepDone:verifyCswap",
        ]
    );
    let lines: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            SetupProgress::Line {
                step: SetupStep::InstallCswap,
                text,
            } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let all = lines.join("\n");
    assert!(all.contains("Installed 2 executables"), "{all}");
    assert!(
        all.contains("ma***@example.com") && all.contains("sk-ant-***"),
        "{all}"
    );
    assert!(
        !all.contains("mario.rossi") && !all.contains("SEGRETO"),
        "{all}"
    );
}

#[test]
fn install_failure_is_reported_as_step_failed() {
    let env = Env::new(
        true,
        false,
        None,
        json!({"overrides": {"uvInstall": {"exit": 2, "stderr": "error: No solution found\n"}}}),
    );
    let mut events = Vec::new();
    let e = env.setup().install(&mut collect(&mut events)).unwrap_err();
    assert_eq!(e.kind, CswapErrorKind::Failed);
    assert_eq!(e.message, "No solution found");
    assert!(matches!(
        events.last(),
        Some(SetupProgress::StepFailed {
            step: SetupStep::InstallCswap,
            ..
        })
    ));
}

#[test]
fn upgrade_verifies_version_and_schema() {
    let ok = Env::new(
        true,
        true,
        Some("list.json"),
        json!({"upgradeTo": "0.28.0"}),
    );
    let mut events = Vec::new();
    let outcome = ok.setup().upgrade(&mut collect(&mut events)).unwrap();
    assert_eq!(outcome.previous_version.as_deref(), Some("0.27.2"));
    assert_eq!(
        outcome.diagnosis.cswap.unwrap().version.as_deref(),
        Some("0.28.0")
    );

    let schema = Env::new(true, true, Some("list.json"), json!({"upgradeSchema": 2}));
    let mut events = Vec::new();
    let e = schema
        .setup()
        .upgrade(&mut collect(&mut events))
        .unwrap_err();
    assert_eq!(e.kind, CswapErrorKind::UnsupportedSchema);
    assert!(matches!(
        events.last(),
        Some(SetupProgress::StepFailed {
            step: SetupStep::VerifyCswap,
            ..
        })
    ));
}

#[test]
fn upgrade_busy_and_without_uv() {
    let stderr = "error: failed to remove directory: The process cannot access the file because \
                  it is being used by another process. (os error 32)\n";
    let busy = Env::new(
        true,
        true,
        Some("list.json"),
        json!({"overrides": {"uv": {"exit": 2, "stderr": stderr}}}),
    );
    let e = busy.setup().upgrade(&mut |_| {}).unwrap_err();
    assert_eq!(e.kind, CswapErrorKind::Busy);
    assert!(
        e.message.contains("chiudi i processi cswap"),
        "{}",
        e.message
    );

    let no_uv = Env::new(false, true, Some("list.json"), json!({}));
    let e = no_uv.setup().upgrade(&mut |_| {}).unwrap_err();
    assert_eq!(e.kind, CswapErrorKind::NotInstalled);
}

#[test]
fn update_check_uses_cache_and_remembers_dismissal() {
    let env = Env::new(true, true, Some("list.json"), json!({}));
    let data = env.dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    fs::write(
        data.join(CACHE_FILE),
        json!({"checkedAt": now - 60, "latest": "0.28.0"}).to_string(),
    )
    .unwrap();
    let setup = env.setup();
    let info = setup.check_update(false).unwrap();
    assert_eq!(info.installed.as_deref(), Some("0.27.2"));
    assert!(info.update_available && !info.dismissed);
    assert_eq!(info.checked_at, now - 60);
    setup.dismiss_update("0.28.0").unwrap();
    assert!(setup.check_update(false).unwrap().dismissed);
    assert!(setup.dismiss_update("../x").is_err());
}

/// Installazione reale di uv e claude-swap in un profilo isolato (rete necessaria).
/// Su Windows va lanciata da PowerShell:
/// `cargo test -p swapdeck-core --test setup real_install -- --ignored --nocapture`.
#[test]
#[ignore = "rete e installazione reale in una cartella temporanea"]
fn real_install_in_isolated_profile() {
    let root = tempfile::tempdir().unwrap();
    let r = root.path();
    let home = r.join("home");
    let bin = r.join("bin");
    let p = |rel: &str| r.join(rel).into_os_string();
    // Le cartelle dati devono esistere: altrimenti Windows PowerShell non le risolve e
    // scrive la cache dei moduli in un percorso relativo alla cartella corrente.
    for rel in ["home/AppData/Roaming", "home/AppData/Local", "tmp", "bin"] {
        fs::create_dir_all(r.join(rel)).unwrap();
    }
    let vars: Vec<(&str, OsString)> = vec![
        ("TEMP", p("tmp")),
        ("TMP", p("tmp")),
        ("PSModuleAnalysisCachePath", p("home/AppData/Local/ps-module-cache")),
        ("HOME", home.clone().into_os_string()),
        ("USERPROFILE", home.clone().into_os_string()),
        ("APPDATA", p("home/AppData/Roaming")),
        ("LOCALAPPDATA", p("home/AppData/Local")),
        ("XDG_CONFIG_HOME", p("home/.config")),
        ("XDG_DATA_HOME", p("home/.local/share")),
        ("XDG_CACHE_HOME", p("home/.cache")),
        ("XDG_BIN_HOME", bin.clone().into_os_string()),
        ("CLAUDE_CONFIG_DIR", p("home/.claude")),
        ("UV_UNMANAGED_INSTALL", bin.clone().into_os_string()),
        ("UV_NO_MODIFY_PATH", "1".into()),
        ("UV_NO_CONFIG", "1".into()),
        ("UV_TOOL_DIR", p("tools")),
        ("UV_TOOL_BIN_DIR", bin.clone().into_os_string()),
        ("UV_CACHE_DIR", p("cache")),
        ("UV_PYTHON_INSTALL_DIR", p("python")),
        ("UV_PYTHON_BIN_DIR", p("python-bin")),
    ];
    let child_env = vars.into_iter().map(|(k, v)| (k.into(), v)).collect();
    let search = SearchEnv {
        uv_tool_bin_dir: Some(bin.clone().into_os_string()),
        path: Some(bin.clone().into_os_string()),
        xdg_bin_home: Some(bin.clone().into_os_string()),
        home: Some(home.clone()),
        ..SearchEnv::default()
    };
    let setup = Setup::new(r.join("data"))
        .with_search(search)
        .with_child_env(child_env);
    let before = setup.diagnose();
    assert_eq!(before.status, SetupStatus::CswapMissing);
    assert!(before.uv.is_none(), "uv non deve venire dal profilo reale");
    let outcome = setup
        .install(&mut |e| println!("{}", serde_json::to_string(&e).unwrap()))
        .unwrap();
    println!("{}", serde_json::to_string_pretty(&outcome).unwrap());
    assert!(outcome.uv_installed);
    assert_eq!(outcome.diagnosis.status, SetupStatus::Ready);
    assert!(Path::new(&outcome.diagnosis.cswap.unwrap().path).starts_with(r));
    assert!(Path::new(&outcome.diagnosis.uv.unwrap().path).starts_with(r));
    let update = setup.check_update(true).unwrap();
    println!("{}", serde_json::to_string(&update).unwrap());
    assert!(!update.update_available, "appena installata è l'ultima");
    // Aggiornamento reale (nessuna versione nuova: uv lo dice e la verifica passa).
    let upgraded = setup
        .upgrade(&mut |e| println!("{}", serde_json::to_string(&e).unwrap()))
        .unwrap();
    assert_eq!(upgraded.diagnosis.status, SetupStatus::Ready);
    assert_eq!(upgraded.previous_version, update.installed);
}
