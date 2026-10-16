//! Finto `cswap` per i test di integrazione: risponde con le fixture e con uno
//! stato JSON (`list --json`) che modifica come farebbe cswap, simula prompt su
//! stdin, codici di uscita e attese. Si configura con `fake.json` accanto
//! all'eseguibile; ogni chiamata è registrata in una riga JSON nel file di log.
//! Senza `fake.json` usa `fixtures/cswap/` e parte da `list.json`, salvando lo
//! stato in `state.json` accanto all'eseguibile: così si può indicare con
//! `SWAPDECK_CSWAP` per provare app ed estensione senza toccare il cswap reale.
//! Chiamato `uv`, simula `uv tool upgrade claude-swap`.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use serde_json::{Value, json};

struct Fake {
    cfg: Value,
    dir: PathBuf,
    stdin_lines: Vec<String>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("__sleep") {
        if let Some(p) = args.get(1) {
            fs::write(p, std::process::id().to_string()).unwrap();
        }
        std::thread::sleep(Duration::from_secs(60));
        return ExitCode::SUCCESS;
    }
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().to_path_buf();
    let cfg: Value = fs::read_to_string(dir.join("fake.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or(json!({}));
    let mut fake = Fake {
        cfg,
        dir,
        stdin_lines: Vec::new(),
    };
    let is_uv = exe
        .file_stem()
        .is_some_and(|s| s.to_string_lossy().starts_with("uv"));
    let code = if is_uv {
        fake.uv(&args)
    } else {
        fake.dispatch(&args)
    };
    fake.log(&args, code);
    ExitCode::from(code)
}

impl Fake {
    fn log(&self, args: &[String], code: u8) {
        let Some(log) = self.cfg.get("log").and_then(Value::as_str) else {
            return;
        };
        let line = json!({
            "args": args,
            "stdin": self.stdin_lines,
            "code": code,
            "noColor": std::env::var("NO_COLOR").ok().as_deref() == Some("1"),
            "secureStorageEnv": std::env::var_os("CLAUDE_SECURESTORAGE_CONFIG_DIR").is_some(),
        });
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log)
            .unwrap();
        writeln!(f, "{line}").unwrap();
    }

    fn read_line(&mut self) -> Option<String> {
        let mut s = String::new();
        match std::io::stdin().lock().read_line(&mut s) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                let l = s.trim().to_owned();
                self.stdin_lines.push(l.clone());
                Some(l)
            }
        }
    }

    fn fixture(&self, name: &str) -> String {
        let dir = self.cfg["fixtures"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cswap"));
        fs::read_to_string(dir.join(name)).unwrap()
    }

    fn state_path(&self) -> PathBuf {
        self.cfg["state"]
            .as_str()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.dir.join("state.json"))
    }

    fn state(&self) -> Value {
        fs::read_to_string(self.state_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_else(|| {
                if self.cfg.get("state").is_none() {
                    // Senza fake.json (prove dell'app con SWAPDECK_CSWAP): parte da list.json.
                    serde_json::from_str(&self.fixture("list.json")).unwrap()
                } else {
                    json!({"schemaVersion": 1, "activeAccountNumber": null, "accounts": []})
                }
            })
    }

    /// Mappature delle cartelle (`<stato>.maps.json`: percorso → account ed email).
    fn maps_path(&self) -> PathBuf {
        self.state_path().with_extension("maps.json")
    }

    fn maps(&self) -> serde_json::Map<String, Value> {
        fs::read_to_string(self.maps_path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    fn save_maps(&self, maps: &serde_json::Map<String, Value>) {
        fs::write(
            self.maps_path(),
            serde_json::to_string_pretty(maps).unwrap(),
        )
        .unwrap();
    }

    /// `map` (elenco nel formato di cswap 0.27) e `map <n> [path]`.
    fn map(&self, rest: &[&str]) -> u8 {
        let mut maps = self.maps();
        let Some(first) = rest.first() else {
            if maps.is_empty() {
                println!("No directory mappings yet.");
                println!("Map one with: cswap map <NUM|EMAIL> [PATH]");
                return 0;
            }
            println!("Directory mappings:");
            let state = self.state();
            for (path, entry) in &maps {
                let email = entry["email"].as_str().unwrap_or_default();
                let slot = Self::accounts(&state)
                    .into_iter()
                    .find(|a| a["email"].as_str() == Some(email))
                    .and_then(|a| a["number"].as_u64());
                match slot {
                    Some(n) => println!("  {path} → {n}: {email} [personal]"),
                    None => println!("  {path} → {email} (account removed)"),
                }
            }
            return 0;
        };
        let n: u64 = first.parse().unwrap_or(0);
        let Some(a) = Self::find(&self.state(), n) else {
            return self.error(&format!("Account-{n} does not exist"));
        };
        let path = rest.get(1).copied().unwrap_or(".");
        let email = a["email"].as_str().unwrap_or_default().to_owned();
        println!("Mapped {path} → Account-{n} ({email})");
        maps.insert(path.to_owned(), serde_json::json!({ "email": email }));
        self.save_maps(&maps);
        0
    }

    fn save(&self, state: &Value) {
        fs::write(
            self.state_path(),
            serde_json::to_string_pretty(state).unwrap(),
        )
        .unwrap();
    }

    fn accounts(state: &Value) -> Vec<Value> {
        state["accounts"].as_array().cloned().unwrap_or_default()
    }

    fn find(state: &Value, n: u64) -> Option<Value> {
        Self::accounts(state)
            .into_iter()
            .find(|a| a["number"].as_u64() == Some(n))
    }

    fn set_accounts(state: &mut Value, mut accounts: Vec<Value>) {
        accounts.sort_by_key(|a| a["number"].as_u64());
        state["accounts"] = Value::Array(accounts);
    }

    fn envelope(&self, ty: &str, msg: &str) -> u8 {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"schemaVersion": 1, "error": {"type": ty, "message": msg}})
            )
            .unwrap()
        );
        1
    }

    fn error(&self, msg: &str) -> u8 {
        eprintln!("Error: {msg}");
        1
    }

    fn usage_error(&self, msg: &str) -> u8 {
        eprintln!("usage: cswap <command> [args] [options]\ncswap: error: {msg}");
        2
    }

    fn eof_traceback(&self) -> u8 {
        eprintln!(
            "Traceback (most recent call last):\n  File \"cli.py\", line 1, in <module>\nEOFError: EOF when reading a line"
        );
        1
    }

    /// Applica le sostituzioni configurate per il sottocomando; `Some(code)` = esce subito.
    fn overrides(&self, sub: &str) -> Option<u8> {
        let o = self.cfg.get("overrides")?.get(sub)?;
        if let Some(s) = o.get("sleep").and_then(Value::as_f64) {
            if o.get("spawnChild").and_then(Value::as_bool) == Some(true) {
                let _ = Command::new(std::env::current_exe().unwrap())
                    .arg("__sleep")
                    .arg(self.dir.join("grandchild.pid"))
                    .stdin(Stdio::null())
                    .spawn();
                let pid_file = self.dir.join("grandchild.pid");
                for _ in 0..200 {
                    if pid_file.exists() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            std::thread::sleep(Duration::from_secs_f64(s));
        }
        let code = o.get("exit").and_then(Value::as_u64)?;
        if let Some(f) = o.get("stdoutFixture").and_then(Value::as_str) {
            print!("{}", self.fixture(f));
        }
        if let Some(s) = o.get("stdout").and_then(Value::as_str) {
            print!("{s}");
        }
        if let Some(s) = o.get("stderr").and_then(Value::as_str) {
            eprint!("{s}");
        }
        Some(code as u8)
    }

    fn dispatch(&mut self, args: &[String]) -> u8 {
        let Some(sub) = args.first().cloned() else {
            eprintln!("TUI avviata per errore");
            std::thread::sleep(Duration::from_secs(60));
            return 1;
        };
        if let Some(code) = self.overrides(&sub) {
            return code;
        }
        let rest: Vec<&str> = args[1..].iter().map(String::as_str).collect();
        let json_flag = rest.contains(&"--json");
        match sub.as_str() {
            "--version" => {
                println!("{}", self.cfg["version"].as_str().unwrap_or("cswap 0.27.2"));
                0
            }
            "list" if json_flag => {
                println!("{}", serde_json::to_string_pretty(&self.state()).unwrap());
                0
            }
            "status" if json_flag => self.status(),
            "switch" if json_flag => self.switch(&rest),
            "list" | "status" | "switch" => self.usage_error("il finto cswap vuole --json"),
            "add" => self.add(&rest, None),
            "add-token" => {
                if rest.first() != Some(&"-") {
                    return self.usage_error("il finto cswap vuole add-token -");
                }
                let token = self.read_line().unwrap_or_default();
                if token.is_empty() {
                    return self.error("Token cannot be empty");
                }
                self.add(&rest[1..], Some(token))
            }
            "remove" => self.remove(&rest),
            "disable" | "enable" => self.toggle(&sub, &rest),
            "alias" => self.alias(&rest),
            "swap" => self.swap(&rest),
            "move" => self.move_account(&rest),
            "map" => self.map(&rest),
            "unmap" => {
                let path = rest.first().copied().unwrap_or(".").to_owned();
                let mut maps = self.maps();
                if maps.remove(&path).is_some() {
                    self.save_maps(&maps);
                    println!("Unmapped {path}");
                } else {
                    println!("No mapping for {path}");
                }
                0
            }
            "config" => self.config(&rest),
            "export" => {
                let path = rest[0];
                fs::write(path, "{\"accounts\": []}").unwrap();
                println!("Exported accounts to {path}");
                0
            }
            "import" => {
                if !Path::new(rest[0]).exists() {
                    return self.error(&format!("file not found: {}", rest[0]));
                }
                println!("Imported 0 account(s), skipped 0");
                0
            }
            "unclaimed" => match rest.as_slice() {
                [] => {
                    println!("No unclaimed credential entries");
                    0
                }
                ["--purge", "abc"] => {
                    println!("Purged abc");
                    0
                }
                ["--purge", id] => {
                    println!("no unclaimed entry {id}");
                    1
                }
                _ => self.usage_error("unclaimed"),
            },
            "purge" => {
                print!("Are you sure you want to purge all data? [y/N] ");
                let _ = std::io::stdout().flush();
                match self.read_line() {
                    None => self.eof_traceback(),
                    Some(a) if a.eq_ignore_ascii_case("y") => {
                        self.save(&json!({"schemaVersion": 1, "activeAccountNumber": null, "accounts": []}));
                        println!("All data purged");
                        0
                    }
                    Some(_) => {
                        println!("Cancelled");
                        0
                    }
                }
            }
            "auto" if json_flag => {
                let once = rest.contains(&"--once");
                if !once {
                    // Ucciso dal client prima di uscire: registra la chiamata prima degli eventi.
                    let all: Vec<String> = std::iter::once(sub.clone())
                        .chain(rest.iter().map(|s| s.to_string()))
                        .collect();
                    self.log(&all, 0);
                }
                let events = self.fixture("auto-events.jsonl");
                let mut out = std::io::stdout().lock();
                for line in events.lines().filter(|l| !l.contains("\"error\": {")) {
                    writeln!(out, "{line}").unwrap();
                }
                writeln!(
                    out,
                    "{{\"schemaVersion\": 1, \"event\": \"future-event\", \"ts\": \"x\"}}"
                )
                .unwrap();
                // Pausa tra due giri, salvo `autoBusy` (giro in corso fino alla chiusura).
                if self.cfg.get("autoBusy").and_then(Value::as_bool) != Some(true) {
                    writeln!(
                        out,
                        "{{\"schemaVersion\": 1, \"event\": \"sleep\", \"ts\": \"x\", \"seconds\": 60.0, \"until\": \"x\"}}"
                    )
                    .unwrap();
                }
                out.flush().unwrap();
                drop(out);
                if once {
                    return 2;
                }
                std::thread::sleep(Duration::from_secs(60));
                0
            }
            other => self.usage_error(&format!("unknown command {other}")),
        }
    }

    /// `uv`: `--version`, `tool install claude-swap` (con `installTo` collega sé stesso come
    /// cswap in quel percorso), `tool upgrade claude-swap` (con `upgradeTo` cambia la
    /// versione di cswap, con `upgradeSchema` lo `schemaVersion` dello stato). `uvStderr`
    /// è stampato su stderr da ogni comando `tool`.
    fn uv(&mut self, args: &[String]) -> u8 {
        if args.first().map(String::as_str) == Some("--version") {
            println!("uv 0.9.9 (fake)");
            return 0;
        }
        if let Some(s) = self.cfg.get("uvStderr").and_then(Value::as_str) {
            eprint!("{s}");
        }
        if args == ["tool", "install", "claude-swap"] {
            if let Some(code) = self.overrides("uvInstall") {
                return code;
            }
            eprintln!("Resolved 12 packages in 10ms\nInstalled 2 executables: claude-swap, cswap");
            if let Some(dest) = self.cfg.get("installTo").and_then(Value::as_str) {
                let src = std::env::current_exe().unwrap();
                if fs::hard_link(&src, dest).is_err() {
                    fs::copy(&src, dest).unwrap();
                }
            }
            return 0;
        }
        // Ripristino: `tool install claude-swap==X --force` riporta versione e schemaVersion 1.
        if let [tool, install, spec, force] = args {
            if let (true, Some(v)) = (
                tool == "tool" && install == "install" && force == "--force",
                spec.strip_prefix("claude-swap=="),
            ) {
                if let Some(code) = self.overrides("uvRollback") {
                    return code;
                }
                let mut cfg = self.cfg.clone();
                cfg["version"] = json!(format!("cswap {v}"));
                fs::write(self.dir.join("fake.json"), cfg.to_string()).unwrap();
                let mut state = self.state();
                state["schemaVersion"] = json!(1);
                self.save(&state);
                eprintln!("Installed 2 executables: claude-swap, cswap");
                return 0;
            }
        }
        if let Some(code) = self.overrides("uv") {
            return code;
        }
        if args == ["tool", "upgrade", "claude-swap"] {
            if let Some(v) = self.cfg.get("upgradeTo").and_then(Value::as_str) {
                let mut cfg = self.cfg.clone();
                cfg["version"] = json!(format!("cswap {v}"));
                fs::write(self.dir.join("fake.json"), cfg.to_string()).unwrap();
            }
            if let Some(s) = self.cfg.get("upgradeSchema").and_then(Value::as_u64) {
                let mut state = self.state();
                state["schemaVersion"] = json!(s);
                self.save(&state);
            }
            println!("Updated claude-swap v0.27.2 -> v0.28.0");
            0
        } else {
            eprintln!("error: unexpected arguments");
            2
        }
    }

    fn status(&self) -> u8 {
        let state = self.state();
        let active = state["activeAccountNumber"]
            .as_u64()
            .and_then(|n| Self::find(&state, n))
            .map(|mut a| {
                let o = a.as_object_mut().unwrap();
                o.remove("active");
                o.remove("disabled");
                o.remove("loginExpiresAt");
                o.insert("managed".into(), json!(true));
                a
            });
        let mut out = json!({"schemaVersion": 1, "active": active});
        if active.is_some() {
            out["totalManagedAccounts"] = json!(Self::accounts(&state).len());
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        0
    }

    fn switch(&mut self, rest: &[&str]) -> u8 {
        let mut state = self.state();
        let accounts = Self::accounts(&state);
        if accounts.is_empty() {
            return self.envelope("ConfigError", "No accounts are managed yet");
        }
        let mut target: Option<u64> = None;
        let mut strategy = None;
        let mut models = None;
        let mut i = 0;
        while i < rest.len() {
            match rest[i] {
                "--strategy" => {
                    strategy = rest.get(i + 1).copied();
                    i += 1;
                }
                "--model" => {
                    models = rest.get(i + 1).copied();
                    i += 1;
                }
                "--force" | "--json" => {}
                v => match v.parse() {
                    Ok(n) => target = Some(n),
                    Err(_) => {
                        return self.envelope(
                            "ValidationError",
                            &format!("Invalid account identifier: {v}"),
                        );
                    }
                },
            }
            i += 1;
        }
        let active = state["activeAccountNumber"].as_u64();
        let reference = |n: Option<u64>| {
            n.and_then(|n| Self::find(&state, n))
                .map(|a| json!({"number": a["number"], "email": a["email"]}))
        };
        let from = reference(active);
        let enabled: Vec<u64> = accounts
            .iter()
            .filter(|a| a.get("disabled").is_none())
            .filter_map(|a| a["number"].as_u64())
            .collect();
        let (strategy_name, dest) = match (target, strategy) {
            (Some(n), _) => {
                if Self::find(&state, n).is_none() {
                    return self.envelope(
                        "AccountNotFoundError",
                        &format!("No account found with identifier: {n}"),
                    );
                }
                ("direct", Some(n))
            }
            (None, Some("best")) => ("best", active),
            (None, s) => {
                let next = enabled
                    .iter()
                    .copied()
                    .find(|n| Some(*n) > active)
                    .or_else(|| enabled.first().copied());
                (
                    if s.is_some() {
                        "next-available"
                    } else {
                        "rotation"
                    },
                    next,
                )
            }
        };
        let switched = dest.is_some() && dest != active;
        let reason = match (switched, strategy_name) {
            (true, _) => "switched",
            (false, "best") => "already-best",
            (false, _) => "already-active",
        };
        let to = reference(dest);
        if switched {
            state["activeAccountNumber"] = json!(dest);
            let accounts = Self::accounts(&state)
                .into_iter()
                .map(|mut a| {
                    a["active"] = json!(a["number"].as_u64() == dest);
                    a
                })
                .collect();
            Self::set_accounts(&mut state, accounts);
            if self.cfg.get("ignoreSwitch").and_then(Value::as_bool) != Some(true) {
                self.save(&state);
            }
        }
        let mut out = json!({
            "schemaVersion": 1,
            "switched": switched,
            "from": from,
            "to": to,
            "strategy": strategy_name,
            "reason": reason,
            "message": format!("{reason} (finto)"),
            "warnings": [],
        });
        if let Some(m) = models {
            out["models"] = json!(m.split(',').collect::<Vec<_>>());
            out["modelSource"] = json!("cli");
        }
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        0
    }

    fn add(&mut self, rest: &[&str], token: Option<String>) -> u8 {
        if token.is_none() && self.cfg.get("loggedIn").and_then(Value::as_bool) == Some(false) {
            return self.error("No active Claude account found. Please log in first.");
        }
        let mut slot: Option<u64> = None;
        let mut alias = None;
        let mut email = None;
        let mut i = 0;
        while i < rest.len() {
            match rest[i] {
                "--slot" => slot = rest.get(i + 1).and_then(|s| s.parse().ok()),
                "--alias" => alias = rest.get(i + 1).copied(),
                "--email" => email = rest.get(i + 1).copied(),
                other => return self.usage_error(&format!("unexpected {other}")),
            }
            i += 2;
        }
        let mut state = self.state();
        let mut accounts = Self::accounts(&state);
        let n = slot.unwrap_or_else(|| {
            accounts
                .iter()
                .filter_map(|a| a["number"].as_u64())
                .max()
                .unwrap_or(0)
                + 1
        });
        if slot.is_some() && Self::find(&state, n).is_some() {
            println!("Slot {n} already occupied");
            print!("Overwrite slot {n}? [y/N] ");
            let _ = std::io::stdout().flush();
            match self.read_line() {
                Some(a) if a.eq_ignore_ascii_case("y") || a.eq_ignore_ascii_case("yes") => {}
                _ => {
                    println!("Cancelled");
                    return 0;
                }
            }
            accounts.retain(|a| a["number"].as_u64() != Some(n));
        }
        let email = email.map(str::to_owned).unwrap_or_else(|| match &token {
            Some(_) => format!("setup-token-{n}@token.local"),
            None => format!("new{n}@example.com"),
        });
        let mut row = json!({
            "number": n, "email": email, "organizationName": "", "organizationUuid": "",
            "isOrganization": false, "active": false, "usageStatus": "unavailable", "usage": null,
        });
        if let Some(a) = alias {
            row["alias"] = json!(a);
        }
        accounts.push(row);
        Self::set_accounts(&mut state, accounts);
        self.save(&state);
        println!("Added Account-{n} ({email})");
        0
    }

    fn remove(&mut self, rest: &[&str]) -> u8 {
        let n: u64 = rest.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let mut state = self.state();
        let Some(acc) = Self::find(&state, n) else {
            return self.error(&format!("Account-{n} does not exist"));
        };
        print!(
            "Are you sure you want to permanently remove Account-{n} ({})? [y/N] ",
            acc["email"].as_str().unwrap()
        );
        let _ = std::io::stdout().flush();
        match self.read_line() {
            None => self.eof_traceback(),
            Some(a) if a.eq_ignore_ascii_case("y") => {
                if self.cfg.get("ignoreRemove").and_then(Value::as_bool) != Some(true) {
                    let accounts = Self::accounts(&state)
                        .into_iter()
                        .filter(|a| a["number"].as_u64() != Some(n))
                        .collect();
                    Self::set_accounts(&mut state, accounts);
                    if state["activeAccountNumber"].as_u64() == Some(n) {
                        state["activeAccountNumber"] = Value::Null;
                    }
                    self.save(&state);
                }
                println!("Account-{n} removed");
                0
            }
            Some(_) => {
                println!("Cancelled");
                0
            }
        }
    }

    fn toggle(&mut self, sub: &str, rest: &[&str]) -> u8 {
        let n: u64 = rest.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let mut state = self.state();
        if Self::find(&state, n).is_none() {
            return self.error(&format!("Account-{n} does not exist"));
        }
        let accounts = Self::accounts(&state)
            .into_iter()
            .map(|mut a| {
                if a["number"].as_u64() == Some(n) {
                    let o = a.as_object_mut().unwrap();
                    if sub == "disable" {
                        o.insert("disabled".into(), json!(true));
                    } else {
                        o.remove("disabled");
                    }
                }
                a
            })
            .collect();
        Self::set_accounts(&mut state, accounts);
        self.save(&state);
        println!("Account-{n} {sub}d");
        0
    }

    fn alias(&mut self, rest: &[&str]) -> u8 {
        let mut state = self.state();
        if rest.is_empty() {
            let rows: Vec<String> = Self::accounts(&state)
                .iter()
                .filter_map(|a| {
                    a.get("alias").map(|al| {
                        format!(
                            "  {}: {} ({})",
                            a["number"],
                            al.as_str().unwrap(),
                            a["email"].as_str().unwrap()
                        )
                    })
                })
                .collect();
            if rows.is_empty() {
                println!("No aliases set");
            } else {
                println!("Aliases:\n{}", rows.join("\n"));
            }
            return 0;
        }
        let n: u64 = rest[0].parse().unwrap_or(0);
        if Self::find(&state, n).is_none() {
            return self.error(&format!("Account-{n} does not exist"));
        }
        let accounts = Self::accounts(&state)
            .into_iter()
            .map(|mut a| {
                if a["number"].as_u64() == Some(n) {
                    let o = a.as_object_mut().unwrap();
                    match rest.get(1) {
                        Some(&"--unset") => {
                            o.remove("alias");
                        }
                        Some(name) => {
                            o.insert("alias".into(), json!(name));
                        }
                        None => {}
                    }
                }
                a
            })
            .collect();
        Self::set_accounts(&mut state, accounts);
        self.save(&state);
        println!("Alias updated for Account {n}");
        0
    }

    fn renumber(&mut self, map: impl Fn(u64) -> u64) {
        let mut state = self.state();
        let active = state["activeAccountNumber"].as_u64().map(&map);
        let accounts = Self::accounts(&state)
            .into_iter()
            .map(|mut a| {
                let n = a["number"].as_u64().unwrap();
                a["number"] = json!(map(n));
                a
            })
            .collect();
        Self::set_accounts(&mut state, accounts);
        state["activeAccountNumber"] = json!(active);
        self.save(&state);
    }

    fn swap(&mut self, rest: &[&str]) -> u8 {
        let a: u64 = rest[0].parse().unwrap_or(0);
        let b: u64 = rest[1].parse().unwrap_or(0);
        if a == b {
            return self.error("Cannot swap an account with itself");
        }
        self.renumber(|n| {
            if n == a {
                b
            } else if n == b {
                a
            } else {
                n
            }
        });
        println!("Swapped Account-{a} and Account-{b}");
        0
    }

    fn move_account(&mut self, rest: &[&str]) -> u8 {
        let a: u64 = rest[0].parse().unwrap_or(0);
        let b: u64 = rest[1].parse().unwrap_or(0);
        if Self::find(&self.state(), a).is_none() {
            return self.error(&format!("Account-{a} does not exist"));
        }
        self.renumber(|n| {
            if n == a {
                b
            } else if n == b {
                a
            } else {
                n
            }
        });
        println!("Moved Account-{a} to slot {b}");
        0
    }

    fn config(&mut self, rest: &[&str]) -> u8 {
        match rest {
            [] | ["list"] => {
                print!("{}", self.fixture("config-list.txt"));
                0
            }
            ["--json"] | ["list", "--json"] => {
                print!("{}", self.fixture("config-list.json"));
                0
            }
            ["get", key, "--json"] => {
                if *key != "autoswitch.threshold" {
                    return self.envelope("ConfigError", &format!("unknown setting '{key}'"));
                }
                print!("{}", self.fixture("config-get.json"));
                0
            }
            ["set", key, value] => {
                if !key.starts_with("autoswitch.") && !key.starts_with("ui.") {
                    return self.error(&format!("unknown setting '{key}'"));
                }
                println!("{key} = {value}");
                0
            }
            ["unset", key] => {
                println!("{key} unset (default: 90)");
                0
            }
            ["path"] => {
                println!("/home/user/.local/share/claude-swap/settings.json");
                0
            }
            _ => self.usage_error("--json can only be used with 'list', 'status', or 'switch'"),
        }
    }
}
