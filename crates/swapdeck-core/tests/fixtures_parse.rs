//! Parsing di tutte le fixture di `fixtures/cswap/`.

use std::fs;
use std::path::PathBuf;

use swapdeck_core::cswap::client::parse_auto_line;
use swapdeck_core::cswap::error::{account_issue, parse_error_envelope};
use swapdeck_core::cswap::{
    AutoEvent, AutoMessage, ConfigGetResponse, ConfigListResponse, CswapErrorKind, CswapVersion,
    ListResponse, StatusResponse, SwitchReason, SwitchResponse, UsageStatus,
};

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cswap")
}

fn read(name: &str) -> String {
    fs::read_to_string(dir().join(name)).unwrap()
}

fn names(prefix: &str, ext: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix) && n.ends_with(ext))
        .collect();
    v.sort();
    assert!(!v.is_empty(), "nessuna fixture {prefix}*{ext}");
    v
}

#[test]
fn every_json_fixture_is_covered() {
    let known = ["list", "status", "switch-", "error-", "config-"];
    for n in names("", ".json") {
        assert!(
            known.iter().any(|k| n.starts_with(k)),
            "fixture senza test: {n}"
        );
    }
}

#[test]
fn list_fixtures() {
    for n in names("list", ".json") {
        let l: ListResponse =
            serde_json::from_str(&read(&n)).unwrap_or_else(|e| panic!("{n}: {e}"));
        assert_eq!(l.schema_version, 1, "{n}");
        for a in &l.accounts {
            assert!(a.number >= 1, "{n}");
            assert_eq!(a.usage.is_some(), a.usage_status == UsageStatus::Ok, "{n}");
        }
    }
    let real: ListResponse = serde_json::from_str(&read("list.json")).unwrap();
    assert_eq!(real.accounts.len(), 5);
    assert_eq!(real.active_account_number, Some(6));
    assert!(real.accounts.iter().any(|a| a.login_expires_at.is_some()));
    let scoped = real.accounts[0]
        .usage
        .as_ref()
        .unwrap()
        .scoped
        .as_ref()
        .unwrap();
    assert_eq!(scoped[0].name.as_deref(), Some("Fable"));
    let empty: ListResponse = serde_json::from_str(&read("list-no-accounts.json")).unwrap();
    assert!(empty.accounts.is_empty() && empty.active_account_number.is_none());
}

#[test]
fn account_issues() {
    let rl: ListResponse = serde_json::from_str(&read("list-rate-limited.json")).unwrap();
    assert_eq!(
        account_issue(&rl.accounts[0]),
        Some(CswapErrorKind::StaleUsage)
    );
    assert_eq!(
        account_issue(&rl.accounts[1]),
        Some(CswapErrorKind::RateLimited)
    );
    assert_eq!(
        rl.accounts[1].usage_retry_at.as_deref(),
        Some("2026-10-15T19:36:00Z")
    );
    let te: ListResponse = serde_json::from_str(&read("list-token-expired.json")).unwrap();
    assert_eq!(
        account_issue(&te.accounts[0]),
        Some(CswapErrorKind::TokenExpired)
    );
    assert_eq!(
        account_issue(&te.accounts[1]),
        Some(CswapErrorKind::ReloginRequired)
    );
    let st: ListResponse = serde_json::from_str(&read("list-usage-stale.json")).unwrap();
    let kinds: Vec<_> = st.accounts.iter().map(account_issue).collect();
    assert_eq!(
        kinds,
        [
            Some(CswapErrorKind::StaleUsage),
            Some(CswapErrorKind::StaleUsage),
            None,
            Some(CswapErrorKind::Failed)
        ]
    );
    let dis: ListResponse = serde_json::from_str(&read("list-disabled.json")).unwrap();
    assert!(dis.accounts.iter().any(|a| a.is_disabled()));
    assert!(dis.accounts.iter().any(|a| !a.is_organization));
}

#[test]
fn status_fixtures() {
    for n in names("status", ".json") {
        let s: StatusResponse =
            serde_json::from_str(&read(&n)).unwrap_or_else(|e| panic!("{n}: {e}"));
        assert_eq!(s.schema_version, 1);
    }
    let s: StatusResponse = serde_json::from_str(&read("status.json")).unwrap();
    let a = s.active.unwrap();
    assert!(a.managed && a.number == Some(6) && a.usage.is_some());
    let u: StatusResponse = serde_json::from_str(&read("status-unmanaged.json")).unwrap();
    assert!(!u.active.unwrap().managed);
    let none: StatusResponse = serde_json::from_str(&read("status-no-active.json")).unwrap();
    assert!(none.active.is_none());
}

#[test]
fn switch_fixtures() {
    for n in names("switch-", ".json") {
        let s: SwitchResponse =
            serde_json::from_str(&read(&n)).unwrap_or_else(|e| panic!("{n}: {e}"));
        assert_ne!(s.reason, SwitchReason::Unknown, "{n}");
    }
    let s: SwitchResponse = serde_json::from_str(&read("switch-switched.json")).unwrap();
    assert!(s.switched && s.to.unwrap().number == Some(1));
    let u: SwitchResponse = serde_json::from_str(&read("switch-unmanaged.json")).unwrap();
    assert_eq!(u.from.unwrap().number, None);
    let b: SwitchResponse =
        serde_json::from_str(&read("switch-best-usage-unavailable.json")).unwrap();
    assert_eq!(b.models.unwrap(), ["Fable"]);
}

#[test]
fn error_and_config_fixtures() {
    for n in names("error-", ".json") {
        let e = parse_error_envelope(&read(&n)).unwrap_or_else(|| panic!("{n}"));
        assert!(!e.error.error_type.is_empty() && !e.error.message.is_empty());
    }
    let c: ConfigListResponse = serde_json::from_str(&read("config-list.json")).unwrap();
    assert_eq!(c.settings.len(), 9);
    assert!(c.settings.iter().all(|s| !s.is_set));
    let g: ConfigGetResponse = serde_json::from_str(&read("config-get.json")).unwrap();
    assert_eq!(g.value, serde_json::json!(90.0));
}

#[test]
fn auto_events_and_version() {
    let msgs: Vec<AutoMessage> = read("auto-events.jsonl")
        .lines()
        .filter_map(parse_auto_line)
        .collect();
    assert_eq!(msgs.len(), 7);
    assert!(
        matches!(&msgs[0], AutoMessage::Event(AutoEvent::Poll { fetch_errors: Some(f), .. }) if f["2"] == "http-429")
    );
    assert!(
        matches!(&msgs[6], AutoMessage::Error(e) if e.error_type.as_deref() == Some("ConfigError"))
    );
    assert!(parse_auto_line(r#"{"schemaVersion":1,"event":"nuovo","ts":"x"}"#).is_none());
    let v = CswapVersion::parse(&read("version.txt")).unwrap();
    assert!(v.is_supported());
}

#[test]
fn unknown_fields_and_values_are_tolerated() {
    let text = r#"{"schemaVersion":1,"activeAccountNumber":1,"futureField":true,
      "accounts":[{"number":1,"email":"a@example.com","usageStatus":"brand_new","usage":null,"extra":1}]}"#;
    let l: ListResponse = serde_json::from_str(text).unwrap();
    assert_eq!(l.accounts[0].usage_status, UsageStatus::Unknown);
}
