//! Scenari di F20 con file jsonl sintetici (mai copie di sessioni reali).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::attribution::Timeline;
use super::estimate::{self, MinuteSeries};
use super::index::{self, UsageIndex};
use super::source::{self, Parsed};
use super::*;
use crate::cswap::UsageStatus;
use crate::engine::SwitchCause;
use crate::history::{SwitchRecord, UsageSample};
use crate::timeutil::format_iso;

/// Oggi alle 00:00 UTC meno un giorno, più 8 ore: le ore di modifica reali dei file restano
/// dentro la conservazione.
fn t0() -> i64 {
    let now = crate::timeutil::now_unix();
    now - now % 86_400 - 86_400 + 8 * 3600
}
const SECRET: &str = "TESTO-RISERVATO-42";

fn assistant(ts: i64, id: &str, model: &str, i: u64, o: u64, cc: u64, cr: u64) -> String {
    serde_json::json!({
        "type": "assistant",
        "timestamp": format_iso(ts),
        "sessionId": "s",
        "cwd": format!("C:/{SECRET}"),
        "message": {
            "id": id,
            "model": model,
            "role": "assistant",
            "content": [{"type": "text", "text": format!("risposta {SECRET} \"usage\": finta")}],
            "usage": {
                "input_tokens": i,
                "output_tokens": o,
                "cache_creation_input_tokens": cc,
                "cache_read_input_tokens": cr,
                "service_tier": "standard"
            }
        }
    })
    .to_string()
}

fn user(ts: i64) -> String {
    serde_json::json!({
        "type": "user",
        "timestamp": format_iso(ts),
        "message": {"role": "user", "content": format!("domanda {SECRET} con \"usage\": {{\"input_tokens\": 9}}")},
        "toolUseResult": {"stdout": SECRET}
    })
    .to_string()
}

fn append(path: &Path, lines: &[String]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    for l in lines {
        f.write_all(l.as_bytes()).unwrap();
        f.write_all(b"\n").unwrap();
    }
}

fn append_raw(path: &Path, text: &str) {
    let mut f = OpenOptions::new().append(true).open(path).unwrap();
    f.write_all(text.as_bytes()).unwrap();
}

/// Cambia l'ora di modifica (alcuni file system hanno risoluzione grossolana).
fn touch(path: &Path, secs: u64) {
    let t = std::time::SystemTime::now() + std::time::Duration::from_secs(secs);
    let f = OpenOptions::new().append(true).open(path).unwrap();
    f.set_modified(t).unwrap();
}

fn sample(
    ts: i64,
    account: u32,
    active: bool,
    five: f64,
    five_reset: i64,
    seven: f64,
    seven_reset: i64,
) -> UsageSample {
    UsageSample {
        ts,
        account,
        active,
        status: UsageStatus::Ok,
        five_hour_pct: Some(five),
        seven_day_pct: Some(seven),
        five_hour_resets_at: Some(format_iso(five_reset)),
        seven_day_resets_at: Some(format_iso(seven_reset)),
        seven_day_expected_pct: None,
        fetched_at: Some(format_iso(ts)),
    }
}

fn switch(ts: i64, from: u32, to: u32) -> HistoryRecord {
    HistoryRecord::Switch(SwitchRecord {
        ts,
        from: Some(from),
        to,
        cause: SwitchCause::PrimaryAtLimit,
        switched: true,
        message: String::new(),
    })
}

fn write_history(data: &Path, records: &[HistoryRecord]) {
    let dir = data.join("history");
    fs::create_dir_all(&dir).unwrap();
    for r in records {
        let d = crate::timeutil::utc_date(r.ts()).unwrap();
        let name = format!(
            "{:04}-{:02}-{:02}.jsonl",
            d.year(),
            u8::from(d.month()),
            d.day()
        );
        append(&dir.join(name), &[serde_json::to_string(r).unwrap()]);
    }
}

struct Env {
    _tmp: tempfile::TempDir,
    projects: PathBuf,
    data: PathBuf,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let projects = tmp.path().join("claude").join("projects");
    let data = tmp.path().join("data");
    fs::create_dir_all(&projects).unwrap();
    fs::create_dir_all(&data).unwrap();
    Env {
        projects,
        data,
        _tmp: tmp,
    }
}

impl Env {
    fn paths(&self) -> UsagePaths {
        UsagePaths {
            data_dir: Some(self.data.clone()),
            projects_dir: Some(self.projects.clone()),
        }
    }
}

fn query(group_by: UsageGroupBy, account: Option<u32>) -> UsageQuery {
    UsageQuery {
        account,
        window: BreakdownWindow::Custom,
        from_seconds: Some(t0() - 86_400),
        to_seconds: Some(t0() + 86_400),
        group_by,
    }
}

#[test]
fn parse_line_keeps_only_numbers() {
    let line = assistant(t0(), "m1", "claude-opus-4-8", 10, 20, 30, 40);
    let Parsed::Usage(e) = source::parse_line(line.as_bytes()) else {
        panic!("riga non riconosciuta");
    };
    assert_eq!(e.ts, t0());
    assert_eq!(e.id.as_deref(), Some("m1"));
    assert_eq!(e.model, "claude-opus-4-8");
    assert_eq!(e.tokens, TokenCounts::new(10, 20, 30, 40));
    // Utente con «"usage":» solo dentro il testo (escapato): non si analizza.
    assert_eq!(source::parse_line(user(t0()).as_bytes()), Parsed::Other);
    // Riga troncata con la chiave: illeggibile.
    assert_eq!(
        source::parse_line(&line.as_bytes()[..line.len() - 5]),
        Parsed::Invalid
    );
    // Consumo nullo e modello sintetico: niente.
    let zero = assistant(t0(), "m2", "<synthetic>", 0, 0, 0, 0);
    assert_eq!(source::parse_line(zero.as_bytes()), Parsed::Other);
}

#[test]
fn dedupe_counts_a_message_once() {
    let mut recent = Vec::new();
    let a = TokenCounts::new(5, 1, 0, 100);
    assert_eq!(index::dedupe(&mut recent, Some("x"), a), (a, true));
    // Stesso messaggio, output cresciuto: si aggiunge solo la differenza.
    let b = TokenCounts::new(5, 7, 0, 100);
    assert_eq!(
        index::dedupe(&mut recent, Some("x"), b),
        (TokenCounts::new(0, 6, 0, 0), false)
    );
    assert_eq!(
        index::dedupe(&mut recent, Some("x"), a),
        (TokenCounts::default(), false)
    );
    assert_eq!(index::dedupe(&mut recent, None, a), (a, true));
}

#[test]
fn read_lines_waits_for_partial_tail() {
    let e = env();
    let path = e.projects.join("p").join("s.jsonl");
    append(&path, &[user(t0())]);
    let full = assistant(t0() + 1, "m1", "claude-opus-4-8", 1, 2, 3, 4);
    append_raw(&path, &full[..40]);
    let mut seen = 0;
    let mut f = source::open_regular(&path).unwrap();
    let len = f.metadata().unwrap().len();
    let out = source::read_lines(&mut f, 0, len, |_| {
        seen += 1;
        true
    })
    .unwrap();
    assert_eq!(seen, 1);
    assert!(out.offset < len, "la riga a metà non si consuma");
    append_raw(&path, &full[40..]);
    let mut f = source::open_regular(&path).unwrap();
    let len = f.metadata().unwrap().len();
    let out2 = source::read_lines(&mut f, out.offset, len, |l| {
        assert!(matches!(source::parse_line(l), Parsed::Usage(_)));
        true
    })
    .unwrap();
    assert_eq!(
        out2.offset, len,
        "riga completa senza a capo finale: consumata"
    );
}

fn sessions_fixture(e: &Env) -> (PathBuf, PathBuf) {
    let main = e.projects.join("c--progetto-uno").join("sess-1.jsonl");
    append(
        &main,
        &[
            user(t0()),
            assistant(t0() + 10, "m1", "claude-opus-4-8", 100, 50, 1000, 0),
            // Stesso messaggio su più righe (blocchi diversi).
            assistant(t0() + 11, "m1", "claude-opus-4-8", 100, 80, 1000, 0),
            "{\"type\":\"assistant\",\"message\":{\"usage\":{\"input_tok".to_owned(),
            assistant(t0() + 20, "m2", "claude-sonnet-4-6", 10, 10, 0, 500),
        ],
    );
    let agent = e
        .projects
        .join("c--progetto-uno")
        .join("sess-1")
        .join("subagents")
        .join("agent-abc.jsonl");
    append(
        &agent,
        &[assistant(t0() + 30, "a1", "claude-haiku-4-5", 7, 3, 0, 0)],
    );
    fs::write(
        agent.with_file_name("agent-abc.meta.json"),
        r#"{"agentType":"Explore","description":"Cerca nel codice","toolUseId":"t","prompt":"non letto"}"#,
    )
    .unwrap();
    let wf = e
        .projects
        .join("c--progetto-due")
        .join("sess-2")
        .join("subagents")
        .join("workflows")
        .join("wf_1")
        .join("agent-def.jsonl");
    append(
        &wf,
        &[assistant(t0() + 40, "w1", "claude-opus-4-8", 1, 1, 0, 0)],
    );
    (main, agent)
}

#[test]
fn scan_reads_sessions_subagents_and_grows_incrementally() {
    let e = env();
    let (main, agent) = sessions_fixture(&e);
    let mut idx = UsageIndex::default();
    let now = t0() + 3600;
    let stats = index::scan(
        &mut idx,
        &e.projects,
        &Timeline::default(),
        now,
        now - 30 * 86_400,
        ScanBudget::unlimited(),
    );
    assert_eq!(stats.files, 3);
    assert_eq!(stats.files_read, 3);
    assert_eq!(stats.messages, 4);
    assert_eq!(stats.skipped, 1, "riga troncata a metà file");
    let b = breakdown(&idx, &[], &query(UsageGroupBy::Agent, None), now).unwrap();
    let main_row = b
        .rows
        .iter()
        .find(|r| r.key == "main:c--progetto-uno/sess-1")
        .unwrap();
    assert_eq!(main_row.totals.tokens, TokenCounts::new(110, 90, 1000, 500));
    assert_eq!(main_row.totals.messages, 2);
    let agent_row = b.rows.iter().find(|r| r.key == "agent:abc").unwrap();
    assert_eq!(agent_row.agent_type.as_deref(), Some("Explore"));
    assert_eq!(agent_row.description.as_deref(), Some("Cerca nel codice"));
    assert!(
        b.rows
            .iter()
            .any(|r| r.key == "agent:def" && r.project.as_deref() == Some("c--progetto-due"))
    );
    // Tutto senza storico: utenza sconosciuta.
    assert_eq!(b.unattributed.tokens, b.total.tokens);

    // Nessun cambiamento: nessun file riletto.
    let again = index::scan(
        &mut idx,
        &e.projects,
        &Timeline::default(),
        now,
        now - 30 * 86_400,
        ScanBudget::unlimited(),
    );
    assert_eq!(again.files_read, 0);

    // Il file cresce: si leggono solo le righe nuove.
    let before = fs::metadata(&agent).unwrap().len();
    append(
        &agent,
        &[assistant(t0() + 50, "a2", "claude-haiku-4-5", 1, 1, 0, 0)],
    );
    touch(&agent, 5);
    let grown = index::scan(
        &mut idx,
        &e.projects,
        &Timeline::default(),
        now,
        now - 30 * 86_400,
        ScanBudget::unlimited(),
    );
    assert_eq!(grown.files_read, 1);
    assert_eq!(grown.messages, 1);
    assert_eq!(grown.bytes, fs::metadata(&agent).unwrap().len() - before);
    let b = breakdown(&idx, &[], &query(UsageGroupBy::Model, None), now).unwrap();
    let haiku = b.rows.iter().find(|r| r.key == "claude-haiku-4-5").unwrap();
    assert_eq!(haiku.totals.tokens, TokenCounts::new(8, 4, 0, 0));
    let shares: f64 = b.rows.iter().map(|r| r.share_pct).sum();
    assert!((shares - 100.0).abs() < 0.05, "{shares}");
    let _ = main;
}

#[test]
fn replaced_file_is_not_counted_twice() {
    let e = env();
    let path = e.projects.join("p").join("s.jsonl");
    append(
        &path,
        &[assistant(t0(), "m1", "claude-opus-4-8", 10, 10, 0, 0)],
    );
    let mut idx = UsageIndex::default();
    let now = t0() + 600;
    index::scan(
        &mut idx,
        &e.projects,
        &Timeline::default(),
        now,
        now - 86_400,
        ScanBudget::unlimited(),
    );
    // Riscritto più corto con lo stesso messaggio e uno nuovo.
    fs::remove_file(&path).unwrap();
    append(
        &path,
        &[
            assistant(t0(), "m1", "claude-opus-4-8", 10, 10, 0, 0),
            assistant(t0() + 5, "m9", "claude-opus-4-8", 1, 0, 0, 0),
        ],
    );
    touch(&path, 10);
    index::scan(
        &mut idx,
        &e.projects,
        &Timeline::default(),
        now,
        now - 86_400,
        ScanBudget::unlimited(),
    );
    let b = breakdown(&idx, &[], &query(UsageGroupBy::Project, None), now).unwrap();
    assert_eq!(b.total.tokens, TokenCounts::new(11, 10, 0, 0));
}

#[test]
fn attribution_follows_switch_in_the_middle_of_a_session() {
    let e = env();
    let five_reset = t0() + 4 * 3600;
    let seven_reset = t0() + 3 * 86_400;
    write_history(
        &e.data,
        &[
            HistoryRecord::Sample(sample(
                t0() - 60,
                1,
                true,
                10.0,
                five_reset,
                20.0,
                seven_reset,
            )),
            switch(t0() + 600, 1, 2),
            HistoryRecord::Sample(sample(
                t0() + 601,
                2,
                true,
                5.0,
                five_reset,
                7.0,
                seven_reset,
            )),
            HistoryRecord::Sample(sample(
                t0() + 1200,
                2,
                true,
                6.0,
                five_reset,
                7.0,
                seven_reset,
            )),
        ],
    );
    let path = e.projects.join("p").join("s.jsonl");
    append(
        &path,
        &[
            assistant(t0() + 100, "m1", "claude-opus-4-8", 10, 0, 0, 0),
            assistant(t0() + 700, "m2", "claude-opus-4-8", 20, 0, 0, 0),
            // Storico muto da più di 30 minuti: sconosciuta.
            assistant(t0() + 1200 + 31 * 60, "m3", "claude-opus-4-8", 40, 0, 0, 0),
        ],
    );
    let now = t0() + 7200;
    let q1 = query(UsageGroupBy::Session, Some(1));
    let b1 = usage_breakdown(&e.paths(), &q1, now).unwrap();
    assert_eq!(b1.total.tokens.input, 10);
    let b2 = usage_breakdown(&e.paths(), &query(UsageGroupBy::Session, Some(2)), now).unwrap();
    assert_eq!(b2.total.tokens.input, 20);
    assert_eq!(b2.unattributed.tokens.input, 40);
    let all = usage_breakdown(&e.paths(), &query(UsageGroupBy::Session, None), now).unwrap();
    assert_eq!(all.total.tokens.input, 70);
    assert!(all.scan.index_bytes == 0 || all.scan.files_read == 0);
    assert!(b1.scan.index_bytes > 0);
}

#[test]
fn timeline_handles_inactive_sample_and_failed_switch() {
    let five = t0() + 3600;
    let recs = vec![
        HistoryRecord::Sample(sample(t0(), 1, true, 1.0, five, 1.0, five)),
        HistoryRecord::Switch(SwitchRecord {
            ts: t0() + 10,
            from: Some(1),
            to: 3,
            cause: SwitchCause::PrimaryAtLimit,
            switched: false,
            message: String::new(),
        }),
        HistoryRecord::Sample(sample(t0() + 20, 1, false, 1.0, five, 1.0, five)),
        HistoryRecord::Sample(sample(t0() + 40, 4, true, 1.0, five, 1.0, five)),
    ];
    let t = Timeline::from_records(&recs);
    assert_eq!(t.account_at(t0() - 1), None);
    assert_eq!(
        t.account_at(t0() + 15),
        Some(1),
        "cambio non riuscito ignorato"
    );
    assert_eq!(t.account_at(t0() + 30), None, "l'attiva non lo è più");
    assert_eq!(t.account_at(t0() + 50), Some(4));
    assert_eq!(t.account_at(t0() + 40 + 31 * 60), None);
}

#[test]
fn index_never_contains_message_text() {
    let e = env();
    sessions_fixture(&e);
    let now = t0() + 3600;
    usage_breakdown(&e.paths(), &query(UsageGroupBy::Agent, None), now).unwrap();
    let text = fs::read_to_string(e.data.join("usage").join("index.json")).unwrap();
    assert!(!text.contains(SECRET), "testo dei messaggi nell'indice");
    assert!(!text.contains("risposta") && !text.contains("domanda") && !text.contains("non letto"));
    assert!(
        text.contains("Cerca nel codice"),
        "descrizione dell'agente dai metadati"
    );
    // Ricaricato uguale; corrotto = ricostruito.
    let (loaded, reset) = index::load(&e.data);
    assert!(!reset);
    assert_eq!(loaded.files.len(), 3);
    fs::write(e.data.join("usage").join("index.json"), b"{rotto").unwrap();
    let b = usage_breakdown(&e.paths(), &query(UsageGroupBy::Agent, None), now).unwrap();
    assert!(b.scan.index_reset);
    assert_eq!(b.scan.files_read, 3);
}

#[test]
fn retention_drops_old_hours_and_files() {
    let e = env();
    sessions_fixture(&e);
    let mut idx = UsageIndex::default();
    let now = t0() + 3600;
    index::scan(
        &mut idx,
        &e.projects,
        &Timeline::default(),
        now,
        now - 86_400,
        ScanBudget::unlimited(),
    );
    assert!(!idx.hours.is_empty());
    idx.prune(t0() + 2 * 3600);
    assert!(idx.hours.is_empty() && idx.minutes.is_empty());
    assert_eq!(idx.models.len(), 0);
    assert_eq!(idx.sources.len(), idx.files.len());
}

#[test]
fn custom_window_is_validated() {
    let idx = UsageIndex::default();
    let mut q = query(UsageGroupBy::Project, None);
    q.from_seconds = Some(t0());
    q.to_seconds = Some(t0() - 1);
    assert!(matches!(
        breakdown(&idx, &[], &q, t0()),
        Err(UsageError::Invalid(_))
    ));
    q.to_seconds = None;
    assert!(breakdown(&idx, &[], &q, t0()).is_err());
    let json = r#"{"window":"5h","groupBy":"model","account":2}"#;
    let parsed: UsageQuery = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.window, BreakdownWindow::FiveHour);
    assert!(serde_json::from_str::<UsageQuery>(r#"{"window":"1h","groupBy":"model"}"#).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    let e = env();
    let outside = e.data.join("fuori.jsonl");
    append(
        &outside,
        &[assistant(t0(), "x", "claude-opus-4-8", 1, 1, 0, 0)],
    );
    let dir = e.projects.join("p");
    fs::create_dir_all(&dir).unwrap();
    std::os::unix::fs::symlink(&outside, dir.join("link.jsonl")).unwrap();
    std::os::unix::fs::symlink(&e.data, e.projects.join("linkdir")).unwrap();
    let mut idx = UsageIndex::default();
    let stats = index::scan(
        &mut idx,
        &e.projects,
        &Timeline::default(),
        t0() + 60,
        t0() - 86_400,
        ScanBudget::unlimited(),
    );
    assert_eq!(stats.files, 0);
    assert!(source::open_regular(&dir.join("link.jsonl")).is_err());
}

// ---- Calibrazione e stima ----

const MODEL: u32 = 0;

/// Indice con la serie al minuto dell'utenza 1 e i campioni: ogni 10 minuti `local_points`
/// punti di consumo locale (k token equivalenti per punto) più `remote(i)` punti da altrove.
fn calibration_fixture(
    k: f64,
    steps: usize,
    remote: impl Fn(usize) -> f64,
) -> (UsageIndex, Vec<UsageSample>, i64) {
    let mut idx = UsageIndex::default();
    idx.models.push("claude-opus-4-8".into());
    let start = t0();
    let mut samples = Vec::new();
    let mut pct = 0.0;
    let mut five_reset = start + 5 * 3600;
    let seven_reset = start + 6 * 86_400;
    let mut seven = 0.0;
    let mut t = start;
    for i in 0..steps {
        if t >= five_reset {
            five_reset += 5 * 3600;
            pct = 0.0;
        }
        samples.push(sample(t, 1, true, pct, five_reset, seven, seven_reset));
        // 4 punti locali in 10 minuti, spalmati sui minuti.
        let local_points = 4.0;
        let per_minute = (local_points * k / 10.0) as u64;
        for m in 0..10 {
            *idx.minutes.entry((t + m * 60, Some(1), MODEL)).or_default() =
                TokenCounts::new(per_minute, 0, 0, 0);
        }
        pct += local_points + remote(i);
        seven += (local_points + remote(i)) / 10.0;
        t += 600;
    }
    samples.push(sample(t, 1, true, pct, five_reset, seven, seven_reset));
    (idx, samples, t + 30)
}

fn five_hour(b: &[OtherDevicesEstimate]) -> &OtherDevicesEstimate {
    b.iter()
        .find(|e| e.account == 1 && e.window == BreakdownWindow::FiveHour)
        .unwrap()
}

#[test]
fn estimate_local_only_is_zero_with_high_confidence() {
    let (idx, samples, now) = calibration_fixture(1000.0, 28, |_| 0.0);
    let refs: Vec<&UsageSample> = samples.iter().collect();
    let cal = estimate::calibrate(&estimate::intervals(
        1,
        &refs,
        BreakdownWindow::FiveHour,
        &MinuteSeries::for_account(&idx, 1),
        &Timeline::from_records(&recs(&samples)),
    ))
    .unwrap();
    assert!((cal.pooled - 1000.0).abs() < 1.0, "{}", cal.pooled);
    assert!(cal.per_model.contains_key(&MODEL));
    let mut q = query(UsageGroupBy::Model, Some(1));
    q.window = BreakdownWindow::FiveHour;
    let b = breakdown(&idx, &recs(&samples), &q, now).unwrap();
    let e = five_hour(&b.other_devices);
    assert_eq!(e.status, EstimateStatus::Available);
    assert!(e.other_pct.unwrap() < 0.5, "{e:?}");
    assert_eq!(e.confidence, Some(Confidence::High));
    assert!(!e.history_gaps);
}

#[test]
fn estimate_finds_remote_share() {
    // Metà degli intervalli con 4 punti da un altro dispositivo; gli ultimi 3 intervalli (nella
    // finestra corrente) pure.
    let (idx, samples, now) =
        calibration_fixture(
            1000.0,
            34,
            |i| if i % 2 == 1 || i >= 31 { 4.0 } else { 0.0 },
        );
    let mut q = query(UsageGroupBy::Project, Some(1));
    q.window = BreakdownWindow::FiveHour;
    let b = breakdown(&idx, &recs(&samples), &q, now).unwrap();
    let e = five_hour(&b.other_devices);
    assert_eq!(e.status, EstimateStatus::Available);
    assert!((e.tokens_per_point.unwrap() - 1000.0).abs() < 50.0, "{e:?}");
    let server = e.server_pct.unwrap();
    let other = e.other_pct.unwrap();
    assert!(other > 0.0 && other <= server);
    assert!((e.local_pct.unwrap() + other - server).abs() < 0.01);
    // Il remoto della finestra corrente: almeno i 3 intervalli finali da 4 punti.
    assert!(other >= 12.0 - 0.5, "{e:?}");
    assert!(b.other_devices_share_pct.unwrap() > 0.0);
    let seven = b
        .other_devices
        .iter()
        .find(|e| e.window == BreakdownWindow::SevenDay)
        .unwrap();
    assert_ne!(seven.status, EstimateStatus::NoData);
}

#[test]
fn estimate_needs_enough_samples() {
    let (idx, samples, now) = calibration_fixture(1000.0, 2, |_| 0.0);
    let mut q = query(UsageGroupBy::Project, Some(1));
    q.window = BreakdownWindow::FiveHour;
    let b = breakdown(&idx, &recs(&samples), &q, now).unwrap();
    let e = five_hour(&b.other_devices);
    assert_eq!(e.status, EstimateStatus::NotCalibrated);
    assert_eq!(e.other_pct, None);
    assert_eq!(b.other_devices_share_pct, None);
    // Nessun campione: nessuna stima.
    let b = breakdown(&idx, &[], &q, now).unwrap();
    assert!(b.other_devices.is_empty());
}

#[test]
fn estimate_flags_history_gaps() {
    let (mut idx, samples, now) = calibration_fixture(1000.0, 28, |_| 0.0);
    // Consumo sconosciuto nella finestra corrente.
    for m in 0..30 {
        idx.minutes.insert(
            (now - 3600 + m * 60, None, MODEL),
            TokenCounts::new(5000, 0, 0, 0),
        );
    }
    let mut q = query(UsageGroupBy::Project, Some(1));
    q.window = BreakdownWindow::FiveHour;
    let b = breakdown(&idx, &recs(&samples), &q, now).unwrap();
    let e = five_hour(&b.other_devices);
    assert!(e.history_gaps);
    assert_eq!(e.confidence, Some(Confidence::Low));
}

/// Lettura reale in sola lettura delle sessioni dell'utente (`--ignored`, meglio con `--release`):
/// stampa solo numeri. L'indice va in una cartella temporanea; lo storico reale si legge soltanto.
#[test]
#[ignore = "legge le sessioni reali di Claude Code"]
fn real_sessions_timing() {
    let Some(claude) = source::claude_dir() else {
        return;
    };
    let projects = claude.join("projects");
    let tmp = tempfile::tempdir().unwrap();
    let history = crate::paths::AppPaths::from_system().map(|p| History::new(&p.data_dir));
    let now = crate::timeutil::now_unix();
    let cutoff = now - 30 * 86_400;
    let records = history
        .as_ref()
        .and_then(|h| h.read_range(cutoff - attribution::LOOKBACK_SECS, now).ok())
        .unwrap_or_default();
    let timeline = Timeline::from_records(&records);
    // Prima lettura a budget (256 MB / 8 s), con salvataggio dopo ogni chiamata come l'app.
    let mut idx = UsageIndex::default();
    let mut first = ScanStats::default();
    for call in 1..=50 {
        let s = index::scan(
            &mut idx,
            &projects,
            &timeline,
            now,
            cutoff,
            ScanBudget::default(),
        );
        index::save(tmp.path(), &idx).unwrap();
        println!(
            "chiamata {call}: letti {} MB in {} ms; avanzamento {:?}",
            s.bytes / 1_000_000,
            s.duration_ms,
            s.partial
                .map(|p| (p.scanned_bytes / 1_000_000, p.total_bytes / 1_000_000))
        );
        first.files = s.files;
        first.files_read += s.files_read;
        first.lines += s.lines;
        first.bytes += s.bytes;
        first.messages += s.messages;
        first.skipped += s.skipped;
        first.duration_ms += s.duration_ms;
        if s.partial.is_none() {
            break;
        }
    }
    let saved = index::save(tmp.path(), &idx).unwrap();
    let t = std::time::Instant::now();
    let (mut loaded, _) = index::load(tmp.path());
    let load_ms = t.elapsed().as_millis();
    let second = index::scan(
        &mut loaded,
        &projects,
        &timeline,
        now + 1,
        cutoff,
        ScanBudget::default(),
    );
    let t = std::time::Instant::now();
    let mut q = query(UsageGroupBy::Session, None);
    q.window = BreakdownWindow::SevenDay;
    let b = breakdown(&loaded, &records, &q, now).unwrap();
    let breakdown_ms = t.elapsed().as_millis();
    let attributed = b.total.equivalent - b.unattributed.equivalent;
    println!(
        "prima lettura: file {} letti {} righe {} MB {} messaggi {} saltate {} durata {} ms",
        first.files,
        first.files_read,
        first.lines,
        first.bytes / 1_000_000,
        first.messages,
        first.skipped,
        first.duration_ms
    );
    println!(
        "indice: {} KB, ore {}, minuti {}, sorgenti {}; caricamento {} ms",
        saved / 1024,
        loaded.hours.len(),
        loaded.minutes.len(),
        loaded.sources.len(),
        load_ms
    );
    println!(
        "seconda lettura: letti {} righe {} durata {} ms; classifica 7d {} righe in {} ms; quota attribuita {:.0}%; stime {}",
        second.files_read,
        second.lines,
        second.duration_ms,
        b.rows.len() + b.truncated_rows as usize,
        breakdown_ms,
        if b.total.equivalent > 0.0 {
            attributed / b.total.equivalent * 100.0
        } else {
            0.0
        },
        b.other_devices
            .iter()
            .map(|e| format!("{:?}", e.status))
            .collect::<Vec<_>>()
            .join(",")
    );
    // Id di messaggio ripetuti tra file diversi (sessioni riprese), sugli ultimi 2 giorni.
    let mut found = Vec::new();
    source::discover(&projects, (now - 2 * 86_400) * 1000, &mut found);
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut cross = 0usize;
    for (n, f) in found.iter().enumerate() {
        let Ok(mut file) = source::open_regular(&f.path) else {
            continue;
        };
        let len = file.metadata().unwrap().len();
        let _ = source::read_lines(&mut file, 0, len, |l| {
            if let Parsed::Usage(e) = source::parse_line(l) {
                if let Some(id) = e.id {
                    match seen.get(&id) {
                        Some(&owner) if owner != n => cross += 1,
                        Some(_) => {}
                        None => {
                            seen.insert(id, n);
                        }
                    }
                }
            }
            true
        });
    }
    println!(
        "ultimi 2 giorni: file {}, messaggi {}, righe con id già visto in un altro file {}",
        found.len(),
        seen.len(),
        cross
    );
}

// ---- Correzioni della revisione ----

fn recs(samples: &[UsageSample]) -> Vec<HistoryRecord> {
    samples.iter().cloned().map(HistoryRecord::Sample).collect()
}

fn heartbeat(ts: i64, active: Option<u32>, interval_secs: u32) -> HistoryRecord {
    HistoryRecord::Heartbeat(crate::history::HeartbeatRecord {
        ts,
        active,
        interval_secs,
    })
}

#[test]
fn heartbeats_witness_active_account_and_set_max_gap() {
    let t = t0();
    // Battiti ogni minuto senza campioni nuovi: l'utenza resta nota.
    let mut recs: Vec<HistoryRecord> = (0..10)
        .map(|i| heartbeat(t + i * 60, Some(2), 60))
        .collect();
    // Cambio visto solo dal battito (nessun campione, nessun cambio di SwapDeck).
    recs.push(heartbeat(t + 600, Some(3), 60));
    let tl = Timeline::from_records(&recs);
    assert_eq!(tl.account_at(t + 590), Some(2));
    assert_eq!(tl.account_at(t + 600), Some(3));
    // Silenzio ammesso dall'intervallo: 2 × 60 + 120 s.
    assert_eq!(tl.account_at(t + 600 + 240), Some(3));
    assert_eq!(tl.account_at(t + 600 + 241), None);
    // Intervallo di un'ora: silenzio ammesso più lungo.
    let tl = Timeline::from_records(&[heartbeat(t, Some(1), 3600)]);
    assert_eq!(tl.account_at(t + 2 * 3600), Some(1));
    // Battito senza utenza attiva: sconosciuta.
    let tl = Timeline::from_records(&[heartbeat(t, Some(1), 60), heartbeat(t + 60, None, 60)]);
    assert_eq!(tl.account_at(t + 61), None);
    assert!(tl.active_throughout(1, t, t + 59));
    assert!(!tl.active_throughout(1, t, t + 60));
}

#[test]
fn unreadable_history_stops_without_saving() {
    let e = env();
    sessions_fixture(&e);
    let q = query(UsageGroupBy::Project, None);
    let now = t0() + 3600;
    let err = scan_and_rank(
        &e.paths(),
        &q,
        now,
        ScanBudget::unlimited(),
        Err(io::Error::other("storico rotto")),
    );
    assert!(matches!(err, Err(UsageError::Io(_))));
    assert!(
        !e.data
            .join(index::USAGE_DIR)
            .join(index::INDEX_FILE)
            .exists()
    );
    // Alla lettura dopo riesce e conta tutto.
    let b = usage_breakdown(&e.paths(), &q, now).unwrap();
    assert!(b.total.messages >= 4, "{:?}", b.total);
}

#[test]
fn budget_reads_in_several_calls_with_progress() {
    let e = env();
    sessions_fixture(&e);
    let q = query(UsageGroupBy::Agent, None);
    let now = t0() + 3600;
    let tiny = ScanBudget {
        max_bytes: 1,
        max_duration: std::time::Duration::from_secs(60),
    };
    let mut calls = 0;
    let last = loop {
        calls += 1;
        let b = usage_breakdown_with(&e.paths(), &q, now, tiny).unwrap();
        match b.partial {
            Some(p) => {
                assert!(p.scanned_bytes < p.total_bytes, "{p:?}");
                assert!(b.scan.bytes > 0);
            }
            None => break b,
        }
        assert!(calls < 50);
    };
    assert!(calls > 3, "una riga per chiamata: {calls}");
    let fresh = env();
    sessions_fixture(&fresh);
    let all = usage_breakdown_with(&fresh.paths(), &q, now, ScanBudget::unlimited()).unwrap();
    assert!(all.partial.is_none());
    assert_eq!(last.total, all.total);
    assert_eq!(last.rows, all.rows);
}

#[test]
fn replaced_file_skips_later_lines_of_old_messages() {
    let e = env();
    let path = e.projects.join("p").join("s.jsonl");
    append(
        &path,
        &[
            assistant(t0(), "m1", "claude-opus-4-8", 10, 10, 0, 0),
            // Riga lunga: il file nuovo è più corto (rilettura da capo anche dove il file
            // system ricrea il file con la stessa identità).
            serde_json::json!({"type": "user", "pad": "x".repeat(8000)}).to_string(),
        ],
    );
    let mut idx = UsageIndex::default();
    let now = t0() + 600;
    let tl = Timeline::default();
    index::scan(
        &mut idx,
        &e.projects,
        &tl,
        now,
        now - 86_400,
        ScanBudget::unlimited(),
    );
    fs::remove_file(&path).unwrap();
    // Il messaggio già contato ricompare con una riga successiva più grande: non va aggiunta.
    // Budget minimo: la rilettura passa per più chiamate e il limite resta tra l'una e l'altra.
    append(
        &path,
        &[
            assistant(t0(), "m1", "claude-opus-4-8", 10, 10, 0, 0),
            assistant(t0(), "m1", "claude-opus-4-8", 10, 99, 0, 0),
            assistant(t0() + 40, "m9", "claude-opus-4-8", 1, 0, 0, 0),
        ],
    );
    touch(&path, 10);
    let one_line = ScanBudget {
        max_bytes: 1,
        max_duration: std::time::Duration::from_secs(60),
    };
    let mut calls = 0;
    while index::scan(&mut idx, &e.projects, &tl, now, now - 86_400, one_line)
        .partial
        .is_some()
    {
        calls += 1;
        assert!(calls < 10);
    }
    assert!(calls >= 2, "{calls}");
    let b = breakdown(&idx, &[], &query(UsageGroupBy::Project, None), now).unwrap();
    assert_eq!(b.total.tokens, TokenCounts::new(11, 10, 0, 0));
    assert!(idx.files.values().all(|f| f.floor.is_none()));
}

#[test]
fn calibration_skips_intervals_around_account_changes() {
    let (idx, samples, _) = calibration_fixture(1000.0, 28, |_| 0.0);
    let refs: Vec<&UsageSample> = samples.iter().collect();
    let series = MinuteSeries::for_account(&idx, 1);
    let base = recs(&samples);
    let count = |records: &[HistoryRecord]| {
        estimate::intervals(
            1,
            &refs,
            BreakdownWindow::FiveHour,
            &series,
            &Timeline::from_records(records),
        )
        .len()
    };
    let before = count(&base);
    assert!(before >= 20, "{before}");
    // Cambio a un'altra utenza e ritorno dentro un intervallo: quell'intervallo e i vicini
    // (margine di un giro) si scartano.
    let mut changed = base.clone();
    changed.push(switch(t0() + 3 * 600 + 100, 1, 2));
    changed.push(switch(t0() + 3 * 600 + 200, 2, 1));
    let after = count(&changed);
    assert!(after < before && after >= before - 3, "{before} -> {after}");
    // Utenza non attiva (solo campioni con active: false): nessun intervallo.
    let inactive: Vec<UsageSample> = samples
        .iter()
        .cloned()
        .map(|mut s| {
            s.active = false;
            s
        })
        .collect();
    assert_eq!(count(&recs(&inactive)), 0);
}

#[test]
fn local_sum_stops_at_last_reading() {
    let (mut idx, samples, now) = calibration_fixture(1000.0, 28, |_| 0.0);
    let mut q = query(UsageGroupBy::Model, Some(1));
    q.window = BreakdownWindow::FiveHour;
    let est = |idx: &UsageIndex| {
        five_hour(
            &breakdown(idx, &recs(&samples), &q, now)
                .unwrap()
                .other_devices,
        )
        .clone()
    };
    let before = est(&idx);
    assert_eq!(before.status, EstimateStatus::Available);
    // Consumo nel minuto dell'ultima lettura (dopo il dato del server): non conta.
    let last_t = samples.last().unwrap().ts;
    idx.minutes.insert(
        (last_t, Some(1), MODEL),
        TokenCounts::new(1_000_000, 0, 0, 0),
    );
    assert_eq!(before.local_pct, est(&idx).local_pct);
}

#[test]
fn model_k_needs_five_intervals_and_lowers_confidence() {
    let interval = |m: u32, u: f64| estimate::Interval {
        from: 0,
        to: 1,
        points: 4.0,
        local: [(m, u)].into_iter().collect(),
        unknown: 0.0,
    };
    let mut list: Vec<estimate::Interval> = (0..10).map(|_| interval(0, 4000.0)).collect();
    list.extend((0..4).map(|_| interval(1, 4400.0)));
    let cal = estimate::calibrate(&list).unwrap();
    assert!(cal.per_model.contains_key(&0));
    assert!(!cal.per_model.contains_key(&1), "solo 4 intervalli");
    list.push(interval(1, 4400.0));
    assert!(
        estimate::calibrate(&list)
            .unwrap()
            .per_model
            .contains_key(&1)
    );

    // Nella finestra corrente un modello senza k proprio oltre il 10%: fiducia di un grado sotto.
    let (mut idx, samples, now) = calibration_fixture(1000.0, 28, |_| 0.0);
    idx.models.push("claude-haiku-4-5".into());
    let last_t = samples.last().unwrap().ts;
    idx.minutes.insert(
        (last_t - 300, Some(1), 1),
        TokenCounts::new(20_000, 0, 0, 0),
    );
    let mut q = query(UsageGroupBy::Model, Some(1));
    q.window = BreakdownWindow::FiveHour;
    let e = five_hour(
        &breakdown(&idx, &recs(&samples), &q, now)
            .unwrap()
            .other_devices,
    )
    .clone();
    assert_eq!(e.confidence, Some(Confidence::Medium), "{e:?}");
}

#[test]
fn project_label_is_only_the_last_part() {
    assert_eq!(
        source::project_label("C--ProgettiMieiAi-WidgetCswap"),
        "WidgetCswap"
    );
    assert_eq!(source::project_label("-home-utente-progetti-app-"), "app");
    assert_eq!(source::project_label("solo"), "solo");
    let e = env();
    sessions_fixture(&e);
    let b = usage_breakdown(&e.paths(), &query(UsageGroupBy::Session, None), t0() + 3600).unwrap();
    assert!(!b.rows.is_empty());
    for r in &b.rows {
        let label = r.project_label.as_deref().unwrap();
        assert!(label == "uno" || label == "due", "{label}");
    }
}
