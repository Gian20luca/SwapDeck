# SwapDeck

Monitor e gestione degli account di `cswap` (app desktop Tauri 2, modalità `--headless`, estensione VS Code).

## Regole

- Italiano per documenti, messaggi e UI di default (inglese come seconda lingua); identificatori di codice in inglese.
- Nessun privilegio di amministratore: installazione, dati e attività sempre per utente.
- Token e segreti mai in log, storico, messaggi di errore o fixture; nei log niente email complete.
- **Sul PC di sviluppo mai azioni di scrittura sul `cswap` reale** (switch, add, remove, disable/enable, alias, swap, move, map/unmap, config set, auto, import, purge, upgrade): ci sono sessioni Claude al lavoro e un'attività pianificata che cambia account da sola. Test e prove dell'app o dell'estensione usano il finto cswap (`SWAPDECK_CSWAP`) con il motore spento sul profilo reale; sul `cswap` reale solo comandi in lettura, sempre con sottocomando esplicito e pipe (senza argomenti parte la TUI).
- Il contratto di `cswap` (comandi, JSON, codici di uscita, errori) è in `docs/CSWAP-CONTRATTO.md`.
- La logica sta solo in Rust (`crates/swapdeck-core`); la parte TypeScript la usa, non la duplica.
- Piano e decisioni in `docs/PIANO.md`, avanzamento in `docs/STATO.md`, struttura del codice in `docs/MAPPA-CODICE.md` (da leggere per sezione).

## Struttura

- `crates/swapdeck-core`: logica (client cswap, config, motore, lock, storico).
- `apps/desktop/src-tauri`: binario `swapdeck` (Tauri 2 dallo step 4).
- `apps/vscode`: estensione VS Code.
- `packages/ui`: interfaccia condivisa; `packages/types`: tipi generati dal Rust.

## Comandi

- `pnpm install`, `pnpm build`, `pnpm test`, `pnpm lint` (Prettier e typecheck), `pnpm format`.
- `cargo test --workspace`; `pnpm lint:rust` (rustfmt e clippy con `-D warnings`).
- Node >= 22.10; Vite resta alla 6 finché Node non è >= 22.12 ovunque (`pnpm-workspace.yaml`).
- Windows senza Build Tools MSVC: toolchain rustup `stable-x86_64-pc-windows-gnullvm` con `llvm-mingw` nel PATH utente (vedi `apps/desktop/README.md`). Compila anche Tauri, ma solo per lo sviluppo: gli installer Windows escono dalla CI con MSVC.
