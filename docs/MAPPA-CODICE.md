# Mappa del codice

Struttura iniziale (step 0). Ogni step aggiorna la sezione che tocca.

## Radice

| File | Ruolo |
|---|---|
| `package.json` | script `build`, `test`, `lint`, `format`, `test:rust`, `lint:rust`, `gen:types`, `dist:local` |
| `pnpm-workspace.yaml` | pacchetti, catalogo delle versioni, override di Vite, script di installazione ammessi |
| `tsconfig.base.json` | opzioni TypeScript strict condivise |
| `.prettierrc.json`, `.prettierignore` | formattazione (esclusi `docs/` e `fixtures/`) |
| `Cargo.toml` | workspace Rust, edizione 2024, lint condivisi (`unsafe_code` vietato) |
| `.github/workflows/ci.yml` | push e PR: lint, build, test, clippy, `cargo test` su Windows, Ubuntu 22.04 e macOS; E2E di `packages/ui` su Ubuntu; dispatch con un solo OS (`os`). Action fissate a SHA completo (versione in commento) |
| `.github/workflows/release.yml` | tag `v*`: app Tauri (NSIS per utente, AppImage e .deb, .dmg arm64 e x64) con `swapdeck-cli` come binario esterno, `.vsix` per piattaforma, `latest.json`, release in bozza; dispatch con `dry_run` (solo artefatti) e `platforms`. Compila senza segreti; la chiave dell'updater serve solo allo step «Firma per l'updater» (`tauri signer sign`; il `.app.tar.gz` di macOS si crea a mano). Release vera solo da tag, con versioni di `Cargo.toml` e `apps/vscode/package.json` uguali al tag; asset ricaricati solo su una bozza. Procedura in `docs/RILASCIO.md` |
| `scripts/latest-json.mjs` | `latest.json` dell'updater dai pacchetti firmati (`.sig`); Linux con la chiave `linux-x86_64-appimage` (il .deb non si aggiorna) |
| `scripts/dist-local.mjs` | `pnpm dist:local` (solo Windows, gnullvm): `dist-locale/` con installer NSIS, portatile zippato (DLL di llvm-mingw e WebView2Loader), `.vsix` win32-x64, `LEGGIMI.txt`; `--clean` libera la target di rilascio |

## Rust

| Crate | Percorso | Contenuto |
|---|---|---|
| `swapdeck-core` | `crates/swapdeck-core` | client di cswap, config, cartelle dati (step 1) |
| `swapdeck-desktop` (bin `swapdeck`) | `apps/desktop/src-tauri` | app Tauri 2 (vedi `apps/desktop` sotto) |
| `swapdeck-cli` (bin `swapdeck-cli`) | `apps/cli` | comandi JSON sopra il core per l'estensione VS Code e headless senza GUI (step 7) |

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
| `setup` (step 6) | `Setup`: `diagnose` (cswap e uv: percorso, versione, `schemaVersion`, numero di account), `install` (script ufficiale di uv scaricato e poi eseguito, ripiego PowerShell o curl/wget; `uv tool install claude-swap`; verifica), `upgrade` (rifiuta con `busy` se il lock del motore è vivo e di un altro processo; `uv tool upgrade claude-swap`; `busy` con spiegazione anche per accesso negato; con `unsupportedSchema` torna da solo alla versione precedente, passo `rollbackCswap`), `check_update`/`dismiss_update` (PyPI con cache di 6 ore in `cswap-update.json`, riletta prima di scrivere; pre-release più vecchie della stabile); con `SWAPDECK_CSWAP` impostata `install` e `upgrade` sono rifiutati; script di uv in una cartella privata della cartella dati; avanzamento `SetupProgress` a passi con righe ripulite. `stream` (processo con output riga per riga), `http` (ureq + rustls, certificati di sistema, solo HTTPS, proxy da env, ritentativi), `update` (PyPI e cache) |
| `ts_export` (test) | genera `packages/types/src/generated/` (`pnpm gen:types`) e ne controlla l'aggiornamento |

Test: `tests/fixtures_parse.rs` (tutte le fixture), `tests/cswap_client.rs` (ogni comando e classe d'errore con il finto cswap `tests/support/fake_cswap.rs`, bin `fake-cswap`), `tests/setup.rs` (diagnosi, installazione e aggiornamento con finto cswap e finto uv; `real_install_in_isolated_profile` con `--ignored`: installazione reale in un profilo temporaneo, su Windows da PowerShell).

### Motore, lock, storico (step 2, `crates/swapdeck-core/src`)

| Modulo | Contenuto |
|---|---|
| `engine.rs` | `EngineRules` (`AppConfig.engine` = regole della mappa attiva; l'intervallo è solo `pollIntervalSeconds`, `engine.intervalSecs` migrato in lettura). Lista con priorità `order` (la prima è la principale; numeri assenti in cswap saltati); `primary` e `reserves` restano come forma precedente, prevalgono se `primary` c'è e si riallineano con `sync_order` al caricamento e al salvataggio. `EngineMemory.leftAccounts` (utenze lasciate: reset e finestre per l'isteresi; i campi `primary*` vecchi si leggono e si spostano al primo cambio). Funzioni pure `evaluate` (decisione, motivo leggibile, candidato attuale `Candidate`) e `decide`. Criterio «ordine» = flusso a lista (ritorno alla prima libera, causa `earlierAvailable`); altri criteri: ritorno solo alla principale. `engine/criteria.rs` (criteri e `ScoreWeights`), `engine/explain.rs` (motivi in italiano, alias o numero, mai email), `engine/profiles.rs` (mappe: `EngineProfiles`, `ProfileCommand`, preset; `AppConfig.engineProfiles`, `apply_profile_command`). Scenari in `engine/tests.rs` e `engine/flow_tests.rs` |
| `lock.rs` | `engine.lock` nella cartella dati: `try_acquire` (`create_new`, subentro con `engine.lock.takeover`), `LockGuard::heartbeat`, rilascio al `Drop`. Stesso host: scaduto solo con PID morto o riusato (ora di avvio diversa); host diversi o avvio ignoto: 3 intervalli senza battito |
| `history.rs` | `History` (`history/AAAA-MM-GG.jsonl`, UTC): `read_range`, `prune` (30 giorni, al massimo `MAX_PRUNE_PER_CALL` file per giro contro i salti dell'orologio); `HistoryWriter` (solo con `LockGuard`): `append` (rotazione fallita annotata in `take_prune_error`, non propagata), `record_samples` |
| `conflicts.rs` | `SystemConflicts`: processi via sysinfo (riga illeggibile di `cswap` = `process:unreadable`, avviso), attività su nome e comando via `Get-ScheduledTask` (Windows), `crontab -l` e `systemctl --user list-timers` (Linux), `launchctl list` (macOS); esito dei comandi verificato, lettura fallita = `tasks:unverified` che sospende (ignorabile); cache 10 min, risultato verificato tenuto al massimo 30 min. Id stabili per `ignoredConflicts` (`cron:<hash>`, mai il testo della riga) |
| `headless.rs` | trait `CswapPort` (implementato da `CswapClient`) e `Clock`, `run(RunOptions, Runtime)` (conflitti letti prima di `list`; con `RunOptions.config_file` regole e intervallo riletti prima di ogni giro, config illeggibile = regole di prima), `Engine` a giri singoli per l'app (`set_rules`, `set_interval` senza riavvio), `cli_main` per `swapdeck --headless [--once] [--dry-run]` (cartelle da `AppPaths::from_env_or_system`, `SWAPDECK_HOME` per le prove; uscite 0/1/2/3). Test con finto cswap in `headless/tests.rs` |
| `timeutil.rs` | secondi Unix, parsing RFC 3339 di cswap, giorni UTC |

### `apps/cli` (step 7)

Successo: JSON su stdout, uscita 0. Errore: `{"error": CswapError}` su stdout, uscita 1 (fallito) o 2 (uso o JSON non validi). `headless` usa i codici del core (0/1/2/3, 3 = lock occupato).

| File | Contenuto |
|---|---|
| `src/lib.rs` | `main_with`/`dispatch`: `version`, `state` (lista + errore + stato del motore, sempre uscita 0), `status`, `cswap-config [KEY]`, `confirm - [--active N|none]`, `action -` (azioni **solo da stdin**: gli argomenti sono visibili agli altri processi), `history`, `history events [--since S] [--account N]`, `mappings`, `config get`, `config set <json|->` (merge patch RFC 7386), `preferences get|set <json|->` (`PreferencesPatch`, campi sconosciuti rifiutati), `engine status`, `engine state` (regole, vista dall'ultima decisione se il motore gira, conflitti solo a motore acceso), `engine rules set <json|->` (validate, il motore le rilegge al giro dopo), `setup diagnose|install|upgrade|check-update [--force]|dismiss-update <V>` (install e upgrade: un `SetupProgress` JSON per riga, poi l'esito), `headless [--once] [--dry-run] [--stop-on-stdin-eof]`. Errori di parsing con sola riga e colonna; nel messaggio d'uso solo il primo argomento |
| `src/output.rs` | `CliState`, `EngineStatus` (config + lock), `CliVersion` (`PROTOCOL`), `ConfigView`, `CliErrorEnvelope` |
| `src/ts_export.rs` (test) | tipi in `apps/vscode/src/generated/` (`pnpm gen:types`), con import dei tipi del core da `@swapdeck/types` |
| `tests/cli.rs` | integrazione col finto cswap (bin di test `swapdeck-fake-cswap`) e `SWAPDECK_HOME` temporanea |

## TypeScript

Ogni pacchetto ha `src/index.ts`, test `src/*.test.ts` (vitest), `tsconfig.json` (typecheck, test inclusi)
e `tsconfig.build.json` (emissione in `dist/`, test esclusi).

| Pacchetto | Percorso | Contenuto |
|---|---|---|
| `@swapdeck/types` | `packages/types` | `SCHEMA_VERSION`; tipi ts-rs dallo step 1 |
| `@swapdeck/desktop` | `apps/desktop` | pagina dell'app Tauri con `TauriBackend` (vedi sotto) |
| `@swapdeck/ui` | `packages/ui` | React 19 + Vite 6: dashboard, mini-widget, grafici uPlot, i18n, tema (vedi sotto) |
| `swapdeck-vscode` | `apps/vscode` | estensione VS Code (step 7, vedi sotto) |

### `packages/ui` (step 3)

Esporta i sorgenti (`src/index.ts`), non una build: i consumatori (Tauri, webview VS Code) li compilano con Vite. Stili in `@swapdeck/ui/styles.css`; `@swapdeck/ui/fake` espone il FakeBackend (fuori dal bundle di produzione). Anteprima: `pnpm --filter @swapdeck/ui dev`.

| Percorso (`src/`) | Ruolo |
|---|---|
| `backend/types.ts` | interfaccia `Backend` (async, `onStateChanged`, `hostConfirms` facoltativo: l'host chiede da sé le conferme), `HistoryPoint`, `BackendError`; `actions.ts` costruisce le `CswapAction`; `context.tsx`, `useAccounts.ts` ; metodi facoltativi di gestione (step 5): `listMappings`, `getEngineState`/`setEngineRules`, `getPreferences`/`setPreferences`, `getAppSettings`/`setAutostart`, `getHistoryEvents`, `pickOpenFile`/`pickSaveFile`/`pickFolder` (un host senza metodo ottiene «non disponibile» o il solo campo di testo); `miniWindow` facoltativo (`MiniWindowControls`: `hide`, `setCompact`, `isCompact`, `fitHeight`; senza, `MiniWidget` non mostra «Nascondi» e «Comprimi», come nel webview di VS Code) e `setWindowTheme` facoltativo (tema della cornice nativa, chiamato da `WindowTheme`); `SetupBackend` facoltativo (F5: diagnosi, installazione, controllo e aggiornamento di cswap; `hasSetup`, senza il quale `SetupWizard` e `CswapUpdateNotice` non mostrano nulla); `useActionRunner.tsx` esegue le azioni con la conferma dovuta (`confirmationFor`, saltata se `hostConfirms`), `useLoad` in `lib/` |
| `dev/` | `FakeBackend` (fixture di `fixtures/cswap/` con istanti riallineati a «ora», cambi simulati; `fakeSetup.ts` simula diagnosi, installazione, controllo versione e aggiornamento con l'opzione `setup`), `history.ts` (storico sintetico di 7 giorni), `main.tsx` (anteprima con demo dei dialoghi di conferma e degli stati di cswap) |
| `components/` | `Dashboard`, `AccountCard`, `UsageBar`, `PaceIndicator`, `QuickSwitch`, `HistoryPanel`, `UsageChart` (uPlot), `MiniWidget` (vista estesa con barre 5h, 7d e per modello, ora dell'ultima lettura, pulsanti Aggiorna/Comprimi/Nascondi in alto a destra; vista compressa a striscia 280×28 con alias e % 5h; l'altezza della finestra segue il contenuto con `fitHeight`), `WindowTheme`, `navigation.tsx` (`NavProvider`/`useNavigate`: la scheda Account apre la scheda Storico con l'account scelto, aperto e col focus), `ConfirmDialog` (anche a doppia conferma con parola), `PreferenceControls`; `SwapDeckApp` (schede Account, Cartelle, Regole, Storico, Strumenti, Impostazioni, a misura di 420×640; con `SetupBackend` passa prima da `SetupWizard` se cswap non è pronto e mostra `CswapUpdateNotice` sopra le schede) e `manage/` (`AddAccount`, `AccountActions`, `FoldersPage`, `RulesPage`, `HistoryPage`, `ToolsPage`, `SettingsPage`, `common.tsx`); `Dashboard manage` aggiunge le azioni sulle schede; `SetupWizard` (primo avvio: installazione di cswap, guida al primo account), `SetupRun` (passi e log, più i passi avviati fuori elenco come `rollbackCswap`), `CswapUpdateNotice` (avviso di nuova versione; chiuderlo senza aver provato ricorda la versione, dopo un tentativo no; aggiornamento con conferma) |
| `i18n/` | dizionari `it.ts` (fonte delle chiavi) ed `en.ts`, `I18nProvider`, `useI18n`; i testi della gestione sono in `it.manage.ts` ed `en.manage.ts` |
| `theme/`, `styles/index.css` | `ThemeProvider` (chiaro/scuro/auto via `data-theme`), variabili CSS, test di contrasto AA |
| `lib/` | soglie e formattazione (`format.ts`), helper degli account (`account.ts`), `useNow`, `engineRules.ts` (preset cswap-guard, bozza e validazione come `EngineRules::validate`), `cswapConfig.ts` (chiavi e valori di `cswap config`), `historyEvents.ts`, `useLoad.ts`, `engineMarks.ts` e `useEngineMarks.ts` (soglie e modo del motore per i trattini sulle barre di 5h e 7d dell'account attivo: rosso se il motore cambia davvero, grigio se spento, in sola osservazione o sospeso; tooltip con `aria-describedby`; il ritmo atteso ha un trattino tratteggiato dentro la barra), `formatClock` e `formatWhen` in `format.ts` (ora `HH:MM:SS`, e `oggi/domani/ven 16/10` più ora con `Intl.DateTimeFormat`), `setup.ts` (reducer dell'avanzamento, testi degli errori), `useSetupRun` |
| `e2e/`, `playwright.config.ts` (fuori da `src/`) | Playwright su Chromium contro l'anteprima (porta 5199): `pnpm --filter @swapdeck/ui e2e`; screenshot in `e2e/screenshots/` (ignorati da git). Copertura: `pnpm --filter @swapdeck/ui test:coverage` (v8, soglia 80% righe e funzioni, esclusi `src/dev/**`) |

### `apps/desktop` (step 4)

Binario `swapdeck` (Tauri 2). Pagina Vite in `src/` (porta 5175), crate in `src-tauri/`.
Build di sviluppo: `pnpm --filter @swapdeck/desktop build`, poi `cargo build -p swapdeck-desktop --features custom-protocol` (pagina incorporata) oppure `pnpm --filter @swapdeck/desktop tauri dev`.

| Percorso | Ruolo |
|---|---|
| `src/TauriBackend.ts` | `Backend` con `invoke` (nella finestra `mini` anche `miniWindow` sui comandi `mini_*`; `setWindowTheme` sulla finestra Tauri); ed evento `state-changed` (`Snapshot`: lista, errore, `engine`, `engineSuspended`, `autoRunning`); comandi dell'app (`getSnapshot`, `getAppSettings`, `setAutostart`, `showMain`); step 5: metodi di gestione (`list_mappings`, `get_engine_state`, `set_engine_rules`, `get_preferences`, `set_preferences`, `get_history_events`) e selettori con `tauri-plugin-dialog` (permessi `dialog:allow-open/save` solo nella finestra `main`); step 6: implementa `SetupBackend` (avanzamento dall'evento `setup-progress`, ascoltato solo durante il comando) |
| `src/EngineStatus.tsx` | righe di stato sopra la dashboard: motore sospeso (motivo e conflitti), sola osservazione, `cswap auto` in esecuzione |
| `src/main.tsx` | `SwapDeckApp` nella finestra `main` (lingua e tema iniziali da `get_preferences`, se c'è), `MiniWidget` trascinabile nella finestra `mini` (scelta dall'etichetta della finestra) |
| `src-tauri/src/api.rs` | interfaccia sottile `CswapApi`; `ClientApi` sopra `CswapClient` (ricerca pigra, una chiamata alla volta, `cswap auto` in un Job Object con `KILL_ON_JOB_CLOSE`, `auto_poll`, `shutdown` all'uscita, `exclusive` per l'aggiornamento di cswap: ferma `cswap auto` e tiene il turno delle chiamate, così ciclo di lettura e motore aspettano; `relocate`); finto per i test |
| `src-tauri/src/auto_watch.rs` | controllo di `cswap auto` ogni 2 s: eventi, cambi, fine o errore (stato nella UI e notifica) |
| `src-tauri/src/engine_loop.rs` | motore nel ciclo di lettura (`headless::Engine`): con il lock storico, battito, decisione e cambio; senza solo osservazione; `EngineView` per la UI; conflitti letti solo a motore acceso; regole e intervallo riletti dalle impostazioni a ogni giro (cambio dalla UI senza riavvio) |
| `src-tauri/src/poller.rs` | ciclo di lettura: un thread, lettura (con il motore) a ogni intervallo e a richiesta, richieste unite; `run_paced` anticipa la lettura dopo |
| `src-tauri/src/state.rs` | `Shared` (api, `Snapshot`, impostazioni, notifiche), evento `state-changed` |
| `src-tauri/src/commands.rs` | comandi `invoke` (elenco in `build.rs`, permessi in `capabilities/`); `get_history` dai campioni dello storico (un punto per account per intervallo, atteso sulle 5 ore dal reset); gestione: `list_mappings`, `get_engine_state`, `set_engine_rules` (valida, salva e chiede subito un giro), `get_preferences`/`set_preferences` (patch validata, nuovo intervallo dal giro dopo), `get_history_events`; mini-widget: `mini_hide`, `mini_set_compact`, `mini_is_compact`, `mini_fit_height` (permessi nella capability `mini`); argomenti JSON non validi = `invalidUsage` |
| `src-tauri/src/setup_cmd.rs` | comandi di `SetupBackend`: `diagnose_setup`, `install_cswap`, `upgrade_cswap` (dentro `CswapApi::exclusive`), `check_cswap_update`, `dismiss_cswap_update`; una sola operazione alla volta, evento `setup-progress`, dopo l'operazione cswap si cerca di nuovo e si rilegge |
| `src-tauri/src/tray.rs`, `ring.rs` | icona ad anello disegnata a runtime, tooltip, menu con il cambio rapido |
| `src-tauri/src/notify.rs`, `text.rs` | soglie e cambi di account (una volta per finestra di reset), testi it/en |
| `src-tauri/src/windows.rs` | principale nascosta alla chiusura (tema nativo dalle preferenze prima di mostrarla), mini-widget con posizione e stato compresso ricordati (`miniWidget.{x,y,compact}` nella config): `set_mini_compact` (280×28 o ultima altezza), `fit_mini_height` (150–400 px logici, `clamp_into` tiene la finestra dentro lo schermo), `hide_mini` (chiude la finestra: l'app resta nell'area di notifica e la voce «Mini-widget» del menu la riapre) |
| `src-tauri/src/settings.rs`, `terminal.rs`, `app.rs` | impostazioni da `config.json` (`SWAPDECK_HOME` come `--headless`; regole `engine` in sola lettura; `autostart` e `miniWidget` in `extra`, `updates.enabled` in sola lettura), terminale di `cswap run`, avvio, plugin e uscita ordinata |
| `src-tauri/src/updater.rs` | aggiornamenti con `tauri-plugin-updater` solo con `updates.enabled` (spento di default): controllo dopo 30 s e ogni 24 h, attesa crescente sugli errori, download con verifica della firma, installazione all'uscita senza riaprire l'app; senza voce per l'installazione (.deb) si ferma |
| `src-tauri/tauri.conf.json`, `tauri.release.conf.json` | bundle (NSIS `currentUser`, bootstrapper WebView2), chiave pubblica ed endpoint dell'updater; la seconda, solo in CI, attiva il bundle e `swapdeck-cli` esterno (artefatti dell'updater firmati a parte) |
| `src-tauri/src/autostart.rs` | avvio al login con `auto-launch` e percorso tra virgolette; forzato solo al primo avvio (chiave `autostart` assente), poi vale il sistema |
| `src-tauri/src/main.rs` | `--headless` (motore senza interfaccia, `swapdeck_core::headless::cli_main`) o app; `compile_error!` in rilascio senza `custom-protocol` |

### `apps/vscode` (step 7)

Host in Node (bundle esbuild `out/extension.js`), webview React con `@swapdeck/ui` (`out/webview`). Nessuna logica di cswap in TypeScript: tutto passa da `swapdeck-cli` (incluso nel .vsix in `bin/`). `extensionKind: ["ui"]`: gira sempre sulla macchina locale.

| File (`src/`) | Ruolo |
|---|---|
| `extension.ts` | `activate`: barra di stato, polling (`Refresher`), cambio rapido, comandi, conferme con dialoghi modali (`askConfirmation`; parola digitata per `purge`), notifiche, motore (`syncEngine`; con lock occupato attende e ritenta quando lo `state` lo dà libero). Restituisce `SwapDeckApi` per i test |
| `cli.ts` | `createCliRunner` (spawn senza shell, stdin, timeout, limite dell'output; `onEvent` per i comandi con un evento JSON per riga, esito dall'ultima riga), `resolveCliPath` (solo percorsi assoluti), `CliError` |
| `bridge.ts` | richieste del webview → CLI: metodi ammessi, `ACTION_KINDS` chiuso (controllo di completezza a compilazione), `run` solo con `SAFE_CLAUDE_ARGS`, conferma imposta dall'host prima di `action`, `describeAction`; gestione: `listMappings`, `getEngineState`, `setEngineRules` e `setPreferences` (oggetti JSON via stdin), `getPreferences`, `getHistoryEvents`; setup: `diagnoseSetup`, `checkCswapUpdate`, `dismissCswapUpdate` (versione validata), `installCswap`/`upgradeCswap` con conferma modale dell'host (`confirmSetup`), motore dell'estensione fermato e riavviato attorno all'aggiornamento (`pauseEngine`), eventi inoltrati al webview come messaggi `progress` |
| `refresher.ts` | lettura condivisa con contatore di generazione: una lettura partita prima di un'azione non sovrascrive lo stato |
| `engine.ts` | `EngineProcess` (`headless --stop-on-stdin-eof`, stop con stdin chiuso poi kill), `LineSplitter` (righe per stream, UTF-8), `shouldRetryEngine`, `LOCK_BUSY_EXIT` |
| `panel.ts`, `protocol.ts` | webview con CSP a nonce e `localResourceRoots` = `out/webview`; messaggi host ↔ webview |
| `webview/` | `VsCodeBackend` (`Backend` via `postMessage`, `hostConfirms = true`, metodi di gestione, `SetupBackend` con l'avanzamento per id di richiesta), `main.tsx` (`SwapDeckApp` completa a schede) |
| `statusBar.ts`, `notifier.ts` | logica pura di barra (testo, tooltip, livello) e notifiche (soglie e cambio dell'account attivo) |
| `generated/` | tipi della CLI (ts-rs) |

Altro: `l10n/` e `package.nls*.json` (inglese sorgente, bundle italiano; `l10n.test.ts` ne controlla la completezza); `test:coverage` (vitest v8, soglia 70% righe, `vitest.config.mts`); `test/suite.ts` + `scripts/run-ext-tests.mjs` (`test:ext`: VS Code isolato in `.vscode-test/`, finto cswap, `SWAPDECK_HOME` temporanea); `scripts/package.mjs` (.vsix per target).
