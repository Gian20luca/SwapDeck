//! Runner senza interfaccia (F10): `swapdeck --headless [--once] [--dry-run]`.
//!
//! A ogni giro: battito del lock, `cswap list`, campioni nello storico, conflitti,
//! decisione del motore e, se acceso e non in osservazione, il cambio. Solo chi tiene il
//! lock scrive lo storico e cambia account; gli altri restano in attesa del lock.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::conflicts::{Conflict, ConflictSource};
use crate::cswap::{CswapError, ListResponse, SwitchResponse};
use crate::engine::{
    Decision, EngineMemory, EngineRules, IdleReason, MAX_INTERVAL_SECS, MIN_INTERVAL_SECS,
    Observation, SuspendReason, SwitchCause, decide,
};
use crate::history::{
    DEFAULT_RETENTION_DAYS, DecisionRecord, History, HistoryRecord, HistoryWriter, SwitchRecord,
    create_private_dir,
};
use crate::lock::{self, Acquire, LockGuard, LockInfo};
use crate::timeutil::{format_iso, now_unix};

/// Memoria del motore nella cartella dati.
pub const MEMORY_FILE: &str = "engine-memory.json";

/// Accesso a cswap usato dal motore. L'adattatore reale lo fornisce il client (step 1).
pub trait CswapPort {
    /// `cswap list --json`.
    fn list(&mut self) -> Result<ListResponse, CswapError>;
    /// `cswap switch <number> --json`.
    fn switch_to(&mut self, number: u32) -> Result<SwitchResponse, CswapError>;
}

/// Orologio del runner (finto nei test).
pub trait Clock {
    fn now(&mut self) -> i64;
    /// Attende `secs` secondi o finché `stop` diventa vero.
    fn sleep(&mut self, secs: u32, stop: &AtomicBool);
}

/// Orologio di sistema; l'attesa controlla `stop` ogni 200 ms.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&mut self) -> i64 {
        now_unix()
    }

    fn sleep(&mut self, secs: u32, stop: &AtomicBool) {
        let end = Instant::now() + Duration::from_secs(u64::from(secs));
        while !stop.load(Ordering::SeqCst) {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            std::thread::sleep(left.min(Duration::from_millis(200)));
        }
    }
}

/// Opzioni del runner.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub data_dir: PathBuf,
    pub rules: EngineRules,
    /// Un solo giro.
    pub once: bool,
    /// Solo osservazione, anche se le regole dicono altro.
    pub dry_run: bool,
    pub retention_days: u32,
    /// Limite di giri (test); `None` = fino a `stop`.
    pub max_ticks: Option<u64>,
}

impl RunOptions {
    pub fn new(data_dir: PathBuf, rules: EngineRules) -> Self {
        Self {
            data_dir,
            rules,
            once: false,
            dry_run: false,
            retention_days: DEFAULT_RETENTION_DAYS,
            max_ticks: None,
        }
    }
}

/// Dipendenze del runner.
pub struct Runtime<'a> {
    pub port: &'a mut dyn CswapPort,
    pub conflicts: &'a mut dyn ConflictSource,
    pub clock: &'a mut dyn Clock,
    pub out: &'a mut dyn Write,
    pub stop: &'a AtomicBool,
}

/// Esito di [`run`].
#[derive(Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Finished {
        ticks: u64,
        switches: u32,
    },
    /// `--once` con il lock tenuto da un'altra istanza.
    LockBusy(Option<LockInfo>),
}

/// Testo per l'utente di una decisione.
pub fn describe(d: &Decision) -> String {
    let cause = |c: &SwitchCause| match c {
        SwitchCause::PrimaryAtLimit => "principale al limite",
        SwitchCause::ReserveAtLimit => "riserva al limite",
        SwitchCause::ActiveNeedsRelogin => "account attivo da riautenticare",
        SwitchCause::PrimaryReset => "principale resettato",
        SwitchCause::ReservesAtLimit => "riserve al limite",
    };
    match d {
        Decision::NoAction { reason } => match reason {
            IdleReason::BelowThreshold => "ok: principale sotto soglia",
            IdleReason::OnReserve => "ok: su riserva, principale non ancora disponibile",
            IdleReason::Cooldown => "attesa: pausa minima tra due cambi",
            IdleReason::StaleUsage => "attesa: consumi dell'account attivo vecchi o sconosciuti",
            IdleReason::ActiveExcluded => "nessuna azione: account attivo escluso",
            IdleReason::ActiveOutsideRules => "nessuna azione: account attivo fuori dalle regole",
        }
        .to_owned(),
        Decision::Switch { from, to, cause: c }
        | Decision::ReturnToPrimary { from, to, cause: c } => {
            format!("cambio {from} -> {to}: {}", cause(c))
        }
        Decision::Suspended { reason, detail } => {
            let r = match reason {
                SuspendReason::Disabled => "motore spento",
                SuspendReason::InvalidRules => "regole non valide",
                SuspendReason::Conflict => "altro motore attivo",
                SuspendReason::NoActiveAccount => "nessun account attivo gestito",
                SuspendReason::NoViableTarget => "nessun account utilizzabile",
            };
            if detail.is_empty() {
                format!("sospeso: {r}")
            } else {
                format!("sospeso: {r} ({})", detail.join(", "))
            }
        }
    }
}

fn load_memory(dir: &Path) -> EngineMemory {
    fs::read(dir.join(MEMORY_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_memory(dir: &Path, mem: &EngineMemory) -> io::Result<()> {
    let mut tmp = tempfile::Builder::new()
        .prefix(".engine-memory.")
        .tempfile_in(dir)?;
    tmp.write_all(&serde_json::to_vec(mem).map_err(io::Error::other)?)?;
    tmp.as_file().sync_all()?;
    tmp.persist(dir.join(MEMORY_FILE)).map_err(|e| e.error)?;
    Ok(())
}

struct Holder {
    guard: LockGuard,
    writer: HistoryWriter,
    mem: EngineMemory,
}

struct Runner<'o, 'r> {
    opts: &'o RunOptions,
    rules: EngineRules,
    rt: Runtime<'r>,
    holder: Option<Holder>,
    last_decision: Option<Decision>,
    last_warnings: Vec<String>,
    switches: u32,
}

impl Runner<'_, '_> {
    fn log(&mut self, now: i64, msg: &str) {
        // Un errore di stdout (pipe chiusa) non deve fermare il motore.
        let _ = writeln!(self.rt.out, "{} {msg}", format_iso(now));
        let _ = self.rt.out.flush();
    }

    fn try_lock(&mut self, now: i64) -> io::Result<Option<Option<LockInfo>>> {
        match lock::try_acquire(&self.opts.data_dir, self.rules.interval_secs, now)? {
            Acquire::Acquired(guard) => {
                let history = History::new(&self.opts.data_dir);
                let writer = history.writer(&guard, self.opts.retention_days)?;
                let mem = load_memory(&self.opts.data_dir);
                self.holder = Some(Holder { guard, writer, mem });
                // La decisione vista in attesa del lock va registrata di nuovo da titolare.
                self.last_decision = None;
                self.log(now, "lock del motore acquisito");
                Ok(None)
            }
            Acquire::Busy(info) => Ok(Some(info)),
        }
    }

    fn beat(&mut self, now: i64) {
        let Some(h) = self.holder.as_mut() else {
            return;
        };
        match h.guard.heartbeat(now) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                self.holder = None;
                self.log(now, "lock perso: un'altra istanza ha preso il motore");
            }
            Err(e) => self.log(now, &format!("battito del lock non riuscito: {e}")),
        }
    }

    fn note_conflicts(&mut self, now: i64, conflicts: &[Conflict]) {
        let mut warnings: Vec<String> = conflicts
            .iter()
            .filter(|c| !c.certain)
            .map(|c| c.label.clone())
            .collect();
        warnings.sort();
        if warnings != self.last_warnings {
            if !warnings.is_empty() {
                self.log(
                    now,
                    &format!("avviso: possibile conflitto con {}", warnings.join(", ")),
                );
            }
            self.last_warnings = warnings;
        }
    }

    fn tick(&mut self, now: i64) {
        // Prima i conflitti (la lettura delle attività può durare a lungo), poi `list`:
        // la decisione usa consumi appena letti.
        let conflicts = self.rt.conflicts.detect();
        self.note_conflicts(now, &conflicts);
        let list = match self.rt.port.list() {
            Ok(l) => l,
            Err(e) => {
                self.log(now, &format!("cswap list non riuscito: {e}"));
                return;
            }
        };
        if let Some(h) = self.holder.as_mut() {
            if let Err(e) = h.writer.record_samples(&list, now) {
                self.log(now, &format!("storico non scritto: {e}"));
            }
        }
        let mem = self
            .holder
            .as_ref()
            .map(|h| h.mem.clone())
            .unwrap_or_default();
        let obs = Observation {
            list: &list,
            conflicts: &conflicts,
        };
        let decision = decide(obs, &self.rules, &mem, now);
        if self.last_decision.as_ref() != Some(&decision) {
            let prefix = if self.holder.is_none() {
                "[in attesa del lock] "
            } else if self.rules.dry_run && decision.target().is_some() {
                "[solo osservazione] "
            } else {
                ""
            };
            self.log(now, &format!("{prefix}{}", describe(&decision)));
            let record = HistoryRecord::Decision(DecisionRecord {
                ts: now,
                decision: decision.clone(),
                dry_run: self.rules.dry_run,
            });
            if let Some(h) = self.holder.as_mut() {
                if let Err(e) = h.writer.append(&record) {
                    self.log(now, &format!("storico non scritto: {e}"));
                }
            }
            self.last_decision = Some(decision.clone());
        }
        if self.rules.enabled && !self.rules.dry_run {
            if let (Some(to), Some(cause)) = (decision.target(), cause_of(&decision)) {
                self.execute(now, &list, to, cause);
            }
        }
    }

    fn report_prune_error(&mut self, now: i64) {
        let err = self
            .holder
            .as_mut()
            .and_then(|h| h.writer.take_prune_error());
        if let Some(e) = err {
            self.log(now, &format!("rotazione dello storico non riuscita: {e}"));
        }
    }

    fn execute(&mut self, now: i64, list: &ListResponse, to: u32, cause: SwitchCause) {
        let Some(h) = self.holder.as_mut() else {
            return;
        };
        if !h.guard.still_owned() {
            self.holder = None;
            self.log(now, "lock perso prima del cambio: nessuna azione");
            return;
        }
        let from = list.active_account_number;
        let (switched, message) = match self.rt.port.switch_to(to) {
            Ok(r) => (r.switched, format!("{:?}", r.reason)),
            Err(e) => (false, e.to_string()),
        };
        if switched {
            h.mem.record_switch(&self.rules, list, to, now);
        } else {
            // Anche un tentativo fallito rispetta la pausa minima: niente raffiche.
            h.mem.last_switch_at = Some(now);
        }
        let mem_result = save_memory(&self.opts.data_dir, &h.mem);
        let rec = HistoryRecord::Switch(SwitchRecord {
            ts: now,
            from,
            to,
            cause,
            switched,
            message: message.clone(),
        });
        let hist_result = h.writer.append(&rec);
        let from_s = from.map_or_else(|| "?".to_owned(), |n| n.to_string());
        if switched {
            self.switches += 1;
            self.log(now, &format!("CAMBIO ESEGUITO {from_s} -> {to}"));
        } else {
            self.log(
                now,
                &format!("cambio {from_s} -> {to} non eseguito: {message}"),
            );
        }
        if let Err(e) = mem_result.and(hist_result) {
            self.log(now, &format!("stato del motore non salvato: {e}"));
        }
    }
}

fn cause_of(d: &Decision) -> Option<SwitchCause> {
    match d {
        Decision::Switch { cause, .. } | Decision::ReturnToPrimary { cause, .. } => Some(*cause),
        _ => None,
    }
}

/// Ciclo del motore senza interfaccia. Rilascia il lock all'uscita.
pub fn run(opts: &RunOptions, rt: Runtime<'_>) -> io::Result<RunOutcome> {
    create_private_dir(&opts.data_dir)?;
    let mut rules = opts.rules.clone();
    rules.dry_run |= opts.dry_run;
    let interval = rules
        .interval_secs
        .clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS);
    rules.interval_secs = interval;
    let mut r = Runner {
        opts,
        rules,
        rt,
        holder: None,
        last_decision: None,
        last_warnings: Vec::new(),
        switches: 0,
    };
    let mut ticks = 0u64;
    let mut busy_logged = false;
    loop {
        let now = r.rt.clock.now();
        if r.holder.is_some() {
            r.beat(now);
        } else if let Some(info) = r.try_lock(now)? {
            if opts.once {
                return Ok(RunOutcome::LockBusy(info));
            }
            if !busy_logged {
                busy_logged = true;
                let who = info.map_or_else(
                    || "un'altra istanza".to_owned(),
                    |i| format!("PID {} su {}", i.pid, i.host),
                );
                r.log(
                    now,
                    &format!("motore già attivo ({who}): in attesa del lock"),
                );
            }
        } else {
            busy_logged = false;
        }
        r.tick(now);
        r.report_prune_error(now);
        // Secondo battito: il giro può durare molto (prima lettura delle attività, cswap lento).
        if r.holder.is_some() {
            let after = r.rt.clock.now();
            r.beat(after);
        }
        ticks += 1;
        let done = opts.once
            || r.rt.stop.load(Ordering::SeqCst)
            || opts.max_ticks.is_some_and(|m| ticks >= m);
        if done {
            break;
        }
        r.rt.clock.sleep(interval, r.rt.stop);
        if r.rt.stop.load(Ordering::SeqCst) {
            break;
        }
    }
    let switches = r.switches;
    if let Some(h) = r.holder.take() {
        h.guard.release();
    }
    Ok(RunOutcome::Finished { ticks, switches })
}

/// Opzioni di `swapdeck --headless`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CliFlags {
    pub once: bool,
    pub dry_run: bool,
}

pub const USAGE: &str = "uso: swapdeck --headless [--once] [--dry-run]";

/// Opzioni dopo `--headless`.
pub fn parse_flags(args: &[String]) -> Result<CliFlags, String> {
    let mut f = CliFlags::default();
    for a in args {
        match a.as_str() {
            "--once" => f.once = true,
            "--dry-run" => f.dry_run = true,
            other => return Err(format!("opzione sconosciuta: {other}\n{USAGE}")),
        }
    }
    Ok(f)
}

/// Codici di uscita di `--headless`.
pub mod exit_code {
    pub const OK: i32 = 0;
    pub const ERROR: i32 = 1;
    pub const USAGE: i32 = 2;
    pub const LOCK_BUSY: i32 = 3;
}

/// Ingresso di `swapdeck --headless`: `args` sono le opzioni dopo `--headless`.
pub fn cli_main(args: &[String], stop: &AtomicBool) -> i32 {
    let flags = match parse_flags(args) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("{msg}");
            return exit_code::USAGE;
        }
    };
    let _ = (flags, stop);
    // Integrazione con lo step 1: cartella dati, `config.json` (sezione `engine`) e
    // adattatore `CswapPort` del client cswap. Poi:
    // run(&opts, Runtime { port, conflicts: &mut SystemConflicts, clock: &mut SystemClock,
    //     out: &mut io::stdout(), stop })
    eprintln!("swapdeck --headless: client cswap e config non ancora collegati");
    exit_code::ERROR
}

#[cfg(test)]
mod tests;
