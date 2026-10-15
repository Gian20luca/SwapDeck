//! Rilevamento dei conflitti (F11): altri motori che cambiano account da soli.
//!
//! cswap non ha file PID: si guardano l'elenco dei processi (`cswap auto`, `tui`, `watch`,
//! `menubar`) e le attività pianificate che citano cswap nel nome o nel comando
//! (`Get-ScheduledTask`, `crontab -l` e `systemctl --user list-timers`, `launchctl list`).
//! Tutto senza privilegi di amministratore. Una lettura non verificata sospende il motore
//! (`tasks:unverified`, ignorabile da config): non vale come «nessun conflitto».
//!
//! Su Windows la lettura delle attività è lenta (20–60 s misurati con `Get-ScheduledTask`
//! e `schtasks`): la prima è bloccante, poi il risultato si aggiorna ogni 10 minuti in
//! un thread, senza fermare il giro del motore.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Tempo massimo per la lettura delle attività pianificate.
const TASK_QUERY_TIMEOUT: Duration = Duration::from_secs(120);
/// Ogni quanto si rilegge l'elenco delle attività.
const TASK_REFRESH: Duration = Duration::from_secs(600);
/// Id del conflitto «attività non verificabili»: sospende, ignorabile da config.
pub const TASKS_UNVERIFIED: &str = "tasks:unverified";
/// Oltre questa età un risultato verificato non copre più una lettura fallita.
const TASK_RESULT_MAX_AGE: Duration = Duration::from_secs(1800);
/// Tempo massimo per `systemctl --user list-timers`.
const TIMERS_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ConflictKind {
    Process,
    ScheduledTask,
}

/// Un altro motore di cambio account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    /// Chiave stabile, usata da `EngineRules::ignored_conflicts`
    /// (`process:auto`, `task:cswap-guard`, `cron:<hash della riga>`, `timer:<unità>`,
    /// `launchd:<etichetta>`). Mai il testo di una riga di crontab.
    pub id: String,
    pub kind: ConflictKind,
    /// Testo per l'utente.
    pub label: String,
    /// `true` = il motore si sospende; `false` = solo avviso (`tui`, `watch`, attività disabilitata).
    pub certain: bool,
}

/// Fonte dei conflitti (finta nei test del runner).
pub trait ConflictSource {
    fn detect(&mut self) -> Vec<Conflict>;
}

/// Rilevamento reale su processi e attività pianificate (attività in cache).
#[derive(Debug, Default)]
pub struct SystemConflicts {
    tasks: Arc<Mutex<TaskCache>>,
}

#[derive(Debug, Default)]
struct TaskCache {
    result: Option<Vec<Conflict>>,
    fetched: Option<Instant>,
    /// Ultima lettura riuscita.
    verified: Option<Instant>,
    running: bool,
}

impl TaskCache {
    /// Una lettura fallita tiene l'ultimo risultato verificato solo se recente.
    fn store(&mut self, found: Option<Vec<Conflict>>) {
        let now = Instant::now();
        self.fetched = Some(now);
        self.running = false;
        match found {
            Some(v) => {
                self.result = Some(v);
                self.verified = Some(now);
            }
            None if self
                .verified
                .is_some_and(|t| now.duration_since(t) < TASK_RESULT_MAX_AGE) => {}
            None => self.result = Some(vec![unverified()]),
        }
    }
}

fn unverified() -> Conflict {
    Conflict {
        id: TASKS_UNVERIFIED.to_owned(),
        kind: ConflictKind::ScheduledTask,
        label: "attività pianificate non verificabili".to_owned(),
        certain: true,
    }
}

impl SystemConflicts {
    fn tasks(&self) -> Vec<Conflict> {
        let mut cache = self.tasks.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.fetched.is_none() && !cache.running {
            // Primo giro: si aspetta, per non agire prima di sapere se c'è un guard.
            drop(cache);
            let found = detect_tasks();
            let mut cache = self.tasks.lock().unwrap_or_else(PoisonError::into_inner);
            cache.store(found);
            return cache.result.clone().unwrap_or_default();
        }
        let stale = cache.fetched.is_some_and(|t| t.elapsed() >= TASK_REFRESH);
        if stale && !cache.running {
            cache.running = true;
            let shared = Arc::clone(&self.tasks);
            std::thread::spawn(move || {
                let found = detect_tasks();
                shared
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .store(found);
            });
        }
        cache.result.clone().unwrap_or_default()
    }
}

impl ConflictSource for SystemConflicts {
    fn detect(&mut self) -> Vec<Conflict> {
        let mut out = detect_processes();
        out.extend(self.tasks());
        dedup(out)
    }
}

fn dedup(items: Vec<Conflict>) -> Vec<Conflict> {
    let mut out: Vec<Conflict> = Vec::new();
    for c in items {
        match out.iter_mut().find(|o| o.id.eq_ignore_ascii_case(&c.id)) {
            Some(o) => o.certain |= c.certain,
            None => out.push(c),
        }
    }
    out
}

/// Sottocomando di cswap in una riga di comando: `Some((nome, certo))` se è un motore.
pub fn classify_cmdline(args: &[String]) -> Option<(&'static str, bool)> {
    let pos = args.iter().position(|a| is_cswap_program(a))?;
    let sub = args[pos + 1..].iter().find(|a| !a.starts_with('-'));
    match sub.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("auto") => Some(("auto", true)),
        Some("tui") => Some(("tui", false)),
        Some("watch") => Some(("watch", false)),
        Some("menubar") => Some(("menubar", false)),
        None => Some(("cswap", false)),
        Some(_) => None,
    }
}

fn is_cswap_program(arg: &str) -> bool {
    let name = arg.rsplit(['/', '\\']).next().unwrap_or(arg);
    let lower = name.to_ascii_lowercase();
    let stem = lower
        .strip_suffix(".exe")
        .or_else(|| lower.strip_suffix(".py"))
        .unwrap_or(&lower);
    stem == "cswap" || stem == "claude-swap"
}

/// Processi `cswap` dell'utente corrente che fanno girare un motore.
pub fn detect_processes() -> Vec<Conflict> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::Always)
            .with_user(UpdateKind::Always),
    );
    let me = sysinfo::get_current_pid().ok();
    let my_user = me
        .and_then(|p| sys.process(p))
        .and_then(|p| p.user_id().cloned());
    let mut out = Vec::new();
    for (pid, p) in sys.processes() {
        if Some(*pid) == me {
            continue;
        }
        if let (Some(mine), Some(theirs)) = (&my_user, p.user_id()) {
            if mine != theirs {
                continue;
            }
        }
        let args: Vec<String> = p
            .cmd()
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        if let Some(c) = classify_process(&args, &p.name().to_string_lossy(), pid.as_u32()) {
            out.push(c);
        }
    }
    out
}

/// Conflitto di un processo da riga di comando e nome. Riga illeggibile (vuota) con nome
/// `cswap`/`claude-swap`: conflitto possibile (solo avviso).
pub fn classify_process(args: &[String], name: &str, pid: u32) -> Option<Conflict> {
    if args.is_empty() {
        return is_cswap_program(name).then(|| Conflict {
            id: "process:unreadable".to_owned(),
            kind: ConflictKind::Process,
            label: format!("cswap con riga di comando non leggibile (PID {pid})"),
            certain: false,
        });
    }
    classify_cmdline(args).map(|(sub, certain)| Conflict {
        id: format!("process:{sub}"),
        kind: ConflictKind::Process,
        label: format!("cswap {sub} (PID {pid})"),
        certain,
    })
}

/// Attività pianificate dell'utente che citano cswap; `None` se la lettura non è verificata.
pub fn detect_tasks() -> Option<Vec<Conflict>> {
    if cfg!(windows) {
        detect_windows_tasks()
    } else if cfg!(target_os = "macos") {
        let c = run_capture(Path::new("/bin/launchctl"), &["list"], TASK_QUERY_TIMEOUT)?;
        c.success.then(|| parse_launchctl(&c.stdout))
    } else {
        let mut out = detect_crontab()?;
        out.extend(detect_timers()?);
        Some(dedup(out))
    }
}

/// Windows: tutte le attività visibili, filtrate su nome, `Execute` e `Arguments`.
/// Stampa solo percorso, nome e stato (mai gli argomenti). Uscita 2 = lettura fallita.
fn detect_windows_tasks() -> Option<Vec<Conflict>> {
    const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
        [Console]::OutputEncoding = [Text.Encoding]::UTF8; \
        try { $all = @(Get-ScheduledTask) } catch { exit 2 }; \
        foreach ($t in $all) { \
          $text = [string]$t.TaskName; \
          foreach ($a in @($t.Actions)) { $text += ' ' + [string]$a.Execute + ' ' + [string]$a.Arguments }; \
          if ($text -match 'cswap|claude-swap') { $t.TaskPath + $t.TaskName + [char]9 + $t.State } \
        }; \
        exit 0";
    let exe = std::env::var_os("SystemRoot")
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let c = run_capture(
        &exe,
        &["-NoProfile", "-NonInteractive", "-Command", SCRIPT],
        TASK_QUERY_TIMEOUT,
    )?;
    c.success.then(|| parse_windows_tasks(&c.stdout))
}

/// `crontab -l` (in inglese): senza crontab esce con 1 e «no crontab for …» = nessun conflitto.
fn detect_crontab() -> Option<Vec<Conflict>> {
    let Some(exe) = first_file(&["/usr/bin/crontab", "/bin/crontab"]) else {
        return Some(Vec::new());
    };
    let c = run_capture(exe, &["-l"], TASK_QUERY_TIMEOUT)?;
    if c.success {
        return Some(parse_crontab(&c.stdout));
    }
    let none = c.stdout.trim().is_empty() && c.stderr.to_ascii_lowercase().contains("no crontab");
    none.then(Vec::new)
}

/// Timer di systemd dell'utente: nome del timer o del servizio, oppure `ExecStart` del
/// servizio, che citano cswap. Senza gestore utente (container, WSL) non ce ne sono.
fn detect_timers() -> Option<Vec<Conflict>> {
    let Some(exe) = first_file(&["/usr/bin/systemctl", "/bin/systemctl"]) else {
        return Some(Vec::new());
    };
    if !user_manager_running() {
        return Some(Vec::new());
    }
    let args = [
        "--user",
        "list-timers",
        "--all",
        "--no-pager",
        "--no-legend",
    ];
    let c = run_capture(exe, &args, TIMERS_TIMEOUT)?;
    if !c.success {
        return None;
    }
    let timers = parse_timer_units(&c.stdout);
    if timers.is_empty() {
        return Some(Vec::new());
    }
    let mut show: Vec<&str> = vec!["--user", "show", "--no-pager", "--property=Id,ExecStart"];
    show.extend(timers.iter().take(50).map(|(_, svc)| svc.as_str()));
    let c = run_capture(exe, &show, TIMERS_TIMEOUT)?;
    if !c.success {
        return None;
    }
    Some(match_timers(&timers, &c.stdout))
}

#[cfg(unix)]
fn user_manager_running() -> bool {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
        || PathBuf::from(format!("/run/user/{}", rustix::process::getuid().as_raw())),
        PathBuf::from,
    );
    dir.join("systemd").is_dir()
}

#[cfg(not(unix))]
fn user_manager_running() -> bool {
    false
}

fn first_file(candidates: &[&'static str]) -> Option<&'static Path> {
    candidates
        .iter()
        .copied()
        .map(Path::new)
        .find(|p| p.is_file())
}

fn mentions_cswap(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("cswap") || lower.contains("claude-swap")
}

/// Righe `\percorso\nome<TAB>Stato` dello script delle attività.
pub fn parse_windows_tasks(text: &str) -> Vec<Conflict> {
    let out = text
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(name, _)| name.starts_with('\\'))
        .map(|(name, state)| Conflict {
            id: format!("task:{}", name.trim_start_matches('\\')),
            kind: ConflictKind::ScheduledTask,
            label: format!("attività pianificata {name}"),
            // Un'attività disabilitata non gira: solo avviso.
            certain: !state.trim().eq_ignore_ascii_case("disabled"),
        })
        .collect();
    dedup(out)
}

/// `crontab -l`: righe attive che citano cswap. La riga non esce mai da qui: id con
/// l'hash della riga, etichetta ridotta al sottocomando di cswap.
pub fn parse_crontab(text: &str) -> Vec<Conflict> {
    let out = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| mentions_cswap(l))
        .map(|l| {
            let line = l.split_whitespace().collect::<Vec<_>>().join(" ");
            Conflict {
                id: format!("cron:{:016x}", fnv1a(line.as_bytes())),
                kind: ConflictKind::ScheduledTask,
                label: cron_label(&line),
                certain: true,
            }
        })
        .collect();
    dedup(out)
}

/// `crontab: cswap <sottocomando>` se la riga lancia cswap, altrimenti un testo fisso.
fn cron_label(line: &str) -> String {
    let tokens: Vec<&str> = line
        .split_whitespace()
        .map(|t| t.trim_matches(['"', '\'']))
        .collect();
    let Some(pos) = tokens.iter().position(|t| is_cswap_program(t)) else {
        return "crontab: riga che cita cswap".to_owned();
    };
    let sub = tokens[pos + 1..]
        .iter()
        .find(|t| !t.starts_with('-'))
        .filter(|t| t.len() <= 16 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    match sub {
        Some(s) => format!("crontab: cswap {}", s.to_ascii_lowercase()),
        None => "crontab: cswap".to_owned(),
    }
}

/// FNV-1a a 64 bit: stabile tra versioni e piattaforme (gli id finiscono in config).
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// `systemctl --user list-timers --no-legend`: coppie (timer, servizio attivato).
pub fn parse_timer_units(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let tokens: Vec<&str> = l.split_whitespace().collect();
            let i = tokens.iter().rposition(|t| t.ends_with(".timer"))?;
            let svc = tokens.get(i + 1).filter(|s| s.contains('.'))?;
            Some((tokens[i].to_owned(), (*svc).to_owned()))
        })
        .collect()
}

/// Timer che citano cswap nel nome o nell'`ExecStart` del servizio (`systemctl show`).
pub fn match_timers(timers: &[(String, String)], show: &str) -> Vec<Conflict> {
    let mut exec_of: Vec<(&str, &str)> = Vec::new();
    let mut id = "";
    for line in show.lines() {
        if let Some(v) = line.strip_prefix("Id=") {
            id = v.trim();
        } else if let Some(v) = line.strip_prefix("ExecStart=") {
            exec_of.push((id, v));
        }
    }
    let out = timers
        .iter()
        .filter(|(timer, svc)| {
            mentions_cswap(timer)
                || mentions_cswap(svc)
                || exec_of.iter().any(|(i, e)| i == svc && mentions_cswap(e))
        })
        .map(|(timer, _)| Conflict {
            id: format!("timer:{timer}"),
            kind: ConflictKind::ScheduledTask,
            label: format!("timer systemd {timer}"),
            certain: true,
        })
        .collect();
    dedup(out)
}

/// `launchctl list`: `PID\tStato\tEtichetta`.
pub fn parse_launchctl(text: &str) -> Vec<Conflict> {
    let out = text
        .lines()
        .filter_map(|l| l.split('\t').nth(2))
        .map(str::trim)
        .filter(|label| mentions_cswap(label))
        .map(|label| Conflict {
            id: format!("launchd:{label}"),
            kind: ConflictKind::ScheduledTask,
            label: format!("launchd {label}"),
            certain: true,
        })
        .collect();
    dedup(out)
}

/// Esito di un comando di sola lettura.
struct Captured {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Esegue un comando di sola lettura (messaggi in inglese) con tempo massimo;
/// `None` se non parte o scade.
fn run_capture(program: &Path, args: &[&str], timeout: Duration) -> Option<Captured> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().ok()?;
    let out = drain(child.stdout.take()?);
    let err = drain(child.stderr.take()?);
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let text = |h: std::thread::JoinHandle<Vec<u8>>| {
        h.join()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .ok()
    };
    Some(Captured {
        success: status.success(),
        stdout: text(out)?,
        stderr: text(err).unwrap_or_default(),
    })
}

/// Legge una pipe fino alla fine in un thread (al massimo 1 MiB).
fn drain(pipe: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.take(1 << 20).read_to_end(&mut buf);
        buf
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn classifies_command_lines() {
        let cases = [
            (
                r"C:\Users\u\.local\bin\cswap.exe auto --interval 60",
                Some(("auto", true)),
            ),
            (
                r"python.exe C:\Users\u\.local\bin\cswap.exe --json auto",
                Some(("auto", true)),
            ),
            (
                "/home/u/.local/bin/claude-swap auto --once",
                Some(("auto", true)),
            ),
            ("cswap tui", Some(("tui", false))),
            ("cswap watch", Some(("watch", false))),
            ("cswap menubar", Some(("menubar", false))),
            ("cswap", Some(("cswap", false))),
            ("cswap list --json", None),
            ("cswap switch 3 --json", None),
            ("swapdeck --headless", None),
            ("notcswap auto", None),
        ];
        for (line, expect) in cases {
            assert_eq!(classify_cmdline(&args(line)), expect, "{line}");
        }
    }

    #[test]
    fn parses_windows_tasks() {
        let text = "\r\n\\cswap-guard\tReady\r\n\\Tools\\old-cswap\tDisabled\r\nWARNING: x\r\n\\cswap-guard\tRunning\r\n";
        let got = parse_windows_tasks(text);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].id, "task:cswap-guard");
        assert!(got[0].certain);
        assert_eq!(got[1].id, "task:Tools\\old-cswap");
        assert!(!got[1].certain);
    }

    #[test]
    fn failed_task_query_suspends_and_keeps_only_a_recent_result() {
        let mut cache = TaskCache::default();
        cache.store(None);
        let first = cache.result.clone().unwrap();
        assert_eq!(first[0].id, TASKS_UNVERIFIED);
        assert!(first[0].certain, "lettura non verificata: si sospende");
        cache.store(Some(Vec::new()));
        cache.store(None);
        assert!(
            cache.result.clone().unwrap().is_empty(),
            "risultato recente tenuto"
        );
        cache.verified = Instant::now().checked_sub(TASK_RESULT_MAX_AGE);
        cache.store(None);
        assert_eq!(cache.result.unwrap()[0].id, TASKS_UNVERIFIED);
    }

    #[test]
    fn unverified_tasks_suspend_unless_ignored() {
        use crate::engine::tests::{acc, list, user_rules};
        use crate::engine::{Decision, Observation, SuspendReason, decide};
        let l = list(Some(6), vec![acc(6, 10.0, 1.0)]);
        let found = vec![unverified()];
        let obs = Observation {
            list: &l,
            conflicts: &found,
        };
        let mut rules = user_rules();
        assert!(matches!(
            decide(obs, &rules, &Default::default(), 0),
            Decision::Suspended {
                reason: SuspendReason::Conflict,
                ..
            }
        ));
        rules.ignored_conflicts = vec![TASKS_UNVERIFIED.to_owned()];
        assert!(!matches!(
            decide(obs, &rules, &Default::default(), 0),
            Decision::Suspended { .. }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn capture_reports_exit_status() {
        let sh = Path::new("/bin/sh");
        let ok = run_capture(sh, &["-c", "echo hi"], Duration::from_secs(5)).unwrap();
        assert!(ok.success);
        assert_eq!(ok.stdout.trim(), "hi");
        let script = "echo no crontab for u >&2; exit 1";
        let ko = run_capture(sh, &["-c", script], Duration::from_secs(5)).unwrap();
        assert!(!ko.success);
        assert!(ko.stderr.contains("no crontab"));
        assert!(run_capture(sh, &["-c", "sleep 5"], Duration::from_millis(200)).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn capture_reports_exit_status() {
        let cmd = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join(r"System32\cmd.exe");
        let ok = run_capture(&cmd, &["/C", "echo hi"], Duration::from_secs(10)).unwrap();
        assert!(ok.success);
        assert_eq!(ok.stdout.trim(), "hi");
        let ko = run_capture(&cmd, &["/C", "exit 2"], Duration::from_secs(10)).unwrap();
        assert!(!ko.success);
    }

    #[test]
    fn unreadable_cswap_process_is_a_possible_conflict() {
        let c = classify_process(&[], "cswap.exe", 42).unwrap();
        assert_eq!(c.id, "process:unreadable");
        assert!(!c.certain);
        assert!(classify_process(&[], "claude-swap", 42).is_some());
        assert!(classify_process(&[], "notepad.exe", 42).is_none());
        let auto = classify_process(&args("cswap auto"), "cswap", 1).unwrap();
        assert_eq!(auto.id, "process:auto");
    }

    #[test]
    fn parses_crontab_and_launchctl() {
        let cron = "# m h dom mon dow command\n*/2 * * * *   /home/u/.local/bin/cswap auto --once\n0 3 * * * backup.sh\n*/5 * * * * TOKEN=segreto /home/u/guard-cswap.sh\n";
        let got = parse_crontab(cron);
        assert_eq!(got.len(), 2);
        // Id stabile (hash della riga normalizzata) ed etichetta ridotta al comando.
        let line = "*/2 * * * * /home/u/.local/bin/cswap auto --once";
        assert_eq!(got[0].id, format!("cron:{:016x}", fnv1a(line.as_bytes())));
        assert_eq!(got[0].label, "crontab: cswap auto");
        assert_eq!(got[1].label, "crontab: riga che cita cswap");
        for c in &got {
            assert!(!c.id.contains("home") && !c.label.contains("home"));
            assert!(!c.label.contains("segreto"));
        }
        let launch = "PID\tStatus\tLabel\n-\t0\tcom.apple.x\n123\t0\tit.example.cswap-guard\n";
        let got = parse_launchctl(launch);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "launchd:it.example.cswap-guard");
    }

    #[test]
    fn parses_systemd_timers() {
        let list = "Fri 2026-10-16 12:00:00 CEST 4min left Fri 2026-10-16 11:58:00 CEST 1min ago guard.timer guard.service\n\
                    - - - - old-cswap.timer old-cswap.service\n\
                    Sat 2026-10-17 00:00:00 CEST 12h left n/a n/a backup.timer backup.service\n";
        let timers = parse_timer_units(list);
        assert_eq!(timers.len(), 3);
        assert_eq!(
            timers[0],
            ("guard.timer".to_owned(), "guard.service".to_owned())
        );
        let show = "Id=guard.service\nExecStart={ path=/home/u/.local/bin/cswap ; argv[]=/home/u/.local/bin/cswap auto --once ; }\n\nId=old-cswap.service\nExecStart=\n\nId=backup.service\nExecStart={ path=/usr/bin/rsync ; }\n";
        let got = match_timers(&timers, show);
        let ids: Vec<&str> = got.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["timer:guard.timer", "timer:old-cswap.timer"]);
        assert!(got.iter().all(|c| c.certain && !c.label.contains("home")));
    }

    /// Una sola lettura sul PC reale: deve girare senza errori e senza admin.
    #[test]
    #[ignore = "legge processi e attività del PC reale"]
    fn detects_on_real_machine() {
        let found = SystemConflicts::default().detect();
        for c in &found {
            println!("{} certain={} {}", c.id, c.certain, c.label);
        }
        // Sul PC dell'utente gira l'attività `cswap-guard`: il motore va sospeso.
        assert!(
            found
                .iter()
                .any(|c| c.id == "task:cswap-guard" && c.certain)
        );
        let rules = crate::engine::tests::user_rules();
        let l = crate::engine::tests::list(Some(6), vec![crate::engine::tests::acc(6, 99.0, 1.0)]);
        let obs = crate::engine::Observation {
            list: &l,
            conflicts: &found,
        };
        assert!(matches!(
            crate::engine::decide(obs, &rules, &Default::default(), 0),
            crate::engine::Decision::Suspended {
                reason: crate::engine::SuspendReason::Conflict,
                ..
            }
        ));
    }
}
