//! Errori di cswap e loro classificazione (contratto, §2 e §7).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use crate::redact::mask_email;
use crate::redact::{redact, strip_control};

use super::model::{Account, ErrorEnvelope, ListResponse, SUPPORTED_SCHEMA_VERSION, UsageStatus};

/// Età oltre la quale un dato `ok` è considerato vecchio (cadenza normale di cswap).
pub const FRESH_USAGE_MAX_AGE_SECONDS: f64 = 300.0;

/// Lunghezza massima del messaggio ripulito.
const MAX_MESSAGE_CHARS: usize = 300;

/// Classe di un errore o di un problema di un account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CswapErrorKind {
    /// Eseguibile non trovato.
    NotInstalled,
    /// Versione sotto la minima o illeggibile.
    UnsupportedVersion,
    /// `schemaVersion` del JSON assente o diverso da quello supportato (per esempio dopo un aggiornamento).
    UnsupportedSchema,
    /// Il processo non ha risposto entro il tempo massimo.
    Timeout,
    /// L'endpoint dei consumi ha risposto 429.
    RateLimited,
    /// Consumi vecchi o non disponibili.
    StaleUsage,
    /// Token attivo scaduto, rinnovo rimandato: si risolve da solo.
    TokenExpired,
    /// Serve un nuovo `/login` in Claude Code e poi `cswap add`.
    ReloginRequired,
    /// Lock di cswap o di Claude Code occupato: ritentare.
    Busy,
    /// Uso errato (argparse, uscita 2).
    InvalidUsage,
    /// Interrotto (uscita 130).
    Interrupted,
    /// Prompt annullato o azione senza effetto verificato.
    Cancelled,
    /// Comando che scrive interrotto per timeout: non si sa se ha avuto effetto
    /// (l'errore porta l'elenco riletto, se leggibile).
    Uncertain,
    /// Errore generico.
    Failed,
}

/// Errore di un comando cswap, pronto per l'interfaccia (niente token, email mascherate).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CswapError {
    pub kind: CswapErrorKind,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exit_code: Option<i32>,
    /// `error.type` della busta JSON (nome della classe Python), se c'è.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_type: Option<String>,
    /// Elenco riletto dopo un esito incerto (`uncertain`), se leggibile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub list: Option<Box<ListResponse>>,
}

impl CswapError {
    /// Errore con messaggio ripulito.
    pub fn new(kind: CswapErrorKind, message: impl AsRef<str>) -> Self {
        Self {
            kind,
            message: clean_message(message.as_ref()),
            exit_code: None,
            error_type: None,
            list: None,
        }
    }

    /// Esito incerto dopo il timeout di un comando che scrive, con l'elenco riletto.
    pub fn uncertain(cause: &CswapError, list: Option<ListResponse>) -> Self {
        let mut err = Self::new(
            CswapErrorKind::Uncertain,
            format!(
                "esito incerto ({}): controllare l'elenco degli account",
                cause.message
            ),
        );
        err.list = list.map(Box::new);
        err
    }

    fn with_exit(mut self, code: Option<i32>) -> Self {
        self.exit_code = code;
        self
    }
}

impl CswapError {
    /// Riga per il file di log: classe, codice di uscita e tipo (se noti), messaggio ripulito.
    pub fn log_summary(&self) -> String {
        let mut s = format!("{:?}", self.kind);
        if let Some(code) = self.exit_code {
            s.push_str(&format!(" (codice {code})"));
        }
        if let Some(t) = &self.error_type {
            s.push_str(&format!(" [{t}]"));
        }
        format!("{s}: {}", self.message)
    }
}

impl std::fmt::Display for CswapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for CswapError {}

/// Versione di cswap letta da `--version`.
///
/// `pre_release` conserva il segno di una pre-release PEP 440 (`a`, `b`, `rc`, `dev`):
/// `X.Y.Z` stabile è più nuova di `X.Y.Zrc1`. Le post-release (`.post1`, `-1`) e le
/// versioni locali (`+abc`) contano come la stabile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct CswapVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    #[serde(skip)]
    #[ts(skip)]
    pub pre_release: bool,
}

impl Ord for CswapVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let key = |v: &Self| (v.major, v.minor, v.patch, !v.pre_release);
        key(self).cmp(&key(other))
    }
}

impl PartialOrd for CswapVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl CswapVersion {
    /// Minima supportata.
    pub const MIN_SUPPORTED: Self = Self {
        major: 0,
        minor: 26,
        patch: 0,
        pre_release: false,
    };

    /// Legge `^\S+ (\d+)\.(\d+)\.(\d+)` (es. `cswap 0.27.2`) dalla prima riga non vuota.
    pub fn parse(output: &str) -> Option<Self> {
        Self::parse_number(Self::token(output)?)
    }

    /// Parola della versione così com'è (es. `0.28.0rc1`) dalla prima riga non vuota, se
    /// è leggibile e fatta solo di caratteri ammessi da PEP 440.
    pub fn token(output: &str) -> Option<&str> {
        let line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
        let mut words = line.split_whitespace();
        words.next()?;
        let ver = words.next()?;
        let allowed =
            |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+' | '!');
        (ver.len() <= 64 && ver.chars().all(allowed) && Self::parse_number(ver).is_some())
            .then_some(ver)
    }

    /// Versione `X.Y.Z` con eventuale suffisso (es. `0.28.0rc1`, `0.28.0.post1`).
    pub fn parse_number(ver: &str) -> Option<Self> {
        let mut parts = ver.trim().splitn(3, '.');
        let num = |s: Option<&str>| -> Option<u32> {
            let digits: String = s?.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().ok()
        };
        let major = num(parts.next())?;
        let minor = num(parts.next())?;
        let rest = parts.next()?;
        let patch = num(Some(rest))?;
        let suffix = rest
            .trim_start_matches(|c: char| c.is_ascii_digit())
            .trim_start_matches(['.', '-', '_'])
            .to_ascii_lowercase();
        let pre_release = ["a", "b", "c", "rc", "pre", "dev"]
            .iter()
            .any(|p| suffix.starts_with(p));
        Some(Self {
            major,
            minor,
            patch,
            pre_release,
        })
    }

    /// `true` se la versione è supportata.
    pub fn is_supported(self) -> bool {
        self >= Self::MIN_SUPPORTED
    }
}

impl std::fmt::Display for CswapVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Errore per uno `schemaVersion` diverso da quello supportato, se lo è.
pub fn check_schema_version(found: u32) -> Result<(), CswapError> {
    if found == SUPPORTED_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(CswapError::new(
            CswapErrorKind::UnsupportedSchema,
            format!("schemaVersion {found} non supportato (atteso {SUPPORTED_SCHEMA_VERSION})"),
        ))
    }
}

/// Classifica un'uscita di cswap non riuscita.
///
/// `exit_code` è `None` se il processo è stato terminato da un segnale.
pub fn classify_failure(exit_code: Option<i32>, stdout: &str, stderr: &str) -> CswapError {
    if exit_code == Some(130) {
        return CswapError::new(CswapErrorKind::Interrupted, "interrotto").with_exit(exit_code);
    }
    if exit_code == Some(2) {
        let msg = stderr
            .lines()
            .rev()
            .find_map(|l| l.split_once(": error: ").map(|(_, m)| m.to_owned()))
            .unwrap_or_else(|| last_line(stderr));
        return CswapError::new(CswapErrorKind::InvalidUsage, msg).with_exit(exit_code);
    }
    if let Some(env) = parse_error_envelope(stdout) {
        return from_envelope(env, exit_code);
    }
    let filtered: String = stderr
        .lines()
        .filter(|l| !l.contains("A newer version of claude-swap"))
        .collect::<Vec<_>>()
        .join("\n");
    let stderr = filtered.as_str();
    let message = if stderr.contains("Traceback (most recent call last)") {
        last_line(stderr)
    } else {
        stderr
            .lines()
            .find_map(|l| l.trim().strip_prefix("Error:").map(str::to_owned))
            .unwrap_or_else(|| {
                let s = last_line(stderr);
                if s.is_empty() { last_line(stdout) } else { s }
            })
    };
    let message = if message.trim().is_empty() {
        format!(
            "cswap è uscito con codice {}",
            exit_code.map_or("?".into(), |c| c.to_string())
        )
    } else {
        message
    };
    CswapError::new(kind_from("", &message), message).with_exit(exit_code)
}

/// Classifica un'uscita non riuscita di `uv tool upgrade`: uv esce con 2 per quasi
/// tutti gli errori, quindi niente `InvalidUsage`; `Busy` se un file è in uso
/// (cswap in esecuzione su Windows), altrimenti `Failed`.
pub fn classify_uv_failure(exit_code: Option<i32>, stdout: &str, stderr: &str) -> CswapError {
    if exit_code == Some(130) {
        return CswapError::new(CswapErrorKind::Interrupted, "interrotto").with_exit(exit_code);
    }
    let all = format!("{stderr}\n{stdout}");
    let lower = all.to_ascii_lowercase();
    let kind = if lower.contains("os error 32") || lower.contains("used by another process") {
        CswapErrorKind::Busy
    } else {
        CswapErrorKind::Failed
    };
    let message = stderr
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix("error:").map(|m| m.trim().to_owned()))
        .unwrap_or_else(|| last_line(&all));
    let message = if message.is_empty() {
        format!(
            "uv è uscito con codice {}",
            exit_code.map_or("?".into(), |c| c.to_string())
        )
    } else {
        message
    };
    CswapError::new(kind, message).with_exit(exit_code)
}

/// Errore dalla busta JSON di cswap.
pub fn from_envelope(env: ErrorEnvelope, exit_code: Option<i32>) -> CswapError {
    let mut err = CswapError::new(
        kind_from(&env.error.error_type, &env.error.message),
        &env.error.message,
    )
    .with_exit(exit_code);
    err.error_type = Some(env.error.error_type);
    err
}

/// Busta d'errore JSON su stdout, se c'è (anche come ultima riga di `auto --json`).
pub fn parse_error_envelope(stdout: &str) -> Option<ErrorEnvelope> {
    let trimmed = stdout.trim();
    if let Ok(env) = serde_json::from_str::<ErrorEnvelope>(trimmed) {
        return Some(env);
    }
    trimmed
        .lines()
        .rev()
        .find_map(|l| serde_json::from_str::<ErrorEnvelope>(l.trim()).ok())
}

fn kind_from(error_type: &str, message: &str) -> CswapErrorKind {
    let m = message.to_ascii_lowercase();
    match error_type {
        "LockError" | "ClaudeCodeLockTimeout" => return CswapErrorKind::Busy,
        _ => {}
    }
    if m.contains("acquire lock") || m.contains("refreshing credentials") {
        CswapErrorKind::Busy
    } else if m.contains("please log in first")
        || m.contains("invalid_grant")
        || m.contains("relogin")
        || m.contains("re-login")
    {
        CswapErrorKind::ReloginRequired
    } else if m.contains("token expired") || m.contains("token has expired") {
        CswapErrorKind::TokenExpired
    } else if m.contains("http-429") || m.contains("rate limit") || m.contains("429 too many") {
        CswapErrorKind::RateLimited
    } else {
        CswapErrorKind::Failed
    }
}

/// Problema dei consumi di un account, se c'è (per avvisi e per il motore).
pub fn account_issue(account: &Account) -> Option<CswapErrorKind> {
    match account.usage_status {
        UsageStatus::Ok => match account.usage_age_seconds {
            Some(age) if age > FRESH_USAGE_MAX_AGE_SECONDS => Some(CswapErrorKind::StaleUsage),
            _ => None,
        },
        UsageStatus::Unavailable => {
            if account.usage_error.as_deref() == Some("http-429") {
                Some(CswapErrorKind::RateLimited)
            } else {
                Some(CswapErrorKind::StaleUsage)
            }
        }
        UsageStatus::TokenExpired => Some(CswapErrorKind::TokenExpired),
        UsageStatus::ReloginRequired => Some(CswapErrorKind::ReloginRequired),
        UsageStatus::ApiKey | UsageStatus::ForeignCredential => None,
        UsageStatus::KeychainUnavailable | UsageStatus::NoCredentials | UsageStatus::Unknown => {
            Some(CswapErrorKind::Failed)
        }
    }
}

fn last_line(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or_default()
        .to_owned()
}

/// Ripulisce un messaggio: niente sequenze ANSI né caratteri di controllo, niente
/// `Error:` iniziale, token oscurati, email mascherate, lunghezza limitata.
pub fn clean_message(raw: &str) -> String {
    let no_ansi = strip_control(raw);
    let text = no_ansi.trim();
    let text = text.strip_prefix("Error:").unwrap_or(text).trim();
    let joined = redact(text);
    if joined.chars().count() > MAX_MESSAGE_CHARS {
        let mut cut: String = joined.chars().take(MAX_MESSAGE_CHARS).collect();
        cut.push('…');
        cut
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parsing() {
        let v = CswapVersion::parse("cswap 0.27.2\n").unwrap();
        assert_eq!(v.to_string(), "0.27.2");
        assert!(v.is_supported());
        assert!(
            !CswapVersion::parse("claude-swap 0.25.9")
                .unwrap()
                .is_supported()
        );
        let rc = CswapVersion::parse("cswap 1.0.0rc1").unwrap();
        assert!(rc.is_supported() && rc.pre_release);
        assert_eq!(CswapVersion::token("cswap 1.0.0rc1\n"), Some("1.0.0rc1"));
        assert!(CswapVersion::token("cswap 1.0.0;rm").is_none());
        let stable = CswapVersion::parse_number("1.0.0").unwrap();
        assert!(stable > rc, "la stabile è più nuova della pre-release");
        for pre in ["1.0.0a1", "1.0.0b2", "1.0.0.dev0", "1.0.0-rc.1"] {
            assert!(CswapVersion::parse_number(pre).unwrap() < stable, "{pre}");
        }
        for post in ["1.0.0.post1", "1.0.0+local", "1.0.0-1"] {
            let v = CswapVersion::parse_number(post).unwrap();
            assert!(!v.pre_release && v == stable, "{post}");
        }
        assert!(CswapVersion::parse_number("1.0.1rc1").unwrap() > stable);
        assert!(CswapVersion::parse("garbage").is_none());
        assert!(CswapVersion::parse("").is_none());
    }

    #[test]
    fn message_cleaning() {
        let m = clean_message(
            "\u{1b}[31mError: bad token sk-ant-oat01-ABCDEF for mario.rossi@example.com\u{1b}[0m",
        );
        assert_eq!(m, "bad token sk-ant-*** for ma***@example.com");
        let long = format!("x {}", "aB3".repeat(20));
        assert_eq!(clean_message(&long), "x ***");
        assert!(clean_message(&"a ".repeat(400)).ends_with('…'));
    }

    #[test]
    fn failure_classes() {
        assert_eq!(
            classify_failure(Some(130), "", "").kind,
            CswapErrorKind::Interrupted
        );
        let e = classify_failure(
            Some(2),
            "",
            "usage: cswap <command>\ncswap: error: --hold can only be used with 'import-usage'\n",
        );
        assert_eq!(e.kind, CswapErrorKind::InvalidUsage);
        assert_eq!(e.message, "--hold can only be used with 'import-usage'");
        let e = classify_failure(
            Some(1),
            "",
            "Traceback (most recent call last):\n  File x\nEOFError: EOF when reading a line\n",
        );
        assert_eq!(e.message, "EOFError: EOF when reading a line");
        let e = classify_failure(
            Some(1),
            "",
            "Error: No active Claude account found. Please log in first.\n",
        );
        assert_eq!(e.kind, CswapErrorKind::ReloginRequired);
        let e = classify_failure(
            Some(1),
            "",
            "Error: Failed to acquire lock - another instance may be running\n",
        );
        assert_eq!(e.kind, CswapErrorKind::Busy);
        let e = classify_failure(Some(1), "", "");
        assert_eq!(e.kind, CswapErrorKind::Failed);
        assert!(e.message.contains('1'));
    }
}
