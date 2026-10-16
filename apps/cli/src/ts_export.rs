//! Tipi TypeScript dei formati della CLI in `apps/vscode/src/generated/`; i tipi del core si
//! importano da `@swapdeck/types`. `cargo test -p swapdeck-cli cli_types` controlla che siano
//! aggiornati; con `SWAPDECK_UPDATE_TYPES=1` li riscrive.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ts_rs::{Config, TS};

use crate::output::{CliErrorEnvelope, CliState, CliVersion, ConfigView, EngineStatus};

const HEADER: &str =
    "// Generato da swapdeck-cli (ts-rs): non modificare a mano. Rigenera con `pnpm gen:types`.\n";

const OWN: [&str; 5] = [
    "CliErrorEnvelope",
    "CliState",
    "CliVersion",
    "ConfigView",
    "EngineStatus",
];

fn generated_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../vscode/src/generated")
}

/// Riscrive `import type { X } from "./X.js";` verso `@swapdeck/types` se X non è della CLI.
fn rewrite_imports(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .map(|line| {
            let name = line
                .strip_prefix("import type { ")
                .and_then(|r| r.split_once(" }"))
                .map(|(n, _)| n);
            match name {
                Some(n) if !OWN.contains(&n) => {
                    format!("import type {{ {n} }} from '@swapdeck/types';")
                }
                _ => line.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn render(dir: &Path) -> BTreeMap<String, String> {
    let cfg = Config::new()
        .with_out_dir(dir)
        .with_import_extension(Some("js".to_owned()));
    CliVersion::export_all(&cfg).unwrap();
    EngineStatus::export_all(&cfg).unwrap();
    CliState::export_all(&cfg).unwrap();
    ConfigView::export_all(&cfg).unwrap();
    CliErrorEnvelope::export_all(&cfg).unwrap();
    let mut files = BTreeMap::new();
    let mut index = String::from(HEADER);
    for name in OWN {
        let text = fs::read_to_string(dir.join(format!("{name}.ts"))).unwrap();
        files.insert(format!("{name}.ts"), rewrite_imports(&text));
        index.push_str(&format!("export type * from './{name}.js';\n"));
    }
    files.insert("index.ts".to_owned(), index);
    files
}

#[test]
fn cli_types_are_current() {
    let tmp = tempfile::tempdir().unwrap();
    let fresh = render(tmp.path());
    let target = generated_dir();
    if std::env::var("SWAPDECK_UPDATE_TYPES").as_deref() == Ok("1") {
        fs::create_dir_all(&target).unwrap();
        for (name, text) in &fresh {
            fs::write(target.join(name), text).unwrap();
        }
        return;
    }
    for (name, text) in &fresh {
        let current = fs::read_to_string(target.join(name))
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            &current == text,
            "{name} non aggiornato: esegui `pnpm gen:types`"
        );
    }
}
