# Mappa del codice

Struttura iniziale (step 0). Ogni step aggiorna la sezione che tocca.

## Radice

| File | Ruolo |
|---|---|
| `package.json` | script `build`, `test`, `lint`, `format`, `test:rust`, `lint:rust`, `gen:types` |
| `pnpm-workspace.yaml` | pacchetti, catalogo delle versioni, override di Vite, script di installazione ammessi |
| `tsconfig.base.json` | opzioni TypeScript strict condivise |
| `.prettierrc.json`, `.prettierignore` | formattazione (esclusi `docs/` e `fixtures/`) |
| `Cargo.toml` | workspace Rust, edizione 2024, lint condivisi (`unsafe_code` vietato) |

## Rust

| Crate | Percorso | Contenuto |
|---|---|---|
| `swapdeck-core` | `crates/swapdeck-core` | client di cswap, config, cartelle dati (step 1) |
| `swapdeck-desktop` (bin `swapdeck`) | `apps/desktop/src-tauri` | segnaposto; Tauri 2 dallo step 4 |

### `swapdeck-core` (step 1)

| Modulo | Contenuto |
|---|---|
| `cswap::model` | tipi serde del JSON di cswap: `ListResponse`, `Account`, `Usage`, `StatusResponse`, `SwitchResponse`, `ErrorEnvelope`, `ConfigListResponse`, `AutoEvent` |
| `cswap::action` | `CswapAction` (azioni dell'interfaccia, solo `Deserialize`), `Confirmation`, `AutoOptions`, `RunOptions` |
| `cswap::error` | `CswapError`/`CswapErrorKind`, `classify_failure`, `account_issue`, `CswapVersion`, pulizia dei messaggi |
| `cswap::client` | `CswapClient` (un metodo per comando di F0, verifica con `list --json`), `ActionOutcome`, `AutoProcess` |
| `cswap::process` | esecuzione con pipe, timeout e chiusura dell'albero dei processi, `CREATE_NO_WINDOW` |
| `cswap::locate` | ricerca di cswap (`SWAPDECK_CSWAP`, `PATH`, `~/.local/bin`) e di uv |
| `cswap::secret` | `SecretToken` (Debug oscurato, azzerato al rilascio) |
| `config` | `AppConfig` (`config.json`): default, `validate`, `normalize`, `load`, `save` atomico |
| `paths` | `AppPaths`: cartelle per utente |
| `ts_export` (test) | genera `packages/types/src/generated/` (`pnpm gen:types`) e ne controlla l'aggiornamento |

Test: `tests/fixtures_parse.rs` (tutte le fixture), `tests/cswap_client.rs` (ogni comando e classe d'errore con il finto cswap `tests/support/fake_cswap.rs`, bin `fake-cswap`).

## TypeScript

Ogni pacchetto ha `src/index.ts`, test `src/*.test.ts` (vitest), `tsconfig.json` (typecheck, test inclusi)
e `tsconfig.build.json` (emissione in `dist/`, test esclusi).

| Pacchetto | Percorso | Contenuto |
|---|---|---|
| `@swapdeck/types` | `packages/types` | `SCHEMA_VERSION`; tipi ts-rs dallo step 1 |
| `@swapdeck/ui` | `packages/ui` | React 19 + Vite 6: dashboard, mini-widget, grafici uPlot, i18n, tema (vedi sotto) |
| `swapdeck-vscode` | `apps/vscode` | `swapdeckExecutable`; estensione dallo step 7 |

### `packages/ui` (step 3)

Esporta i sorgenti (`src/index.ts`), non una build: i consumatori (Tauri, webview VS Code) li compilano con Vite. Stili in `@swapdeck/ui/styles.css`; `@swapdeck/ui/fake` espone il FakeBackend (fuori dal bundle di produzione). Anteprima: `pnpm --filter @swapdeck/ui dev`.

| Percorso (`src/`) | Ruolo |
|---|---|
| `backend/types.ts` | interfaccia `Backend` (async, `onStateChanged`), `HistoryPoint`, `BackendError`; `actions.ts` costruisce le `CswapAction`; `context.tsx`, `useAccounts.ts` |
| `dev/` | `FakeBackend` (fixture di `fixtures/cswap/` con istanti riallineati a «ora», cambi simulati), `history.ts` (storico sintetico di 7 giorni), `main.tsx` (anteprima con demo dei dialoghi di conferma) |
| `components/` | `Dashboard`, `AccountCard`, `UsageBar`, `PaceIndicator`, `QuickSwitch`, `HistoryPanel`, `UsageChart` (uPlot), `MiniWidget`, `ConfirmDialog` (anche a doppia conferma con parola), `PreferenceControls` |
| `i18n/` | dizionari `it.ts` (fonte delle chiavi) ed `en.ts`, `I18nProvider`, `useI18n` |
| `theme/`, `styles/index.css` | `ThemeProvider` (chiaro/scuro/auto via `data-theme`), variabili CSS, test di contrasto AA |
| `lib/` | soglie e formattazione (`format.ts`), helper degli account (`account.ts`), `useNow` |
| `e2e/`, `playwright.config.ts` (fuori da `src/`) | Playwright su Chromium contro l'anteprima (porta 5199): `pnpm --filter @swapdeck/ui e2e`; screenshot in `e2e/screenshots/` (ignorati da git). Copertura: `pnpm --filter @swapdeck/ui test:coverage` (v8, soglia 80% righe e funzioni, esclusi `src/dev/**`) |
