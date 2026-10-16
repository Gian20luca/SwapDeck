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
| `swapdeck-desktop` (bin `swapdeck`) | `apps/desktop/src-tauri` | app Tauri 2 (vedi `apps/desktop` sotto) |

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
| `ts_export` (test) | genera `packages/types/src/generated/` (`pnpm gen:types`) e ne controlla l'aggiornamento |

Test: `tests/fixtures_parse.rs` (tutte le fixture), `tests/cswap_client.rs` (ogni comando e classe d'errore con il finto cswap `tests/support/fake_cswap.rs`, bin `fake-cswap`).

### Motore, lock, storico (step 2, `crates/swapdeck-core/src`)

| Modulo | Contenuto |
|---|---|
| `engine.rs` | `EngineRules` (`AppConfig.engine`; l'intervallo è solo `pollIntervalSeconds`, `engine.intervalSecs` migrato in lettura), `EngineMemory` (con `primaryLeftWindows`: l'isteresi del ritorno vale solo per la finestra che ha fatto lasciare il principale), `Decision`, funzione pura `decide(Observation, regole, memoria, ora)`: conflitti prima della validazione delle regole. Scenari in `engine/tests.rs` |
| `lock.rs` | `engine.lock` nella cartella dati: `try_acquire` (`create_new`, subentro con `engine.lock.takeover`), `LockGuard::heartbeat`, rilascio al `Drop`. Stesso host: scaduto solo con PID morto o riusato (ora di avvio diversa); host diversi o avvio ignoto: 3 intervalli senza battito |
| `history.rs` | `History` (`history/AAAA-MM-GG.jsonl`, UTC): `read_range`, `prune` (30 giorni, al massimo `MAX_PRUNE_PER_CALL` file per giro contro i salti dell'orologio); `HistoryWriter` (solo con `LockGuard`): `append` (rotazione fallita annotata in `take_prune_error`, non propagata), `record_samples` |
| `conflicts.rs` | `SystemConflicts`: processi via sysinfo (riga illeggibile di `cswap` = `process:unreadable`, avviso), attività su nome e comando via `Get-ScheduledTask` (Windows), `crontab -l` e `systemctl --user list-timers` (Linux), `launchctl list` (macOS); esito dei comandi verificato, lettura fallita = `tasks:unverified` che sospende (ignorabile); cache 10 min, risultato verificato tenuto al massimo 30 min. Id stabili per `ignoredConflicts` (`cron:<hash>`, mai il testo della riga) |
| `headless.rs` | trait `CswapPort` (implementato da `CswapClient`) e `Clock`, `run(RunOptions, Runtime)` (conflitti letti prima di `list`), `cli_main` per `swapdeck --headless [--once] [--dry-run]` (cartelle da `AppPaths::from_env_or_system`, `SWAPDECK_HOME` per le prove; uscite 0/1/2/3). Test con finto cswap in `headless/tests.rs` |
| `timeutil.rs` | secondi Unix, parsing RFC 3339 di cswap, giorni UTC |

## TypeScript

Ogni pacchetto ha `src/index.ts`, test `src/*.test.ts` (vitest), `tsconfig.json` (typecheck, test inclusi)
e `tsconfig.build.json` (emissione in `dist/`, test esclusi).

| Pacchetto | Percorso | Contenuto |
|---|---|---|
| `@swapdeck/types` | `packages/types` | `SCHEMA_VERSION`; tipi ts-rs dallo step 1 |
| `@swapdeck/desktop` | `apps/desktop` | pagina dell'app Tauri con `TauriBackend` (vedi sotto) |
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

### `apps/desktop` (step 4)

Binario `swapdeck` (Tauri 2). Pagina Vite in `src/` (porta 5175), crate in `src-tauri/`.
Build di sviluppo: `pnpm --filter @swapdeck/desktop build`, poi `cargo build -p swapdeck-desktop --features custom-protocol` (pagina incorporata) oppure `pnpm --filter @swapdeck/desktop tauri dev`.

| Percorso | Ruolo |
|---|---|
| `src/TauriBackend.ts` | `Backend` con `invoke` ed evento `state-changed` (`Snapshot`: lista, errore, `engine`, `engineSuspended`, `autoRunning`); comandi dell'app (`getSnapshot`, `getAppSettings`, `setAutostart`, `showMain`) |
| `src/EngineStatus.tsx` | righe di stato sopra la dashboard: motore sospeso (motivo e conflitti), sola osservazione, `cswap auto` in esecuzione |
| `src/main.tsx` | dashboard nella finestra `main`, `MiniWidget` trascinabile nella finestra `mini` (scelta dall'etichetta della finestra); `AutostartToggle.tsx` |
| `src-tauri/src/api.rs` | interfaccia sottile `CswapApi`; `ClientApi` sopra `CswapClient` (ricerca pigra, una chiamata alla volta, `cswap auto` in un Job Object con `KILL_ON_JOB_CLOSE`, `auto_poll`, `shutdown` all'uscita); finto per i test |
| `src-tauri/src/auto_watch.rs` | controllo di `cswap auto` ogni 2 s: eventi, cambi, fine o errore (stato nella UI e notifica) |
| `src-tauri/src/engine_loop.rs` | motore nel ciclo di lettura (`headless::Engine`): con il lock storico, battito, decisione e cambio; senza solo osservazione; `EngineView` per la UI; conflitti letti solo a motore acceso |
| `src-tauri/src/poller.rs` | ciclo di lettura: un thread, lettura (con il motore) a ogni intervallo e a richiesta, richieste unite; `run_paced` anticipa la lettura dopo |
| `src-tauri/src/state.rs` | `Shared` (api, `Snapshot`, impostazioni, notifiche), evento `state-changed` |
| `src-tauri/src/commands.rs` | comandi `invoke` (elenco in `build.rs`, permessi in `capabilities/`); `get_history` dai campioni dello storico (un punto per account per intervallo, atteso sulle 5 ore dal reset) |
| `src-tauri/src/tray.rs`, `ring.rs` | icona ad anello disegnata a runtime, tooltip, menu con il cambio rapido |
| `src-tauri/src/notify.rs`, `text.rs` | soglie e cambi di account (una volta per finestra di reset), testi it/en |
| `src-tauri/src/windows.rs` | principale nascosta alla chiusura, mini-widget con posizione ricordata |
| `src-tauri/src/settings.rs`, `terminal.rs`, `app.rs` | impostazioni da `config.json` (`SWAPDECK_HOME` come `--headless`; regole `engine` in sola lettura; `autostart` e `miniWidget` in `extra`), terminale di `cswap run`, avvio, plugin e uscita ordinata |
| `src-tauri/src/autostart.rs` | avvio al login con `auto-launch` e percorso tra virgolette; forzato solo al primo avvio (chiave `autostart` assente), poi vale il sistema |
| `src-tauri/src/main.rs` | `--headless` (motore senza interfaccia, `swapdeck_core::headless::cli_main`) o app; `compile_error!` in rilascio senza `custom-protocol` |
