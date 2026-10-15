# SwapDeck — Stato

Piano: [PIANO.md](PIANO.md). Aggiornato dalla regia a ogni chiusura di step.

| Step | Stato | SHA | Note |
|---|---|---|---|
| 0 Fondamenta e contratto cswap | chiuso | bcbc8f3 | Rust gnullvm + llvm-mingw per utente; Tauri compila in locale (installer in CI con MSVC) |
| 1 swapdeck-core: client cswap | in corso | | |
| 2 Motore, lock, storico, headless | | | |
| 3 packages/ui | in corso | | |
| 4 App Tauri | | | |
| 5 Gestione completa in UI | | | |
| 6 Installazione cswap | | | |
| 7 Estensione VS Code | | | |
| 8 Pacchetti e CI | | | GitHub Actions |
| 9 Verifica finale e documenti | | | |

## Domande aperte per l'utente
(raccolte durante l'esecuzione, con scelta provvisoria)
- Repo privato: l'updater e i download richiedono accesso al repo. Provvisorio: updater spento di default, colleghi come collaboratori.
- cswap aggiornato alla 0.27.2 sul PC (richiesta dell'utente). Il JSON ha `schemaVersion` 1 con il campo nuovo `loginExpiresAt`; il contratto va riallineato nello step 1. Minimo supportato 0.26.0.
- Da verificare su un profilo con account usa e getta: `switch --json` riuscito, prompt di `remove`/`purge`, `auto --json`, 429 reale (fixture [S] costruite dai sorgenti).
- Node installato 22.10: Vite fermo alla 6 con override (Vite 7+ vuole 22.12). Aggiornare Node e togliere l'override (Node di sistema: serve l'utente).
- Toolchain Rust predefinito cambiato in `stable-x86_64-pc-windows-gnullvm` e cartella bin di llvm-mingw aggiunta al PATH utente (fuori dal repo, reversibile).
