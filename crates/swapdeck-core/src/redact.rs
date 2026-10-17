//! Redazione centralizzata di testi destinati a log, diagnostica e messaggi d'errore:
//! niente token, segreti o email complete (le email restano riconoscibili come `ma***@dominio`).

/// Chiavi che precedono un segreto in `chiave=valore` o `chiave: valore`.
const SECRET_KEYS: [&str; 9] = [
    "token",
    "secret",
    "password",
    "passwd",
    "apikey",
    "api_key",
    "api-key",
    "authorization",
    "bearer",
];

/// Testo su una riga, senza sequenze ANSI né caratteri di controllo, con token e segreti
/// oscurati ed email mascherate. Gli spazi multipli si riducono a uno.
pub fn redact(raw: &str) -> String {
    let text = strip_control(raw);
    let mut out: Vec<String> = Vec::new();
    // `Bearer xyz` e `token: xyz`: si oscura la parola che segue la chiave.
    let mut hide_next = false;
    for word in text.split_whitespace() {
        let lower = word.to_ascii_lowercase();
        if hide_next {
            // `Authorization: Bearer xyz`: lo schema resta, il valore no.
            hide_next = lower == "bearer";
            out.push(if hide_next { word } else { "***" }.to_owned());
            continue;
        }
        let bare = lower.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        if SECRET_KEYS.contains(&bare) && (lower.ends_with(':') || bare == "bearer") {
            hide_next = true;
            out.push(word.to_owned());
            continue;
        }
        out.push(redact_word(word));
    }
    out.join(" ")
}

/// Toglie le sequenze ANSI (`ESC [ … lettera`) e sostituisce i caratteri di controllo con spazi.
pub fn strip_control(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(if c.is_control() { ' ' } else { c });
    }
    out
}

/// Una parola senza spazi: token Anthropic, `chiave=valore` segreti, stringhe lunghe con lettere
/// e cifre (possibili token) ed email.
pub fn redact_word(word: &str) -> String {
    if let Some(pos) = word.find("sk-ant-") {
        return format!("{}sk-ant-***", &word[..pos]);
    }
    if let Some((key, _)) = word.split_once('=') {
        let k = key
            .to_ascii_lowercase()
            .trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
            .to_owned();
        if SECRET_KEYS.iter().any(|s| k.ends_with(s)) {
            return format!("{key}=***");
        }
    }
    let masked = mask_token_runs(word);
    if masked != word {
        return masked;
    }
    let core = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '@');
    if let Some((local, domain)) = core.split_once('@') {
        if !local.is_empty() && domain.contains('.') {
            return word.replace(core, &mask_email(local, domain));
        }
    }
    word.to_owned()
}

/// Tratti lunghi (40+) di lettere, cifre e `-_.=+` con almeno una lettera e una cifra: possibili
/// token, anche dentro JSON o tra virgolette. I percorsi (`/`, `\`) spezzano i tratti.
fn mask_token_runs(word: &str) -> String {
    let in_run = |c: char| c.is_ascii_alphanumeric() || "-_.=+".contains(c);
    let mut out = String::with_capacity(word.len());
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        let token = run.len() >= 40
            && run.chars().any(|c| c.is_ascii_digit())
            && run.chars().any(|c| c.is_ascii_alphabetic());
        out.push_str(if token { "***" } else { run.as_str() });
        run.clear();
    };
    for c in word.chars() {
        if in_run(c) {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
            out.push(c);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// `mario.rossi@example.com` → `ma***@example.com`.
pub fn mask_email(local: &str, domain: &str) -> String {
    let keep: String = local.chars().take(2).collect();
    format!("{keep}***@{domain}")
}

/// Sostituisce la cartella home dell'utente con `~` (nome utente fuori dalla diagnostica).
pub fn redact_home(text: &str, home: Option<&std::path::Path>) -> String {
    let Some(home) = home.and_then(|h| h.to_str()).filter(|h| h.len() > 1) else {
        return text.to_owned();
    };
    let mut out = text.replace(home, "~");
    // Windows: la stessa cartella con le barre normali (percorsi riportati da strumenti diversi).
    if cfg!(windows) {
        out = out.replace(&home.replace('\\', "/"), "~");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_secrets_and_emails_never_survive() {
        let raw = "\u{1b}[31mlogin sk-ant-oat01-ABCDEF per mario.rossi@example.com\u{1b}[0m\n\
                   refresh_token=abc123 password=hunter2 Authorization: Bearer xyz.987 \
                   token: corto {\"x\":\"aB3aB3aB3aB3aB3aB3aB3aB3aB3aB3aB3aB3aB3aB3\"}";
        let r = redact(raw);
        for secret in [
            "ABCDEF",
            "mario.rossi",
            "abc123",
            "hunter2",
            "xyz.987",
            "corto",
            "aB3aB3aB3aB3",
        ] {
            assert!(!r.contains(secret), "{secret} in {r}");
        }
        assert!(r.contains("sk-ant-***"));
        assert!(r.contains("ma***@example.com"));
        assert!(r.contains("refresh_token=***") && r.contains("password=***"));
        assert!(!r.contains('\n') && !r.contains('\u{1b}'));
    }

    #[test]
    fn ordinary_text_and_paths_stay_readable() {
        let s = "CAMBIO ESEGUITO #1 -> #3 (principale al limite) /home/a/.local/bin/cswap";
        assert_eq!(redact(s), s);
        assert_eq!(redact("tokens: 12"), "tokens: 12");
        assert_eq!(redact("a@b"), "a@b");
    }

    #[test]
    fn home_is_replaced() {
        let home = if cfg!(windows) {
            std::path::Path::new(r"C:\Users\Mario")
        } else {
            std::path::Path::new("/home/mario")
        };
        let text = format!("log in {}", home.join("x").display());
        let r = redact_home(&text, Some(home));
        assert!(!r.contains("Mario") && !r.contains("mario"), "{r}");
        assert!(r.starts_with("log in ~"));
        assert_eq!(redact_home("x", None), "x");
    }
}
