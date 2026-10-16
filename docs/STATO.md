# SwapDeck — Stato

Piano: [PIANO.md](PIANO.md). Aggiornato dalla regia a ogni chiusura di step.

| Step | Stato | SHA | Note |
|---|---|---|---|
| 0 Fondamenta e contratto cswap | chiuso | bcbc8f3 | Rust gnullvm + llvm-mingw per utente; Tauri compila in locale (installer in CI con MSVC) |
| 1 swapdeck-core: client cswap | chiuso | 2e24a84 | 10 rilievi corretti, copertura 91,6% |
| 2 Motore, lock, storico, headless | verifica (10 rilievi) | | |
| 3 packages/ui | chiuso | 99ac73c | 118 test unitari, 10 E2E, copertura 97,6% |
| 4 App Tauri | revisione | | motore da agganciare dopo lo step 2 |
| 5 Gestione completa in UI | | | |
| 6 Installazione cswap | in corso | | |
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

## Decisioni dell'utente (sera del 15/10)
- CI GitHub Actions su Windows, Linux e Mac; priorità assoluta a Windows.
- **Build Windows locale pronta per la mattina** (installer NSIS per utente se riesce col toolchain locale, altrimenti cartella portatile) in `dist-locale/`, più l'installer MSVC dalla CI.
- Chiave di firma dell'updater: generarla, parte privata in un segreto del repo (gh) e copia nel profilo utente.
- Si possono avviare l'app e installare l'estensione sul PC per le prove, ma **mai switch reali** né altre scritture sul cswap reale (regola nel CLAUDE.md).
- A fine lavoro: release in bozza su GitHub con installer e .vsix.
- **Disco C: del PC quasi pieno** (99%, 4 GB liberi nella notte; liberati 3,4 GB eliminando la target condivisa e ridotto il debuginfo dev). Le build Tauri richiedono vari GB: conviene liberare spazio.
- `swapdeck --headless` nella build di rilascio Windows non ha console (app GUI). Provvisorio per lo step 8: un piccolo binario console separato `swapdeck-cli` per la modalità senza interfaccia, invece di `AttachConsole` (vietato da `unsafe_code`).
