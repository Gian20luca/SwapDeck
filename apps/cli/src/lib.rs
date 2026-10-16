//! `swapdeck-cli`: comandi sottili sopra `swapdeck-core`, con JSON su stdout.
//!
//! Successo: il risultato su stdout e uscita 0. Errore: `{"error": CswapError}` su stdout e uscita
//! [`exit_code::ERROR`] (operazione fallita) o [`exit_code::USAGE`] (argomenti o JSON non validi).
//! `headless` è il runner del core (righe di log, uscite di `headless::exit_code`).
//! Le conferme spettano all'interfaccia: qui si esegue senza chiedere.

pub mod output;
#[cfg(test)]
mod ts_export;

use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use swapdeck_core::config::{
    self, AppConfig, AppPreferences, CONFIG_SCHEMA_VERSION, ConfigError, PreferencesPatch,
};
use swapdeck_core::conflicts::{ConflictSource, SystemConflicts};
use swapdeck_core::cswap::{
    ActionOutcome, AutoMessage, CswapAction, CswapClient, CswapError, CswapErrorKind, ListResponse,
};
use swapdeck_core::engine::{EngineProfiles, EngineRules, EngineState, EngineView, ProfileCommand};
use swapdeck_core::history::{History, HistoryEvents, HistoryQuery, HistoryRecord};
use swapdeck_core::lock;
use swapdeck_core::notify::{self, NotificationConfig, NotifyPoll};
use swapdeck_core::paths::AppPaths;
use swapdeck_core::setup::{Setup, SetupProgress};
use swapdeck_core::timeutil::now_unix;

use output::{CliErrorEnvelope, CliState, CliVersion, ConfigView, EngineStatus, PROTOCOL};

/// Codici di uscita dei comandi JSON (`headless` usa quelli del core: 0/1/2/3).
pub mod exit_code {
    pub const OK: i32 = 0;
    pub const ERROR: i32 = 1;
    pub const USAGE: i32 = 2;
}

pub const USAGE: &str = "uso: swapdeck-cli <comando>
  version
  state
  status
  cswap-config [CHIAVE]
  confirm - [--active N|none]  (azione JSON da stdin)
  action -                     (azione JSON da stdin)
  history [--from SECONDI] [--to SECONDI] [--account N]
  history events [--since SECONDI] [--account N]
  mappings
  config get
  config set <json|->
  preferences get
  preferences set <json|->
  notifications get
  notifications set <json|->   (merge patch della sezione notifications)
  notifications poll [-]       (lista da stdin; notifiche solo senza motore vivo)
  engine status
  engine state
  engine rules set <json|->
  engine profiles [<json|->]          (senza argomento: elenco; con un'operazione: la applica)
  setup diagnose|install|upgrade      (install e upgrade: eventi JSON per riga, poi l'esito)
  setup check-update [--force]
  setup dismiss-update <VERSIONE>
  headless [--once] [--dry-run] [--stop-on-stdin-eof] [--notify=os|stdout|both]";

/// Dimensione massima del JSON letto da argomento o stdin.
const MAX_INPUT_BYTES: u64 = 1024 * 1024;
/// Intervallo di default dello storico.
const DEFAULT_HISTORY_SECS: i64 = 7 * 24 * 3600;
/// Attesa massima di `cswap auto --once`.
const AUTO_ONCE_WAIT: Duration = Duration::from_secs(300);

type Res<T> = Result<T, CswapError>;

fn usage_error(msg: impl AsRef<str>) -> CswapError {
    CswapError::new(CswapErrorKind::InvalidUsage, msg)
}

fn failed(msg: impl AsRef<str>) -> CswapError {
    CswapError::new(CswapErrorKind::Failed, msg)
}

/// Esegue la CLI con gli argomenti (senza il nome del programma) e restituisce il codice di uscita.
pub fn main_with(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("headless") {
        return headless(&args[1..]);
    }
    let mut stdout = io::stdout().lock();
    match dispatch(args) {
        Ok(value) => write_json(&mut stdout, &value).map_or(exit_code::ERROR, |()| exit_code::OK),
        Err(error) => {
            let code = if error.kind == CswapErrorKind::InvalidUsage {
                exit_code::USAGE
            } else {
                exit_code::ERROR
            };
            let _ = write_json(&mut stdout, &CliErrorEnvelope { error });
            code
        }
    }
}

fn write_json(out: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *out, value)?;
    out.write_all(b"\n")?;
    out.flush()
}

fn to_value(value: impl Serialize) -> Res<Value> {
    serde_json::to_value(value).map_err(|e| failed(e.to_string()))
}

fn dispatch(args: &[String]) -> Res<Value> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["version"] => to_value(CliVersion {
            name: "swapdeck-cli".to_owned(),
            version: swapdeck_core::version().to_owned(),
            protocol: PROTOCOL,
        }),
        ["state"] => to_value(state()),
        ["status"] => to_value(client()?.status()?),
        ["cswap-config"] => to_value(client()?.config_list()?),
        ["cswap-config", key] => to_value(client()?.config_get(key)?),
        ["confirm", "-", rest @ ..] => confirm(&read_input("-")?, rest),
        ["action", "-"] => to_value(action(&parse_action(&read_input("-")?)?)?),
        ["confirm" | "action", ..] => Err(usage_error(format!(
            "l'azione va passata da stdin (-)\n{USAGE}"
        ))),
        ["history", "events", rest @ ..] => to_value(history_events(rest)?),
        ["history", rest @ ..] => to_value(history(rest)?),
        ["mappings"] => to_value(client()?.mappings()?),
        ["preferences", "get"] => to_value(preferences_get()?),
        ["preferences", "set", input] => to_value(preferences_set(&read_input(input)?)?),
        ["engine", "state"] => to_value(engine_state(&paths()?)?),
        ["engine", "rules", "set", input] => to_value(engine_rules_set(&read_input(input)?)?),
        ["engine", "profiles"] => to_value(engine_profiles()?),
        ["engine", "profiles", input] => to_value(engine_profile_apply(&read_input(input)?)?),
        ["notifications", "get"] => to_value(notifications_get()?),
        ["notifications", "set", input] => to_value(notifications_set(&read_input(input)?)?),
        ["notifications", "poll"] => to_value(notifications_poll(None)?),
        ["notifications", "poll", "-"] => to_value(notifications_poll(Some(read_input("-")?))?),
        ["config", "get"] => to_value(config_get()?),
        ["config", "set", input] => to_value(config_set(&read_input(input)?)?),
        ["engine", "status"] => to_value(engine_status(&paths()?)),
        ["setup", rest @ ..] => setup_command(rest),
        [] | ["help" | "--help" | "-h"] => Err(usage_error(USAGE)),
        // Solo il primo argomento (accorciato): gli altri possono contenere dati dell'utente.
        [first, ..] => Err(usage_error(format!(
            "comando non valido: {}\n{USAGE}",
            first.chars().take(40).collect::<String>()
        ))),
    }
}

/// `setup …`: diagnosi, installazione e aggiornamento di cswap. Installazione e aggiornamento
/// scrivono un evento `SetupProgress` JSON per riga, poi l'esito (o la busta d'errore).
fn setup_command(rest: &[&str]) -> Res<Value> {
    let setup = || paths().map(|p| Setup::new(p.data_dir));
    let mut progress = |event: SetupProgress| {
        let _ = write_json(&mut io::stdout().lock(), &event);
    };
    match rest {
        ["diagnose"] => to_value(setup()?.diagnose()),
        ["install"] => to_value(setup()?.install(&mut progress)?),
        ["upgrade"] => to_value(setup()?.upgrade(&mut progress)?),
        ["check-update"] => to_value(setup()?.check_update(false)?),
        ["check-update", "--force"] => to_value(setup()?.check_update(true)?),
        ["dismiss-update", version] => {
            setup()?.dismiss_update(version)?;
            Ok(serde_json::json!({ "ok": true }))
        }
        _ => Err(usage_error(format!("opzioni di setup non valide\n{USAGE}"))),
    }
}

fn paths() -> Res<AppPaths> {
    AppPaths::from_env_or_system().ok_or_else(|| failed("cartelle dell'utente non determinabili"))
}

fn client() -> Res<CswapClient> {
    CswapClient::locate()
}

/// JSON da argomento, o da stdin con `-` (obbligatorio per le azioni: gli argomenti sono visibili
/// agli altri processi e possono contenere un token).
fn read_input(arg: &str) -> Res<String> {
    if arg != "-" {
        if arg.len() as u64 > MAX_INPUT_BYTES {
            return Err(usage_error("JSON troppo grande"));
        }
        return Ok(arg.to_owned());
    }
    let mut text = String::new();
    io::stdin()
        .lock()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| usage_error(format!("stdin non leggibile: {e}")))?;
    if text.len() as u64 > MAX_INPUT_BYTES {
        return Err(usage_error("JSON troppo grande"));
    }
    Ok(text)
}

/// Il messaggio di serde può riportare valori dell'input (anche un token): si dà solo la posizione.
fn json_error(what: &str, e: &serde_json::Error) -> CswapError {
    usage_error(format!(
        "{what} non valida (riga {}, colonna {})",
        e.line(),
        e.column()
    ))
}

fn parse_action(text: &str) -> Res<CswapAction> {
    serde_json::from_str(text).map_err(|e| json_error("azione", &e))
}

fn now_ms() -> u64 {
    u64::try_from(now_unix()).unwrap_or(0).saturating_mul(1000)
}

fn state() -> CliState {
    let list = client().and_then(|c| c.list());
    let engine = match paths() {
        Ok(p) => engine_status(&p),
        Err(e) => EngineStatus {
            problem: Some(e.message),
            ..engine_default()
        },
    };
    let updated_at_ms = now_ms();
    match list {
        Ok(list) => CliState {
            list: Some(list),
            error: None,
            updated_at_ms,
            engine,
        },
        Err(error) => CliState {
            list: None,
            error: Some(error),
            updated_at_ms,
            engine,
        },
    }
}

fn engine_default() -> EngineStatus {
    let config = AppConfig::default();
    EngineStatus {
        enabled: config.engine.enabled,
        dry_run: config.engine.dry_run,
        poll_interval_seconds: config.poll_interval_seconds,
        running: false,
        lock: None,
        active_map: None,
        problem: None,
    }
}

/// Config e lock; un errore di lettura finisce in `problem`, non fa fallire il comando.
fn engine_status(paths: &AppPaths) -> EngineStatus {
    let mut status = engine_default();
    let mut problems = Vec::new();
    match config::load(&paths.config_file()) {
        Ok(loaded) => {
            status.enabled = loaded.config.engine.enabled;
            status.dry_run = loaded.config.engine.dry_run;
            status.poll_interval_seconds = loaded.config.poll_interval_seconds;
            status.active_map = loaded
                .config
                .engine_profiles
                .active()
                .map(|p| p.name.clone());
        }
        Err(e) => problems.push(e.to_string()),
    }
    match lock::read(&paths.data_dir) {
        Ok(info) => {
            status.running = info
                .as_ref()
                .is_some_and(|i| !i.is_stale(now_unix(), &lock::host_name()));
            status.lock = info;
        }
        Err(e) => problems.push(format!("lock non leggibile: {e}")),
    }
    if !problems.is_empty() {
        status.problem = Some(problems.join("; "));
    }
    status
}

fn confirm(text: &str, rest: &[&str]) -> Res<Value> {
    let active = match rest {
        [] => client()?.list()?.active_account_number,
        ["--active", "none"] => None,
        ["--active", n] => Some(parse_u32("--active", n)?),
        _ => return Err(usage_error(format!("opzioni non valide\n{USAGE}"))),
    };
    to_value(parse_action(text)?.confirmation(active))
}

fn parse_u32(name: &str, value: &str) -> Res<u32> {
    value
        .parse()
        .map_err(|_| usage_error(format!("{name}: numero non valido")))
}

fn parse_i64(name: &str, value: &str) -> Res<i64> {
    value
        .parse()
        .map_err(|_| usage_error(format!("{name}: numero non valido")))
}

/// Un'azione di F0. `run` restituisce il comando da aprire (lo apre chi chiama); `autoStart` solo
/// con `once` (la CLI non resta in vita: per il controllo continuo c'è il motore, `headless`).
fn action(action: &CswapAction) -> Res<ActionOutcome> {
    let client = client()?;
    match action {
        CswapAction::AutoStart { options } if options.once => auto_once(&client, options),
        CswapAction::AutoStart { .. } | CswapAction::AutoStop => Err(usage_error(
            "cswap auto continuo non si avvia dalla CLI: usare il motore di SwapDeck (headless) o --once",
        )),
        other => client.execute(other),
    }
}

fn auto_once(
    client: &CswapClient,
    options: &swapdeck_core::cswap::AutoOptions,
) -> Res<ActionOutcome> {
    let mut process = client.start_auto(options)?;
    let deadline = Instant::now() + AUTO_ONCE_WAIT;
    let mut events = Vec::new();
    let code = loop {
        while let Ok(msg) = process.events().try_recv() {
            match msg {
                AutoMessage::Event(e) => events.push(e),
                AutoMessage::Error(e) => return Err(e),
            }
        }
        if let Some(code) = process.try_exit() {
            break code;
        }
        if Instant::now() >= deadline {
            process.stop_within(Duration::ZERO);
            return Err(CswapError::new(
                CswapErrorKind::Timeout,
                "cswap auto --once non è terminato in tempo",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    while let Ok(AutoMessage::Event(e)) = process.events().try_recv() {
        events.push(e);
    }
    if let Some(e) = process.exit_error(code) {
        return Err(e);
    }
    let message = events
        .last()
        .and_then(|e| serde_json::to_string(e).ok())
        .unwrap_or_default();
    Ok(ActionOutcome {
        message,
        list: client.list().ok(),
        switch: None,
        command: None,
        warning: None,
    })
}

fn history(rest: &[&str]) -> Res<Vec<HistoryRecord>> {
    let mut from = None;
    let mut to = None;
    let mut account = None;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| usage_error(format!("{flag}: manca il valore")))?;
        match *flag {
            "--from" => from = Some(parse_i64(flag, value)?),
            "--to" => to = Some(parse_i64(flag, value)?),
            "--account" => account = Some(parse_u32(flag, value)?),
            _ => return Err(usage_error(format!("opzione sconosciuta: {flag}\n{USAGE}"))),
        }
    }
    let to = to.unwrap_or_else(now_unix);
    let from = from.unwrap_or(to - DEFAULT_HISTORY_SECS);
    if from > to {
        return Err(usage_error("--from dopo --to"));
    }
    let records = History::new(&paths()?.data_dir)
        .read_range(from, to)
        .map_err(|e| failed(format!("storico non leggibile: {e}")))?;
    Ok(records
        .into_iter()
        .filter(|r| match (r, account) {
            (HistoryRecord::Sample(s), Some(n)) => s.account == n,
            _ => true,
        })
        .collect())
}

/// `history events`: cambi e decisioni del motore (`--since` limitato alla conservazione).
fn history_events(rest: &[&str]) -> Res<HistoryEvents> {
    let mut query = HistoryQuery::default();
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let value = it
            .next()
            .ok_or_else(|| usage_error(format!("{flag}: manca il valore")))?;
        match *flag {
            "--since" => query.since_seconds = Some(parse_i64(flag, value)?),
            "--account" => query.account = Some(parse_u32(flag, value)?),
            _ => {
                return Err(usage_error(format!(
                    "opzione sconosciuta: {flag}
{USAGE}"
                )));
            }
        }
    }
    History::new(&paths()?.data_dir)
        .events(&query, now_unix())
        .map_err(|e| failed(format!("storico non leggibile: {e}")))
}

/// Errore della config: valori non validi = uso sbagliato.
fn config_error(e: ConfigError) -> CswapError {
    match e {
        ConfigError::Invalid(_) => usage_error(e.to_string()),
        _ => failed(e.to_string()),
    }
}

fn preferences_get() -> Res<AppPreferences> {
    let loaded = config::load(&paths()?.config_file()).map_err(config_error)?;
    Ok(loaded.config.preferences())
}

/// Salva solo i campi presenti (validati); gli altri campi della config restano.
fn preferences_set(text: &str) -> Res<AppPreferences> {
    let patch: PreferencesPatch =
        serde_json::from_str(text).map_err(|e| json_error("preferenze", &e))?;
    let saved = config::update(&paths()?.config_file(), |c| c.apply_preferences(&patch))
        .map_err(config_error)?;
    Ok(saved.preferences())
}

/// Regole della config, vista del motore in esecuzione (ultima decisione dello storico) e
/// conflitti (letti solo se il motore è acceso o in osservazione: la lettura costa).
fn engine_state(paths: &AppPaths) -> Res<EngineState> {
    let rules = config::load(&paths.config_file())
        .map_err(config_error)?
        .config
        .engine;
    let running = engine_status(paths).running;
    let view = if running {
        let events = History::new(&paths.data_dir)
            .events(&HistoryQuery::default(), now_unix())
            .unwrap_or_default();
        events.decisions.last().map(|d| {
            EngineView::new(true, &rules, Some(&d.decision)).explained(d.why.clone(), None)
        })
    } else {
        None
    };
    let conflicts = if rules.enabled || rules.dry_run {
        SystemConflicts::default().detect()
    } else {
        Vec::new()
    };
    Ok(EngineState {
        rules,
        view,
        conflicts,
    })
}

/// Valida e salva le regole; il motore in esecuzione (`headless`) le rilegge al giro dopo.
fn engine_rules_set(text: &str) -> Res<EngineState> {
    let rules: EngineRules = serde_json::from_str(text).map_err(|e| json_error("regole", &e))?;
    let paths = paths()?;
    config::update(&paths.config_file(), |c| c.set_engine_rules(rules)).map_err(config_error)?;
    engine_state(&paths)
}

/// Mappe e mappa attiva (migrate dalle regole se il file non le ha ancora).
fn engine_profiles() -> Res<EngineProfiles> {
    Ok(config::load(&paths()?.config_file())
        .map_err(config_error)?
        .config
        .engine_profiles)
}

/// Applica un'operazione sulle mappe; il motore in esecuzione usa la mappa attiva dal giro dopo.
fn engine_profile_apply(text: &str) -> Res<EngineState> {
    let cmd: ProfileCommand = serde_json::from_str(text).map_err(|e| json_error("mappa", &e))?;
    let paths = paths()?;
    config::update(&paths.config_file(), |c| c.apply_profile_command(&cmd))
        .map_err(config_error)?;
    engine_state(&paths)
}

/// Regole delle notifiche (sezione `notifications` della config).
fn notifications_get() -> Res<NotificationConfig> {
    let loaded = config::load(&paths()?.config_file()).map_err(config_error)?;
    Ok(loaded.config.notifications)
}

/// Applica una merge patch (RFC 7386) alle notifiche, la valida e la salva; le altre chiavi
/// della config restano.
fn notifications_set(text: &str) -> Res<NotificationConfig> {
    let patch: Value = serde_json::from_str(text).map_err(|e| json_error("notifiche", &e))?;
    let saved = config::update(&paths()?.config_file(), |c| c.patch_notifications(&patch))
        .map_err(config_error)?;
    Ok(saved.notifications)
}

/// Notifiche per l'estensione VS Code: solo se nessun motore vivo tiene il lock (altrimenti
/// le manda lui). La lista arriva da stdin (`-`, quella appena letta con `state`) o da cswap.
fn notifications_poll(input: Option<String>) -> Res<NotifyPoll> {
    let paths = paths()?;
    let list = match input {
        Some(text) => {
            Some(serde_json::from_str::<ListResponse>(&text).map_err(|e| json_error("lista", &e))?)
        }
        None => client().and_then(|c| c.list()).ok(),
    };
    let loaded = config::load(&paths.config_file()).map_err(config_error)?;
    let mut installed = || {
        client()
            .ok()?
            .version()
            .ok()
            .map(|v| format!("{}.{}.{}", v.major, v.minor, v.patch))
    };
    notify::poll(
        &paths.data_dir,
        list.as_ref(),
        &loaded.config.notifications,
        loaded.config.language,
        now_unix(),
        &mut installed,
    )
    .map_err(|e| failed(format!("notifiche non valutate: {e}")))
}
fn config_view(path: &Path) -> Res<ConfigView> {
    let loaded = config::load(path).map_err(|e| failed(e.to_string()))?;
    Ok(ConfigView {
        config: loaded.config,
        from_file: loaded.from_file,
        path: path.display().to_string(),
        warnings: loaded.warnings,
    })
}

fn config_get() -> Res<ConfigView> {
    config_view(&paths()?.config_file())
}

/// Applica una JSON merge patch (RFC 7386) alla config corrente, la valida e la salva in modo
/// atomico. `null` toglie una chiave (torna il default).
fn config_set(patch_text: &str) -> Res<ConfigView> {
    let patch: Value = serde_json::from_str(patch_text).map_err(|e| json_error("config", &e))?;
    if !patch.is_object() {
        return Err(usage_error("config set vuole un oggetto JSON"));
    }
    let path = paths()?.config_file();
    let current = config::load(&path).map_err(|e| failed(e.to_string()))?;
    let mut merged = to_value(&current.config)?;
    merge_patch(&mut merged, &patch);
    let mut next: AppConfig = serde_json::from_value(merged)
        .map_err(|e| usage_error(format!("config non valida: {e}")))?;
    next.schema_version = CONFIG_SCHEMA_VERSION;
    // Notifiche con le stesse regole di `notifications set` (soglie della versione 1 comprese).
    if let Some(n) = patch.get("notifications").filter(|n| n.is_object()) {
        next.notifications = current
            .config
            .notifications
            .merged(n)
            .map_err(|e| usage_error(format!("config non valida: {e}")))?;
    }
    next.preserved = current.config.preserved;
    let mut warnings = next.normalize();
    config::save(&path, &next).map_err(|e| usage_error(e.to_string()))?;
    let mut view = config_view(&path)?;
    warnings.append(&mut view.warnings);
    view.warnings = warnings;
    Ok(view)
}

/// JSON merge patch (RFC 7386).
fn merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(patch) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(serde_json::Map::new());
    }
    if let Value::Object(map) = target {
        for (key, value) in patch {
            if value.is_null() {
                map.remove(key);
            } else {
                merge_patch(map.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
    }
}

/// `headless`: il runner del core. Si ferma con Ctrl+C o, con `--stop-on-stdin-eof`, quando
/// stdin si chiude (così chi l'ha avviato lo ferma in modo pulito anche su Windows, dove non
/// esistono segnali verso un processo figlio senza console).
fn headless(args: &[String]) -> i32 {
    let stop_on_eof = args.iter().any(|a| a == "--stop-on-stdin-eof");
    let rest: Vec<String> = args
        .iter()
        .filter(|a| *a != "--stop-on-stdin-eof")
        .cloned()
        .collect();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    if let Err(e) = ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst)) {
        eprintln!("gestore di Ctrl+C non installato: {e}");
    }
    if stop_on_eof {
        let flag = Arc::clone(&stop);
        std::thread::spawn(move || {
            let mut sink = [0u8; 256];
            let mut stdin = io::stdin();
            while matches!(stdin.read(&mut sink), Ok(n) if n > 0) {}
            flag.store(true, Ordering::SeqCst);
        });
    }
    swapdeck_core::headless::cli_main(&rest, &stop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn merge_patch_follows_rfc7386() {
        let mut v = json!({"a": 1, "b": {"c": 2, "d": 3}});
        merge_patch(&mut v, &json!({"a": null, "b": {"c": 5}, "e": [1]}));
        assert_eq!(v, json!({"b": {"c": 5, "d": 3}, "e": [1]}));
    }

    #[test]
    fn usage_errors() {
        for bad in [
            "",
            "nope",
            "config",
            "engine",
            "history --from",
            "history --x 1",
        ] {
            let err = dispatch(&args(bad)).unwrap_err();
            assert_eq!(err.kind, CswapErrorKind::InvalidUsage, "{bad}");
        }
    }

    #[test]
    fn action_parse_error_hides_values() {
        let err = parse_action(r#"{"kind":"addToken","token":12345678}"#).unwrap_err();
        assert_eq!(err.kind, CswapErrorKind::InvalidUsage);
        assert!(!err.message.contains("12345678"), "{}", err.message);
    }

    #[test]
    fn version_is_json() {
        let v = dispatch(&args("version")).unwrap();
        assert_eq!(v["name"], "swapdeck-cli");
        assert_eq!(v["protocol"], PROTOCOL);
    }

    #[test]
    fn confirm_with_explicit_active() {
        let v = confirm(r#"{"kind":"disable","account":6}"#, &["--active", "6"]).unwrap();
        assert_eq!(v["kind"], "simple");
        let v = confirm(r#"{"kind":"disable","account":3}"#, &["--active", "6"]).unwrap();
        assert_eq!(v["kind"], "none");
    }

    #[test]
    fn actions_only_from_stdin_and_errors_do_not_echo_arguments() {
        let secret = r#"{"kind":"addToken","token":"sk-ant-oat01-NOT-REAL"}"#;
        for cmd in ["action", "confirm"] {
            let e = dispatch(&[cmd.to_owned(), secret.to_owned()]).unwrap_err();
            assert_eq!(e.kind, CswapErrorKind::InvalidUsage);
            assert!(!e.message.contains("NOT-REAL"), "{}", e.message);
        }
        let e = dispatch(&["bogus".to_owned(), secret.to_owned()]).unwrap_err();
        assert!(
            e.message.starts_with("comando non valido: bogus"),
            "{}",
            e.message
        );
        assert!(!e.message.contains("NOT-REAL"));
    }
}
