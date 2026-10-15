//! Rilevamento dei conflitti (F11): altri motori che cambiano account da soli.
//!
//! cswap non ha file PID: si guardano l'elenco dei processi (`cswap auto`, `tui`, `watch`,
//! `menubar`) e le attività pianificate con «cswap» nel nome (`Get-ScheduledTask`,
//! `crontab -l`, `launchctl list`). Tutto senza privilegi di amministratore.
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
/// Id del conflitto «attività non verificabili» (solo avviso).
pub const TASKS_UNVERIFIED: &str = "tasks:unverified";

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
    /// (`process:auto`, `task:cswap-guard`, `cron:<riga>`, `launchd:<etichetta>`).
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
    running: bool,
}

impl TaskCache {
    fn store(&mut self, found: Option<Vec<Conflict>>) {
        self.fetched = Some(Instant::now());
        self.running = false;
        match found {
            Some(v) => self.result = Some(v),
            None if self.result.is_none() => self.result = Some(vec![unverified()]),
            None => {}
        }
    }
}

fn unverified() -> Conflict {
    Conflict {
        id: TASKS_UNVERIFIED.to_owned(),
        kind: ConflictKind::ScheduledTask,
        label: "attività pianificate non verificabili".to_owned(),
        certain: false,
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
        if let Some((sub, certain)) = classify_cmdline(&args) {
            out.push(Conflict {
                id: format!("process:{sub}"),
                kind: ConflictKind::Process,
                label: format!("cswap {sub} (PID {pid})"),
                certain,
            });
        }
    }
    out
}

/// Attività pianificate dell'utente con «cswap» nel nome; `None` se la lettura fallisce.
pub fn detect_tasks() -> Option<Vec<Conflict>> {
    if cfg!(windows) {
        // Nomi e stati non localizzati, a differenza di `schtasks`.
        const SCRIPT: &str = "Get-ScheduledTask -TaskName '*cswap*','*claude-swap*' \
            -ErrorAction SilentlyContinue | ForEach-Object { $_.TaskPath + $_.TaskName + \
            [char]9 + $_.State }";
        let exe = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
            .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        run_capture(&exe, &["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
            .map(|t| parse_windows_tasks(&t))
    } else if cfg!(target_os = "macos") {
        run_capture(Path::new("/bin/launchctl"), &["list"]).map(|t| parse_launchctl(&t))
    } else {
        let Some(exe) = ["/usr/bin/crontab", "/bin/crontab"]
            .iter()
            .map(Path::new)
            .find(|p| p.is_file())
        else {
            return Some(Vec::new());
        };
        // Senza crontab `crontab -l` esce con 1 e non stampa nulla: nessun conflitto.
        run_capture(exe, &["-l"]).map(|t| parse_crontab(&t))
    }
}

/// Righe `\percorso\nome<TAB>Stato` di `Get-ScheduledTask`.
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

/// `crontab -l`: righe attive che citano cswap.
pub fn parse_crontab(text: &str) -> Vec<Conflict> {
    let out = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter(|l| l.to_ascii_lowercase().contains("cswap") || l.contains("claude-swap"))
        .map(|l| {
            let line = l.split_whitespace().collect::<Vec<_>>().join(" ");
            Conflict {
                id: format!("cron:{line}"),
                kind: ConflictKind::ScheduledTask,
                label: format!("crontab: {line}"),
                certain: true,
            }
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
        .filter(|label| {
            let lower = label.to_ascii_lowercase();
            lower.contains("cswap") || lower.contains("claude-swap")
        })
        .map(|label| Conflict {
            id: format!("launchd:{label}"),
            kind: ConflictKind::ScheduledTask,
            label: format!("launchd {label}"),
            certain: true,
        })
        .collect();
    dedup(out)
}

/// Esegue un comando di sola lettura con tempo massimo; `None` se fallisce o scade.
fn run_capture(program: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + TASK_QUERY_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let buf = reader.join().ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
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
    fn failed_task_query_is_a_warning_and_keeps_last_result() {
        let mut cache = TaskCache::default();
        cache.store(None);
        let first = cache.result.clone().unwrap();
        assert_eq!(first[0].id, TASKS_UNVERIFIED);
        assert!(!first[0].certain);
        cache.store(Some(parse_windows_tasks("\\cswap-guard\tReady")));
        cache.store(None);
        assert_eq!(cache.result.unwrap()[0].id, "task:cswap-guard");
    }

    #[test]
    fn parses_crontab_and_launchctl() {
        let cron = "# m h dom mon dow command\n*/2 * * * *   /home/u/.local/bin/cswap auto --once\n0 3 * * * backup.sh\n";
        let got = parse_crontab(cron);
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].id,
            "cron:*/2 * * * * /home/u/.local/bin/cswap auto --once"
        );
        let launch = "PID\tStatus\tLabel\n-\t0\tcom.apple.x\n123\t0\tit.example.cswap-guard\n";
        let got = parse_launchctl(launch);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id, "launchd:it.example.cswap-guard");
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
