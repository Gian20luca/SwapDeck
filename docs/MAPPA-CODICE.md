# Mappa del codice

Struttura iniziale (step 0). Ogni step aggiorna la sezione che tocca.

## Radice

| File | Ruolo |
|---|---|
| `package.json` | script `build`, `test`, `lint`, `format`, `test:rust`, `lint:rust` |
| `pnpm-workspace.yaml` | pacchetti, catalogo delle versioni, override di Vite, script di installazione ammessi |
| `tsconfig.base.json` | opzioni TypeScript strict condivise |
| `.prettierrc.json`, `.prettierignore` | formattazione (esclusi `docs/` e `fixtures/`) |
| `Cargo.toml` | workspace Rust, edizione 2024, lint condivisi (`unsafe_code` vietato) |

## Rust

| Crate | Percorso | Contenuto |
|---|---|---|
| `swapdeck-core` | `crates/swapdeck-core` | `version()`; logica dallo step 1 |
| `swapdeck-desktop` (bin `swapdeck`) | `apps/desktop/src-tauri` | segnaposto; Tauri 2 dallo step 4 |

## TypeScript

Ogni pacchetto ha `src/index.ts`, test `src/*.test.ts` (vitest), `tsconfig.json` (typecheck, test inclusi)
e `tsconfig.build.json` (emissione in `dist/`, test esclusi).

| Pacchetto | Percorso | Contenuto |
|---|---|---|
| `@swapdeck/types` | `packages/types` | `SCHEMA_VERSION`; tipi ts-rs dallo step 1 |
| `@swapdeck/ui` | `packages/ui` | `clampPercent`; React + Vite dallo step 3 |
| `swapdeck-vscode` | `apps/vscode` | `swapdeckExecutable`; estensione dallo step 7 |
