# App desktop

Binario `swapdeck` (Tauri 2): area di notifica con icona ad anello, finestra principale con la dashboard
di `@swapdeck/ui`, mini-widget, notifiche. Con `--headless [--once] [--dry-run]` gira il motore senza
interfaccia (su Windows la modalità console passerà a `swapdeck-cli`, step 7).

## Motore nell'app

- A ogni lettura il ciclo esegue un giro del motore (`swapdeck_core::headless::Engine`): con il lock
  scrive lo storico, batte il lock, decide e cambia; senza lock (un'altra istanza o `--headless`) osserva.
- Il motore resta spento finché `engine.enabled` (o `engine.dryRun`) non è attivo in `config.json`.
- Sospeso per un conflitto (per esempio un'attività pianificata di un altro motore): icona nella variante «sospeso»,
  riga nel tooltip e avviso sopra la dashboard. Le regole si rileggono all'avvio.
- `cswap auto` avviato dalla UI si ferma all'uscita (attesa breve) e, su Windows, sta in un Job Object
  con `KILL_ON_JOB_CLOSE`: un'uscita forzata dell'app lo chiude comunque.

## Prove locali senza toccare il cswap reale

- `SWAPDECK_CSWAP` verso una copia del finto (`cargo build -p swapdeck-core --bin fake-cswap`, copiato
  come `cswap.exe` in una cartella di prova; lo stato va in `state.json` accanto).
- `SWAPDECK_HOME` verso una cartella di prova (config, lock e storico): mai il motore acceso sul
  profilo reale.
- Build: `pnpm --filter @swapdeck/desktop build`, poi `cargo build -p swapdeck-desktop --features custom-protocol`.
  In rilascio senza `custom-protocol` la build si ferma (`compile_error!`): `tauri build` la attiva da solo.
- Avvio automatico: nelle build di debug il sistema non viene toccato; in rilascio è forzato solo al
  primo avvio, poi l'app legge lo stato del sistema e allinea la config.

## Build locale su Windows senza Build Tools MSVC

- Toolchain rustup `stable-x86_64-pc-windows-gnullvm` come predefinito (`rustup default ...`).
- [llvm-mingw](https://github.com/mstorsjo/llvm-mingw), archivio `ucrt-x86_64` con digest verificato,
  estratto in `%LOCALAPPDATA%\Programs` e con la cartella `bin` nel PATH utente.
- Non usare il toolchain `x86_64-pc-windows-gnu` insieme a llvm-mingw: il linker cerca `libgcc` e fallisce.

Verificato il 2026-10-15: un'app Tauri 2 minima compila (circa 10 minuti la prima volta).
L'eseguibile dipende da `libunwind.dll` di llvm-mingw e da `WebView2Loader.dll`, quindi serve solo
allo sviluppo; gli installer escono dalla CI con MSVC.

## Pacchetti e aggiornamenti

- Installer dalla CI (`.github/workflows/release.yml`, MSVC su Windows); build locale di prova con
  `pnpm dist:local` in `dist-locale/` (installer NSIS e portatile con `libunwind.dll` e `WebView2Loader.dll`).
- Linux senza CI: `pnpm dist:linux` costruisce in Docker (ubuntu 22.04, come la CI) l'ultimo commit e mette
  `.deb`, AppImage, `swapdeck-cli` e `.vsix` linux-x64 in `dist-locale/linux/` (serve una decina di GB liberi
  per il disco di Docker). macOS: [docs/BUILD-MAC.md](../../docs/BUILD-MAC.md).
- Updater (`tauri-plugin-updater`, `src-tauri/src/updater.rs`): spento di default finché il repo è
  privato; si accende con `"updates": { "enabled": true }` in `config.json`. Endpoint `latest.json`
  sulle GitHub Releases, chiave pubblica in `tauri.conf.json`. La CI compila senza segreti e firma
  dopo, in uno step a parte, con `TAURI_SIGNING_PRIVATE_KEY` e `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
  L'aggiornamento si installa all'uscita e non riapre l'app. Rilascio e chiave: `docs/RILASCIO.md`.
- Prove con `SWAPDECK_HOME`: la config sta in `<home>\config\config.json`; con `"autostart": false`
  l'app non registra l'avvio automatico (la voce `SwapDeck` di Windows è unica per utente).
