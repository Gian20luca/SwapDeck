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
| `cswap::error` | `CswapError`/`CswapErrorKind` (`uncertain` porta l'elenco riletto), `classify_failure`, `classify_uv_failure`, `account_issue`, `CswapVersion`, pulizia dei messaggi |
| `cswap::client` | `CswapClient` (un metodo per comando di F0, verifica con `list --json`; timeout 30 s per i comandi che scrivono, poi esito `uncertain`; `switch` riuscito ma non verificato = `Ok` con `warning`; percorsi di file solo assoluti), `ActionOutcome`, `AutoProcess` (limiti di `auto` controllati prima dell'avvio, stop dopo l'evento `sleep` entro `AUTO_IDLE_WAIT`, uscita raccolta una volta sola) |
| `cswap::process` | esecuzione con pipe, timeout e chiusura dell'albero dei processi, `CREATE_NO_WINDOW` |
| `cswap::locate` | ricerca di cswap (`SWAPDECK_CSWAP`, `UV_TOOL_BIN_DIR`, `PATH`, `~/.local/bin`) e di uv (anche `~/.cargo/bin` e Homebrew su macOS) |
| `cswap::secret` | `SecretToken` (Debug oscurato, azzerato al rilascio) |
| `config` | `AppConfig` (`config.json`): default, `validate`, `normalize`, `load`, `save` atomico; `language`/`theme` sconosciuti → default con avviso, valore originale conservato in `preserved` |
| `paths` | `AppPaths`: cartelle per utente |
| `setup` (step 6) | `Setup`: `diagnose` (cswap e uv: percorso, versione, `schemaVersion`, numero di account), `install` (script ufficiale di uv scaricato e poi eseguito, ripiego PowerShell o curl/wget; `uv tool install claude-swap`; verifica), `upgrade` (`uv tool upgrade claude-swap`, `busy` con spiegazione, `unsupportedSchema`), `check_update`/`dismiss_update` (PyPI con cache di 6 ore in `cswap-update.json`); avanzamento `SetupProgress` a passi con righe ripulite. `stream` (processo con output riga per riga), `http` (ureq + rustls, certificati di sistema, solo HTTPS, proxy da env, ritentativi), `update` (PyPI e cache) |
| `ts_export` (test) | genera `packages/types/src/generated/` (`pnpm gen:types`) e ne controlla l'aggiornamento |

Test: `tests/fixtures_parse.rs` (tutte le fixture), `tests/cswap_client.rs` (ogni comando e classe d'errore con il finto cswap `tests/support/fake_cswap.rs`, bin `fake-cswap`), `tests/setup.rs` (diagnosi, installazione e aggiornamento con finto cswap e finto uv; `real_install_in_isolated_profile` con `--ignored`: installazione reale in un profilo temporaneo, su Windows da PowerShell).

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
| `dev/` | `FakeBackend` (fixture di `fixtures/cswap/` con istanti riallineati a «ora», cambi simulati; `fakeSetup.ts` simula diagnosi, installazione, controllo versione e aggiornamento con l'opzione `setup`), `history.ts` (storico sintetico di 7 giorni), `main.tsx` (anteprima con demo dei dialoghi di conferma e degli stati di cswap) |
| `components/` | `Dashboard`, `AccountCard`, `UsageBar`, `PaceIndicator`, `QuickSwitch`, `HistoryPanel`, `UsageChart` (uPlot), `MiniWidget`, `ConfirmDialog` (anche a doppia conferma con parola), `PreferenceControls`, `SetupWizard` (primo avvio: installazione di cswap, guida al primo account), `SetupRun` (passi e log), `CswapUpdateNotice` (avviso di nuova versione, chiusura ricordata per versione, aggiornamento con conferma) |
| `i18n/` | dizionari `it.ts` (fonte delle chiavi) ed `en.ts`, `I18nProvider`, `useI18n` |
| `theme/`, `styles/index.css` | `ThemeProvider` (chiaro/scuro/auto via `data-theme`), variabili CSS, test di contrasto AA |
| `lib/` | soglie e formattazione (`format.ts`), helper degli account (`account.ts`), `useNow`, `setup.ts` (reducer dell'avanzamento, testi degli errori), `useSetupRun` |
| `e2e/`, `playwright.config.ts` (fuori da `src/`) | Playwright su Chromium contro l'anteprima (porta 5199): `pnpm --filter @swapdeck/ui e2e`; screenshot in `e2e/screenshots/` (ignorati da git). Copertura: `pnpm --filter @swapdeck/ui test:coverage` (v8, soglia 80% righe e funzioni, esclusi `src/dev/**`) |
