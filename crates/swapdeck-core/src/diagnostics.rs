//! Testo di diagnostica da incollare in una segnalazione: versioni di SwapDeck e cswap, sistema
//! operativo, stato del motore, mappa attiva e ultime righe del log. Lo stesso testo esce
//! dall'app («Copia diagnostica»), dall'estensione VS Code e da `swapdeck-cli diagnostics`.
//!
//! Tutto passa dalla redazione del core (token, segreti, email) e la cartella home diventa `~`.
//! Su cswap solo comandi in lettura (`--version`, `list --json`, via [`Setup::diagnose`]).

use std::path::{Path, PathBuf};

use crate::applog::{self, logs_dir};
use crate::config::{self, Language};
use crate::lock;
use crate::paths::AppPaths;
use crate::redact::{redact, redact_home};
use crate::setup::{Setup, SetupDiagnosis};
use crate::timeutil::{format_iso, now_unix};

/// Righe di log riportate.
pub const LOG_LINES: usize = 200;

/// Dati raccolti (separati dalla resa per i test).
#[derive(Debug, Clone, Default)]
pub struct DiagnosticsData {
    pub generated_at: i64,
    /// Chi genera il testo (`desktop`, `cli`, `vscode`).
    pub component: String,
    /// Dettagli di chi ospita (es. versione di VS Code e dell'estensione).
    pub client: Option<String>,
    pub os: String,
    pub setup: Option<SetupDiagnosis>,
    pub engine_enabled: Option<bool>,
    pub dry_run: Option<bool>,
    pub interval_secs: Option<u32>,
    pub active_map: Option<String>,
    /// `Some(Some(pid))` motore vivo, `Some(None)` nessun lock valido, `None` lock illeggibile.
    pub engine_pid: Option<Option<u32>>,
    pub config_problem: Option<String>,
    pub logs_dir: Option<PathBuf>,
    pub log_lines: Vec<String>,
    /// Righe in più del chiamante (es. avvio automatico dell'app).
    pub extra: Vec<(String, String)>,
    pub language: Language,
}

/// Raccoglie i dati (`paths` = cartelle dell'utente o `SWAPDECK_HOME`), con la diagnosi di cswap.
pub fn collect(
    paths: Option<&AppPaths>,
    component: &str,
    client: Option<&str>,
    extra: Vec<(String, String)>,
) -> DiagnosticsData {
    let data_dir = paths.map_or_else(std::env::temp_dir, |p| p.data_dir.clone());
    let setup = Setup::new(data_dir).diagnose();
    gather(paths, component, client, extra, Some(setup))
}

/// Come [`collect`], con la diagnosi di cswap già fatta (o assente).
pub fn gather(
    paths: Option<&AppPaths>,
    component: &str,
    client: Option<&str>,
    extra: Vec<(String, String)>,
    setup: Option<SetupDiagnosis>,
) -> DiagnosticsData {
    let mut d = DiagnosticsData {
        generated_at: now_unix(),
        component: component.to_owned(),
        client: client.map(str::to_owned),
        os: format!(
            "{} ({} {})",
            applog::os_description(),
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        setup,
        extra,
        ..DiagnosticsData::default()
    };
    let Some(paths) = paths else {
        d.config_problem = Some("cartelle dell'utente non determinabili".to_owned());
        return d;
    };
    match config::load(&paths.config_file()) {
        Ok(loaded) => {
            let c = loaded.config;
            d.engine_enabled = Some(c.engine.enabled);
            d.dry_run = Some(c.engine.dry_run);
            d.interval_secs = Some(c.poll_interval_seconds);
            d.active_map = c.engine_profiles.active().map(|p| p.name.clone());
            d.language = c.language;
        }
        Err(e) => d.config_problem = Some(e.to_string()),
    }
    d.engine_pid = lock::read(&paths.data_dir).ok().map(|info| {
        info.filter(|i| !i.is_stale(now_unix(), &lock::host_name()))
            .map(|i| i.pid)
    });
    let dir = logs_dir(paths);
    d.log_lines = applog::tail(&dir, LOG_LINES).unwrap_or_default();
    d.logs_dir = Some(dir);
    d
}

fn labels(lang: Language) -> [&'static str; 20] {
    match lang {
        Language::It => [
            "Diagnostica di SwapDeck",
            "Generata",
            "Componente",
            "Sistema operativo",
            "cswap",
            "non trovato",
            "Stato di cswap",
            "account",
            "uv",
            "Motore",
            "acceso",
            "spento",
            "solo osservazione",
            "Processo del motore",
            "nessuno",
            "Mappa attiva",
            "Cartella dei log",
            "Ultime righe del log",
            "sì",
            "no",
        ],
        Language::En => [
            "SwapDeck diagnostics",
            "Generated",
            "Component",
            "Operating system",
            "cswap",
            "not found",
            "cswap status",
            "accounts",
            "uv",
            "Engine",
            "on",
            "off",
            "observe only",
            "Engine process",
            "none",
            "Active map",
            "Logs folder",
            "Last log lines",
            "yes",
            "no",
        ],
    }
}

/// Testo finale, redatto (token, segreti, email, cartella home).
pub fn render(d: &DiagnosticsData, home: Option<&Path>) -> String {
    let [
        title,
        generated,
        component,
        os,
        cswap,
        not_found,
        cswap_status,
        accounts,
        uv,
        engine,
        on,
        off,
        observe,
        engine_proc,
        none,
        active_map,
        logs_folder,
        last_lines,
        yes,
        no,
    ] = labels(d.language);
    let unknown = "?";
    let mut out: Vec<String> = vec![
        title.to_owned(),
        format!("{generated}: {}", format_iso(d.generated_at)),
        format!(
            "SwapDeck: {} ({component}: {})",
            crate::version(),
            d.component
        ),
    ];
    if let Some(client) = &d.client {
        out.push(format!("Client: {client}"));
    }
    out.push(format!("{os}: {}", d.os));
    match &d.setup {
        Some(s) => {
            match &s.cswap {
                Some(t) => out.push(format!(
                    "{cswap}: {} ({})",
                    t.version.as_deref().unwrap_or(unknown),
                    t.path
                )),
                None => out.push(format!("{cswap}: {not_found}")),
            }
            let status = serde_json::to_value(s.status)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            let mut line = format!("{cswap_status}: {status}");
            if let Some(n) = s.account_count {
                line.push_str(&format!(", {accounts} {n}"));
            }
            if let Some(v) = s.schema_version {
                line.push_str(&format!(", schema {v}/{}", s.supported_schema_version));
            }
            if let Some(e) = &s.error {
                line.push_str(&format!(", {}", e.log_summary()));
            }
            out.push(line);
            if let Some(t) = &s.uv {
                out.push(format!(
                    "{uv}: {} ({})",
                    t.version.as_deref().unwrap_or(unknown),
                    t.path
                ));
            }
        }
        None => out.push(format!("{cswap}: {unknown}")),
    }
    let flag = |v: Option<bool>| match v {
        Some(true) => on,
        Some(false) => off,
        None => unknown,
    };
    out.push(format!(
        "{engine}: {}, {observe}: {}, {}s",
        flag(d.engine_enabled),
        match d.dry_run {
            Some(true) => yes,
            Some(false) => no,
            None => unknown,
        },
        d.interval_secs
            .map_or_else(|| unknown.to_owned(), |s| s.to_string())
    ));
    out.push(format!(
        "{engine_proc}: {}",
        match d.engine_pid {
            Some(Some(pid)) => format!("PID {pid}"),
            Some(None) => none.to_owned(),
            None => unknown.to_owned(),
        }
    ));
    out.push(format!(
        "{active_map}: {}",
        d.active_map.as_deref().unwrap_or(none)
    ));
    if let Some(p) = &d.config_problem {
        out.push(format!("Config: {p}"));
    }
    for (k, v) in &d.extra {
        out.push(format!("{k}: {v}"));
    }
    if let Some(dir) = &d.logs_dir {
        out.push(format!("{logs_folder}: {}", dir.display()));
    }
    out.push(String::new());
    out.push(format!("--- {last_lines} ({}) ---", d.log_lines.len()));
    out.extend(d.log_lines.iter().cloned());
    let text: Vec<String> = out.iter().map(|l| redact_home(&redact(l), home)).collect();
    let mut s = text.join("\n");
    s.push('\n');
    s
}

/// Cartella home dell'utente (per la redazione).
pub fn home_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf())
}

/// Raccolta e resa in un passo.
pub fn report(
    paths: Option<&AppPaths>,
    component: &str,
    client: Option<&str>,
    extra: Vec<(String, String)>,
) -> String {
    render(
        &collect(paths, component, client, extra),
        home_dir().as_deref(),
    )
}

/// Apre una cartella nel file manager del sistema (creandola se manca). Nessuna shell: il
/// percorso è un argomento a sé.
pub fn open_folder(dir: &Path) -> std::io::Result<()> {
    crate::paths::create_private_dir(dir)?;
    let program = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut cmd = std::process::Command::new(program);
    cmd.arg(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    // Niente attesa: explorer.exe esce con 1 anche quando apre la cartella.
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{SetupStatus, ToolInfo};

    fn sample(home: &Path) -> DiagnosticsData {
        DiagnosticsData {
            generated_at: 1_790_000_000,
            component: "desktop".into(),
            client: Some("VS Code 1.99".into()),
            os: "Linux (linux x86_64)".into(),
            setup: Some(SetupDiagnosis {
                status: SetupStatus::Ready,
                cswap: Some(ToolInfo {
                    path: home.join(".local/bin/cswap").display().to_string(),
                    version: Some("0.27.2".into()),
                }),
                uv: None,
                min_version: "0.26.0".into(),
                schema_version: Some(1),
                supported_schema_version: 1,
                account_count: Some(3),
                error: None,
            }),
            engine_enabled: Some(true),
            dry_run: Some(false),
            interval_secs: Some(120),
            active_map: Some("Lavoro di mario.rossi@example.com".into()),
            engine_pid: Some(Some(4242)),
            config_problem: None,
            logs_dir: Some(home.join("data/logs")),
            log_lines: vec!["riga con sk-ant-oat01-XYZ e token=abc".into()],
            extra: vec![("Avvio automatico".into(), "sì".into())],
            language: Language::It,
        }
    }

    #[test]
    fn render_has_every_section_and_no_secrets() {
        let home = if cfg!(windows) {
            PathBuf::from(r"C:\Users\Mario")
        } else {
            PathBuf::from("/home/mario")
        };
        let text = render(&sample(&home), Some(&home));
        for part in [
            "Diagnostica di SwapDeck",
            "SwapDeck: ",
            "Client: VS Code 1.99",
            "Sistema operativo: Linux",
            "cswap: 0.27.2 (~",
            "Stato di cswap: ready, account 3, schema 1/1",
            "Motore: acceso, solo osservazione: no, 120s",
            "Processo del motore: PID 4242",
            "Mappa attiva: Lavoro di ma***@example.com",
            "Avvio automatico: sì",
            "Cartella dei log: ~",
            "--- Ultime righe del log (1) ---",
        ] {
            assert!(text.contains(part), "manca «{part}» in:\n{text}");
        }
        for secret in ["Mario", "mario", "XYZ", "abc"] {
            assert!(!text.contains(secret), "{secret} in:\n{text}");
        }
        let mut en = sample(&home);
        en.language = Language::En;
        en.engine_pid = Some(None);
        let text = render(&en, Some(&home));
        assert!(text.contains("SwapDeck diagnostics") && text.contains("Engine process: none"));
    }

    #[test]
    fn collect_reads_config_lock_and_log() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(tmp.path());
        paths.ensure().unwrap();
        std::fs::write(
            paths.config_file(),
            r#"{"schemaVersion":1,"language":"en","engine":{"enabled":false}}"#,
        )
        .unwrap();
        let log = applog::LogFile::new(logs_dir(&paths), "cli");
        log.write(applog::Level::Info, "prima riga").unwrap();
        let d = gather(Some(&paths), "cli", None, Vec::new(), None);
        assert_eq!(d.engine_enabled, Some(false));
        assert_eq!(d.language, Language::En);
        assert_eq!(d.engine_pid, Some(None));
        assert_eq!(d.log_lines.len(), 1);
        assert!(d.log_lines[0].ends_with("prima riga"));
    }
}
