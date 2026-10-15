# SwapDeck — Stato

Piano: [PIANO.md](PIANO.md). Aggiornato dalla regia a ogni chiusura di step.

| Step | Stato | SHA | Note |
|---|---|---|---|
| 0 Fondamenta e contratto cswap | in corso (0b chiuso) | c3e2afd | contratto: docs/CSWAP-CONTRATTO.md |
| 1 swapdeck-core: client cswap | | | |
| 2 Motore, lock, storico, headless | | | |
| 3 packages/ui | | | |
| 4 App Tauri | | | |
| 5 Gestione completa in UI | | | |
| 6 Installazione cswap | | | |
| 7 Estensione VS Code | | | |
| 8 Pacchetti e CI | | | GitHub Actions |
| 9 Verifica finale e documenti | | | |

## Domande aperte per l'utente
(raccolte durante l'esecuzione, con scelta provvisoria)
- Repo privato: l'updater e i download richiedono accesso al repo. Provvisorio: updater spento di default, colleghi come collaboratori.
- cswap 0.27.2 è già su PyPI (installata 0.26.0, minimo supportato 0.26.0). Provvisorio: si sviluppa sulla 0.26.0, l'aggiornamento si decide alla fine.
- Da verificare su un profilo con account usa e getta: `switch --json` riuscito, prompt di `remove`/`purge`, `auto --json`, 429 reale (fixture [S] costruite dai sorgenti).
