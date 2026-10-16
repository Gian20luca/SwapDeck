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

use crate::config::{self, MAX_POLL_INTERVAL_SECONDS, MIN_POLL_INTERVAL_SECONDS};
use crate::conflicts::{Conflict, ConflictSource, SystemConflicts};
use crate::cswap::{CswapClient, CswapError, CswapErrorKind, ListResponse, SwitchResponse};
use crate::engine::{
    Candidate, Decision, EngineMemory, EngineRules, IdleReason, Observation, SuspendReason,
    SwitchCause, evaluate,
};
use crate::history::{
    DEFAULT_RETENTION_DAYS, DecisionRecord, History, HistoryRecord, HistoryWriter, SwitchRecord,
};
use crate::lock::{self, Acquire, LockGuard, LockInfo};
use crate::paths::{AppPaths, create_private_dir};
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

impl CswapPort for CswapClient {
    fn list(&mut self) -> Result<ListResponse, CswapError> {
        CswapClient::list(self)
    }

    fn switch_to(&mut self, number: u32) -> Result<SwitchResponse, CswapError> {
        self.switch(Some(number), None, None, false)?
            .switch
            .ok_or_else(|| CswapError::new(CswapErrorKind::Failed, "switch senza risposta JSON"))
    }
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
    /// Intervallo tra due giri (`pollIntervalSeconds` della config), riportato in 60–3600 s.
    pub interval_secs: u32,
    /// Un solo giro.
    pub once: bool,
    /// Solo osservazione, anche se le regole dicono altro.
    pub dry_run: bool,
    pub retention_days: u32,
    /// Limite di giri (test); `None` = fino a `stop`.
    pub max_ticks: Option<u64>,
    /// Config riletta prima di ogni giro di [`run`]: regole e intervallo cambiati dalla UI
    /// valgono senza riavvio. `None` = regole fisse.
    pub config_file: Option<PathBuf>,
}

impl RunOptions {
    pub fn new(data_dir: PathBuf, rules: EngineRules) -> Self {
        Self {
            data_dir,
            rules,
            interval_secs: MIN_POLL_INTERVAL_SECONDS,
            once: false,
            dry_run: false,
            retention_days: DEFAULT_RETENTION_DAYS,
            max_ticks: None,
            config_file: None,
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
        SwitchCause::EarlierAvailable => "utenza precedente di nuovo libera",
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
    interval: u32,
    rt: Runtime<'r>,
    holder: Option<Holder>,
    last_decision: Option<Decision>,
    last_warnings: Vec<String>,
    switches: u32,
    /// Attesa del lock già segnalata.
    busy_logged: bool,
    /// Errore di rilettura della config già segnalato.
    reload_failed: bool,
}

impl Runner<'_, '_> {
    fn log(&mut self, now: i64, msg: &str) {
        // Un errore di stdout (pipe chiusa) non deve fermare il motore.
        let _ = writeln!(self.rt.out, "{} {msg}", format_iso(now));
        let _ = self.rt.out.flush();
    }

    fn try_lock(&mut self, now: i64) -> io::Result<Option<Option<LockInfo>>> {
        match lock::try_acquire(&self.opts.data_dir, self.interval, now)? {
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

    fn tick(&mut self, now: i64) -> Step {
        // Prima i conflitti (la lettura delle attività può durare a lungo), poi `list`:
        // la decisione usa consumi appena letti.
        let conflicts = self.rt.conflicts.detect();
        self.note_conflicts(now, &conflicts);
        let list = match self.rt.port.list() {
            Ok(l) => l,
            Err(e) => {
                self.log(now, &format!("cswap list non riuscito: {e}"));
                return Step {
                    list: Err(e),
                    decision: None,
                    reason: None,
                    candidate: None,
                    holder: self.holder.is_some(),
                    switched_to: None,
                };
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
        let evaluation = evaluate(obs, &self.rules, &mem, now);
        let decision = evaluation.decision;
        if self.last_decision.as_ref() != Some(&decision) {
            let prefix = if self.holder.is_none() {
                "[in attesa del lock] "
            } else if self.rules.dry_run && decision.target().is_some() {
                "[solo osservazione] "
            } else {
                ""
            };
            self.log(
                now,
                &format!("{prefix}{} ({})", describe(&decision), evaluation.reason),
            );
            let record = HistoryRecord::Decision(DecisionRecord {
                ts: now,
                decision: decision.clone(),
                dry_run: self.rules.dry_run,
                why: Some(evaluation.reason.clone()),
            });
            if let Some(h) = self.holder.as_mut() {
                if let Err(e) = h.writer.append(&record) {
                    self.log(now, &format!("storico non scritto: {e}"));
                }
            }
            self.last_decision = Some(decision.clone());
        }
        let mut switched_to = None;
        if self.rules.enabled && !self.rules.dry_run {
            if let (Some(to), Some(cause)) = (decision.target(), cause_of(&decision)) {
                if self.execute(now, &list, to, cause) {
                    switched_to = Some(to);
                }
            }
        }
        Step {
            list: Ok(list),
            decision: Some(decision),
            reason: Some(evaluation.reason),
            candidate: evaluation.candidate,
            holder: self.holder.is_some(),
            switched_to,
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

    /// `true` se il cambio è riuscito.
    fn execute(&mut self, now: i64, list: &ListResponse, to: u32, cause: SwitchCause) -> bool {
        let Some(h) = self.holder.as_mut() else {
            return false;
        };
        if !h.guard.still_owned() {
            self.holder = None;
            self.log(now, "lock perso prima del cambio: nessuna azione");
            return false;
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
        switched
    }
}

fn cause_of(d: &Decision) -> Option<SwitchCause> {
    match d {
        Decision::Switch { cause, .. } | Decision::ReturnToPrimary { cause, .. } => Some(*cause),
        _ => None,
    }
}

impl<'o, 'r> Runner<'o, 'r> {
    fn new(opts: &'o RunOptions, rt: Runtime<'r>) -> io::Result<Self> {
        create_private_dir(&opts.data_dir)?;
        let mut rules = opts.rules.clone();
        rules.dry_run |= opts.dry_run;
        let interval = opts
            .interval_secs
            .clamp(MIN_POLL_INTERVAL_SECONDS, MAX_POLL_INTERVAL_SECONDS);
        Ok(Self {
            opts,
            rules,
            interval,
            rt,
            holder: None,
            last_decision: None,
            last_warnings: Vec::new(),
            switches: 0,
            busy_logged: false,
            reload_failed: false,
        })
    }

    /// Battito del lock se è nostro, altrimenti tentativo di prenderlo.
    /// `Some(info)` se è tenuto da un'altra istanza.
    fn claim(&mut self, now: i64) -> io::Result<Option<Option<LockInfo>>> {
        if self.holder.is_some() {
            self.beat(now);
        } else if let Some(info) = self.try_lock(now)? {
            return Ok(Some(info));
        }
        self.busy_logged = false;
        Ok(None)
    }

    fn note_busy(&mut self, now: i64, info: Option<&LockInfo>) {
        if self.busy_logged {
            return;
        }
        self.busy_logged = true;
        let who = info.map_or_else(
            || "un'altra istanza".to_owned(),
            |i| format!("PID {} su {}", i.pid, i.host),
        );
        self.log(
            now,
            &format!("motore già attivo ({who}): in attesa del lock"),
        );
    }

    /// Dopo il giro: errori di rotazione e secondo battito (il giro può durare molto:
    /// prima lettura delle attività, cswap lento).
    fn finish(&mut self, now: i64) {
        self.report_prune_error(now);
        if self.holder.is_some() {
            let after = self.rt.clock.now();
            self.beat(after);
        }
    }

    fn release(&mut self) {
        if let Some(h) = self.holder.take() {
            h.guard.release();
        }
    }

    fn set_rules(&mut self, rules: &EngineRules) {
        self.rules = rules.clone();
        self.rules.dry_run |= self.opts.dry_run;
    }

    fn set_interval(&mut self, secs: u32) {
        self.interval = secs.clamp(MIN_POLL_INTERVAL_SECONDS, MAX_POLL_INTERVAL_SECONDS);
        if let Some(h) = self.holder.as_mut() {
            h.guard.set_interval(self.interval);
        }
    }

    /// Rilegge regole e intervallo. Una config non leggibile non ferma il motore: restano le
    /// regole di prima (l'errore si scrive una volta sola).
    fn reload(&mut self, path: &Path, now: i64) {
        match config::load(path) {
            Ok(loaded) => {
                self.reload_failed = false;
                let before = (self.rules.clone(), self.interval);
                self.set_rules(&loaded.config.engine);
                self.set_interval(loaded.config.poll_interval_seconds);
                if before != (self.rules.clone(), self.interval) {
                    self.log(now, "regole del motore aggiornate dalla config");
                }
            }
            Err(e) if !self.reload_failed => {
                self.reload_failed = true;
                self.log(
                    now,
                    &format!("config non riletta, restano le regole di prima: {e}"),
                );
            }
            Err(_) => {}
        }
    }
}

/// Esito di un giro del motore.
#[derive(Debug)]
pub struct Step {
    pub list: Result<ListResponse, CswapError>,
    /// Decisione del giro; `None` se la lettura non è riuscita.
    pub decision: Option<Decision>,
    /// Motivo leggibile della decisione.
    pub reason: Option<String>,
    /// Dove andrebbe il motore se l'attiva superasse la soglia adesso.
    pub candidate: Option<Candidate>,
    /// Il lock è di questa istanza: storico, memoria e cambi sono suoi.
    pub holder: bool,
    /// Account su cui il motore ha cambiato in questo giro.
    pub switched_to: Option<u32>,
}

/// Motore a giri singoli per chi ha già un suo ciclo (l'app desktop): a ogni [`Engine::step`]
/// lock (presa o battito), lettura, storico, decisione e, se il lock è suo, il cambio.
/// Senza lock solo osservazione. Il lock si rilascia con [`Engine::release`] o al drop.
pub struct Engine<'o, 'r> {
    runner: Runner<'o, 'r>,
}

impl<'o, 'r> Engine<'o, 'r> {
    pub fn new(opts: &'o RunOptions, rt: Runtime<'r>) -> io::Result<Self> {
        Ok(Self {
            runner: Runner::new(opts, rt)?,
        })
    }

    pub fn step(&mut self) -> Step {
        let r = &mut self.runner;
        let now = r.rt.clock.now();
        match r.claim(now) {
            Ok(None) => {}
            Ok(Some(info)) => r.note_busy(now, info.as_ref()),
            // Cartella dati non scrivibile: si continua in sola osservazione.
            Err(e) => r.log(now, &format!("lock del motore non disponibile: {e}")),
        }
        let step = r.tick(now);
        r.finish(now);
        step
    }

    pub fn release(mut self) {
        self.runner.release();
    }

    /// Regole nuove dal giro successivo, senza riavvio (il `dry_run` delle opzioni resta).
    pub fn set_rules(&mut self, rules: &EngineRules) {
        self.runner.set_rules(rules);
    }

    /// Intervallo nuovo (riportato in 60–3600 s): vale per il battito e la scadenza del lock.
    pub fn set_interval(&mut self, secs: u32) {
        self.runner.set_interval(secs);
    }

    /// Regole in uso (con il `dry_run` delle opzioni).
    pub fn rules(&self) -> &EngineRules {
        &self.runner.rules
    }
}

impl Drop for Engine<'_, '_> {
    fn drop(&mut self) {
        self.runner.release();
    }
}

/// Ciclo del motore senza interfaccia. Rilascia il lock all'uscita.
pub fn run(opts: &RunOptions, rt: Runtime<'_>) -> io::Result<RunOutcome> {
    let mut r = Runner::new(opts, rt)?;
    let mut ticks = 0u64;
    loop {
        let now = r.rt.clock.now();
        if let Some(path) = &opts.config_file {
            r.reload(path, now);
        }
        if let Some(info) = r.claim(now)? {
            if opts.once {
                return Ok(RunOutcome::LockBusy(info));
            }
            r.note_busy(now, info.as_ref());
        }
        r.tick(now);
        r.finish(now);
        ticks += 1;
        let done = opts.once
            || r.rt.stop.load(Ordering::SeqCst)
            || opts.max_ticks.is_some_and(|m| ticks >= m);
        if done {
            break;
        }
        r.rt.clock.sleep(r.interval, r.rt.stop);
        if r.rt.stop.load(Ordering::SeqCst) {
            break;
        }
    }
    let switches = r.switches;
    r.release();
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
/// Cartelle per utente (o `SWAPDECK_HOME`), `config.json` (sezione `engine`,
/// `pollIntervalSeconds`) e cswap trovato come nel client (`SWAPDECK_CSWAP`, `PATH`, …).
pub fn cli_main(args: &[String], stop: &AtomicBool) -> i32 {
    let flags = match parse_flags(args) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("{msg}");
            return exit_code::USAGE;
        }
    };
    let Some(paths) = AppPaths::from_env_or_system() else {
        eprintln!("cartelle dell'utente non determinabili");
        return exit_code::ERROR;
    };
    let loaded = match config::load(&paths.config_file()) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return exit_code::ERROR;
        }
    };
    for w in &loaded.warnings {
        eprintln!("config: {w}");
    }
    let mut client = match CswapClient::locate() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return exit_code::ERROR;
        }
    };
    let mut opts = RunOptions::new(paths.data_dir.clone(), loaded.config.engine.clone());
    opts.interval_secs = loaded.config.poll_interval_seconds;
    opts.once = flags.once;
    opts.dry_run = flags.dry_run;
    opts.config_file = Some(paths.config_file());
    let mut stdout = io::stdout();
    let result = run(
        &opts,
        Runtime {
            port: &mut client,
            conflicts: &mut SystemConflicts::default(),
            clock: &mut SystemClock,
            out: &mut stdout,
            stop,
        },
    );
    match result {
        Ok(RunOutcome::Finished { .. }) => exit_code::OK,
        Ok(RunOutcome::LockBusy(info)) => {
            let who = info.map_or_else(
                || "un'altra istanza".to_owned(),
                |i| format!("PID {} su {}", i.pid, i.host),
            );
            eprintln!("motore già attivo ({who})");
            exit_code::LOCK_BUSY
        }
        Err(e) => {
            eprintln!("motore fermato: {e}");
            exit_code::ERROR
        }
    }
}

#[cfg(test)]
mod tests;
