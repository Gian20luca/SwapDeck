//! Lettura in sola lettura delle sessioni di Claude Code (`<claude>/projects/**/*.jsonl`).
//!
//! Di ogni riga si tengono solo orario, id del messaggio, modello e conteggi di token: il resto
//! (testo, strumenti, percorsi) lo salta il parser senza copiarlo. Le righe che non contengono la
//! chiave `"usage":` non si analizzano affatto. Link simbolici mai seguiti, file aperti per handle.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Deserialize;

use super::TokenCounts;
use crate::timeutil::parse_iso;

/// Cartella di Claude Code alternativa (come Claude Code stesso).
pub const CLAUDE_CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";
/// Profondità massima sotto la cartella di un progetto (`<sessione>/subagents/workflows/<wf>/…`).
const MAX_DEPTH: usize = 5;
/// File di sessione considerati al massimo per lettura.
pub const MAX_FILES: usize = 50_000;
/// Righe più lunghe di così si saltano senza tenerle in memoria (risultati enormi di strumenti).
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;
/// Metadati di un sotto-agente letti al massimo.
const MAX_META_BYTES: u64 = 64 * 1024;
/// Descrizione dell'agente tenuta al massimo (caratteri).
const MAX_DESCRIPTION_CHARS: usize = 160;

/// Cartella di Claude Code: `CLAUDE_CONFIG_DIR` se impostata e assoluta, altrimenti `~/.claude`
/// (`%USERPROFILE%\.claude` su Windows).
pub fn claude_dir() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os(CLAUDE_CONFIG_DIR_ENV) {
        let p = PathBuf::from(v);
        if p.is_absolute() {
            return Some(p);
        }
    }
    directories::BaseDirs::new().map(|b| b.home_dir().join(".claude"))
}

/// Nome da mostrare di un progetto: solo l'ultima parte del percorso, ricostruita dal nome della
/// cartella di Claude Code (che scrive il percorso con ogni carattere non alfanumerico mutato in
/// `-`: `C--Progetti-SwapDeck` → `SwapDeck`). Un trattino del nome originale non si distingue da
/// un separatore: `my-app` diventa `app`. Mai il percorso intero nella UI o nei log.
pub fn project_label(folder: &str) -> String {
    folder
        .rsplit('-')
        .find(|s| !s.is_empty())
        .unwrap_or("?")
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect()
}

/// File di sessione trovato.
#[derive(Debug, Clone)]
pub struct SessionFile {
    /// Percorso relativo alla cartella `projects` con `/` (chiave dell'indice).
    pub rel: String,
    pub path: PathBuf,
    pub project: String,
    pub session: String,
    pub agent: Option<String>,
    pub len: u64,
    pub mtime_ms: i64,
}

/// Elenca i `.jsonl` modificati da `cutoff_ms` in poi. `false` se l'elenco è incompleto (cartella
/// illeggibile o troppi file): allora l'indice non dimentica i file non visti.
pub fn discover(projects: &Path, cutoff_ms: i64, out: &mut Vec<SessionFile>) -> bool {
    let Ok(entries) = fs::read_dir(projects) else {
        return false;
    };
    let mut complete = true;
    for entry in entries {
        let Ok(entry) = entry else {
            complete = false;
            continue;
        };
        // `file_type` di una voce non segue i link: link a cartelle e file saltati.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let Some(project) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        complete &= walk(&entry.path(), &project, &[], 0, cutoff_ms, out);
    }
    complete && out.len() < MAX_FILES
}

fn walk(
    dir: &Path,
    project: &str,
    parents: &[String],
    depth: usize,
    cutoff_ms: i64,
    out: &mut Vec<SessionFile>,
) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    let mut complete = true;
    for entry in entries {
        if out.len() >= MAX_FILES {
            return false;
        }
        let Ok(entry) = entry else {
            complete = false;
            continue;
        };
        let Ok(kind) = entry.file_type() else {
            complete = false;
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if kind.is_dir() {
            if depth + 1 < MAX_DEPTH {
                let mut next = parents.to_vec();
                next.push(name);
                complete &= walk(&entry.path(), project, &next, depth + 1, cutoff_ms, out);
            }
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        let Some(stem) = name.strip_suffix(".jsonl") else {
            continue;
        };
        let Ok(meta) = entry.metadata() else {
            complete = false;
            continue;
        };
        let mtime_ms = mtime_ms(&meta);
        if mtime_ms < cutoff_ms {
            continue;
        }
        // Sessione principale: `<progetto>/<sessione>.jsonl`; sotto-agenti:
        // `<progetto>/<sessione>/subagents/[…/]agent-<id>.jsonl`.
        let (session, agent) = match parents.first() {
            None => (stem.to_owned(), None),
            Some(s) => (
                s.clone(),
                Some(stem.strip_prefix("agent-").unwrap_or(stem).to_owned()),
            ),
        };
        let mut rel = String::from(project);
        for p in parents {
            rel.push('/');
            rel.push_str(p);
        }
        rel.push('/');
        rel.push_str(&name);
        out.push(SessionFile {
            rel,
            path: entry.path(),
            project: project.to_owned(),
            session,
            agent,
            len: meta.len(),
            mtime_ms,
        });
    }
    complete
}

pub fn mtime_ms(meta: &fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Apre un file regolare in sola lettura senza seguire link (né reparse point su Windows) e senza
/// bloccare chi lo sta scrivendo.
pub fn open_regular(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // FILE_FLAG_OPEN_REPARSE_POINT; condivisione lettura, scrittura e cancellazione.
        opts.custom_flags(0x0020_0000).share_mode(0x7);
    }
    let file = opts.open(path)?;
    if !file.metadata()?.file_type().is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file di sessione non regolare",
        ));
    }
    Ok(file)
}

/// Identità del file aperto (inode su Unix, ora di creazione su Windows): cambia se il file
/// viene sostituito.
pub fn identity(meta: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.ino()
    }
    #[cfg(not(unix))]
    {
        meta.created()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
    }
}

/// Esito di una lettura incrementale.
#[derive(Debug, Default, Clone, Copy)]
pub struct ReadOutcome {
    /// Primo byte non ancora consumato (dopo l'ultima riga completa).
    pub offset: u64,
    pub lines: u64,
    pub bytes: u64,
    /// Righe illeggibili o troppo lunghe.
    pub skipped: u64,
    /// Lettura fermata da `stop` prima di `end`: il resto alla chiamata dopo.
    pub more: bool,
}

/// Legge le righe complete da `offset` fino a `end` (lunghezza vista all'apertura). L'ultima riga
/// senza a capo si consuma solo se è un JSON valido (file chiuso senza a capo finale), altrimenti
/// si rilegge alla volta dopo (file in scrittura). `on_line` restituisce se la riga era JSON.
pub fn read_lines(
    file: &mut File,
    offset: u64,
    end: u64,
    on_line: impl FnMut(&[u8]) -> bool,
) -> io::Result<ReadOutcome> {
    read_lines_until(file, offset, end, |_| false, on_line)
}

/// Come [`read_lines`], ma dopo ogni riga completa `stop(byte consumati)` può fermare la lettura
/// (budget di byte o di tempo): si ferma sempre a fine riga, quindi avanza di almeno una riga.
pub fn read_lines_until(
    file: &mut File,
    offset: u64,
    end: u64,
    mut stop: impl FnMut(u64) -> bool,
    mut on_line: impl FnMut(&[u8]) -> bool,
) -> io::Result<ReadOutcome> {
    let mut out = ReadOutcome {
        offset,
        ..ReadOutcome::default()
    };
    if end <= offset {
        return Ok(out);
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = BufReader::with_capacity(256 * 1024, file.take(end - offset));
    let mut line: Vec<u8> = Vec::new();
    let mut oversize = false;
    let mut pos = offset;
    loop {
        let buf = reader.fill_buf()?;
        if buf.is_empty() {
            break;
        }
        let (chunk, newline) = match memchr::memchr(b'\n', buf) {
            Some(i) => (&buf[..i], true),
            None => (buf, false),
        };
        if !oversize {
            if line.len() + chunk.len() > MAX_LINE_BYTES {
                oversize = true;
                line.clear();
                line.shrink_to(64 * 1024);
            } else {
                line.extend_from_slice(chunk);
            }
        }
        let used = chunk.len() + usize::from(newline);
        reader.consume(used);
        pos += used as u64;
        if newline {
            out.lines += 1;
            if oversize || (!line.is_empty() && !on_line(&line)) {
                out.skipped += 1;
            }
            line.clear();
            oversize = false;
            out.offset = pos;
            if pos < end && stop(pos - offset) {
                out.more = true;
                break;
            }
        }
    }
    if !line.is_empty()
        && !oversize
        && serde_json::from_slice::<serde::de::IgnoredAny>(&line).is_ok()
    {
        out.lines += 1;
        on_line(&line);
        out.offset = pos;
    }
    line.fill(0);
    out.bytes = out.offset - offset;
    Ok(out)
}

/// Consumo di un messaggio dell'assistente.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub ts: i64,
    pub id: Option<String>,
    pub model: String,
    pub tokens: TokenCounts,
}

#[derive(Deserialize)]
struct RawLine {
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    message: Option<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<RawUsage>,
}

#[derive(Deserialize, Default)]
struct RawUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
}

/// Esito dell'analisi di una riga.
#[derive(Debug, PartialEq)]
pub enum Parsed {
    /// Messaggio dell'assistente con token.
    Usage(Entry),
    /// Riga valida senza consumo.
    Other,
    /// Riga illeggibile (troncata).
    Invalid,
}

/// Analizza una riga. Il testo dei messaggi non si copia: i campi sconosciuti si saltano.
pub fn parse_line(line: &[u8]) -> Parsed {
    // Le righe senza la chiave (messaggi dell'utente, risultati di strumenti) non si analizzano:
    // nel testo dentro una stringa JSON la chiave comparirebbe con le virgolette escapate.
    if memchr::memmem::find(line, b"\"usage\":").is_none() {
        return Parsed::Other;
    }
    let Ok(raw) = serde_json::from_slice::<RawLine>(line) else {
        return Parsed::Invalid;
    };
    if raw.kind.as_deref() != Some("assistant") {
        return Parsed::Other;
    }
    let (Some(msg), Some(ts)) = (raw.message, raw.timestamp.as_deref().and_then(parse_iso)) else {
        return Parsed::Other;
    };
    let Some(u) = msg.usage else {
        return Parsed::Other;
    };
    let tokens = TokenCounts {
        input: u.input_tokens.unwrap_or(0),
        output: u.output_tokens.unwrap_or(0),
        cache_creation: u.cache_creation_input_tokens.unwrap_or(0),
        cache_read: u.cache_read_input_tokens.unwrap_or(0),
    };
    if tokens.total() == 0 {
        return Parsed::Other;
    }
    let model = msg
        .model
        .filter(|m| !m.is_empty() && m.len() <= 80 && m.chars().all(|c| c.is_ascii_graphic()))
        .unwrap_or_else(|| "unknown".to_owned());
    Parsed::Usage(Entry {
        ts,
        id: msg.id.filter(|i| i.len() <= 128),
        model,
        tokens,
    })
}

/// Metadati di un sotto-agente (`agent-<id>.meta.json`): solo tipo e descrizione.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Legge i metadati accanto al file del sotto-agente; `None` se mancano o non sono leggibili.
pub fn read_agent_meta(session_file: &Path, agent: &str) -> Option<AgentMeta> {
    let path = session_file.with_file_name(format!("agent-{agent}.meta.json"));
    let file = open_regular(&path).ok()?;
    let mut buf = Vec::new();
    file.take(MAX_META_BYTES).read_to_end(&mut buf).ok()?;
    let meta: AgentMeta = serde_json::from_slice(&buf).ok()?;
    Some(AgentMeta {
        agent_type: meta.agent_type.map(|t| clean(&t, 60)),
        description: meta
            .description
            .map(|d| clean(&d, MAX_DESCRIPTION_CHARS))
            .filter(|d| !d.is_empty()),
    })
}

fn clean(text: &str, max: usize) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}
