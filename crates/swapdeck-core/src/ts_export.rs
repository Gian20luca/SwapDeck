//! Esportazione dei tipi in TypeScript (`packages/types/src/generated/`).
//!
//! `cargo test -p swapdeck-core generated_types` controlla che i file siano aggiornati;
//! con `SWAPDECK_UPDATE_TYPES=1` (o `pnpm gen:types`) li riscrive.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ts_rs::{Config, TS};

use crate::config::{AppConfig, AppPreferences, PreferencesPatch};
use crate::conflicts::Conflict;
use crate::cswap::{
    ActionOutcome, AutoEvent, CommandSpec, ConfigGetResponse, ConfigListResponse, Confirmation,
    CswapAction, CswapError, CswapVersion, DirectoryMapping, ErrorEnvelope, ListResponse,
    StatusResponse, SwitchResponse,
};
use crate::engine::{
    Decision, EngineMemory, EngineProfiles, EngineRules, EngineState, EngineView, Evaluation,
    ProfileCommand,
};
use crate::history::{HistoryEvents, HistoryQuery, HistoryRecord};
use crate::lock::LockInfo;
use crate::setup::{SetupDiagnosis, SetupOutcome, SetupProgress, UpdateInfo};

const HEADER: &str =
    "// Generato da swapdeck-core (ts-rs): non modificare a mano. Rigenera con `pnpm gen:types`.\n";

fn generated_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/types/src/generated")
}

fn render(dir: &Path) -> BTreeMap<String, String> {
    let cfg = Config::new()
        .with_out_dir(dir)
        .with_import_extension(Some("js".to_owned()));
    ListResponse::export_all(&cfg).unwrap();
    StatusResponse::export_all(&cfg).unwrap();
    SwitchResponse::export_all(&cfg).unwrap();
    ErrorEnvelope::export_all(&cfg).unwrap();
    ConfigListResponse::export_all(&cfg).unwrap();
    ConfigGetResponse::export_all(&cfg).unwrap();
    AutoEvent::export_all(&cfg).unwrap();
    CswapAction::export_all(&cfg).unwrap();
    ActionOutcome::export_all(&cfg).unwrap();
    CommandSpec::export_all(&cfg).unwrap();
    Confirmation::export_all(&cfg).unwrap();
    CswapError::export_all(&cfg).unwrap();
    CswapVersion::export_all(&cfg).unwrap();
    AppConfig::export_all(&cfg).unwrap();
    SetupDiagnosis::export_all(&cfg).unwrap();
    SetupProgress::export_all(&cfg).unwrap();
    SetupOutcome::export_all(&cfg).unwrap();
    UpdateInfo::export_all(&cfg).unwrap();
    EngineRules::export_all(&cfg).unwrap();
    EngineMemory::export_all(&cfg).unwrap();
    Decision::export_all(&cfg).unwrap();
    Conflict::export_all(&cfg).unwrap();
    HistoryRecord::export_all(&cfg).unwrap();
    LockInfo::export_all(&cfg).unwrap();
    DirectoryMapping::export_all(&cfg).unwrap();
    AppPreferences::export_all(&cfg).unwrap();
    PreferencesPatch::export_all(&cfg).unwrap();
    EngineView::export_all(&cfg).unwrap();
    EngineState::export_all(&cfg).unwrap();
    EngineProfiles::export_all(&cfg).unwrap();
    ProfileCommand::export_all(&cfg).unwrap();
    Evaluation::export_all(&cfg).unwrap();
    HistoryQuery::export_all(&cfg).unwrap();
    HistoryEvents::export_all(&cfg).unwrap();

    let mut files = BTreeMap::new();
    collect(dir, dir, &mut files);
    let mut index = String::from(HEADER);
    for name in files.keys() {
        let module = name.trim_end_matches(".ts");
        index.push_str(&format!("export type * from './{module}.js';\n"));
    }
    files.insert("index.ts".to_owned(), index);
    files
}

fn collect(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(root, &path, out);
        } else if path.extension().is_some_and(|e| e == "ts") {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
            out.insert(rel, text);
        }
    }
}

#[test]
fn generated_types_are_current() {
    let tmp = tempfile::tempdir().unwrap();
    let fresh = render(tmp.path());
    let target = generated_dir();
    if std::env::var("SWAPDECK_UPDATE_TYPES").as_deref() == Ok("1") {
        if target.exists() {
            fs::remove_dir_all(&target).unwrap();
        }
        for (name, text) in &fresh {
            let path = target.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        return;
    }
    let mut current = BTreeMap::new();
    if target.exists() {
        collect(&target, &target, &mut current);
    }
    assert!(
        current == fresh,
        "tipi TypeScript non aggiornati: esegui `pnpm gen:types`"
    );
}
