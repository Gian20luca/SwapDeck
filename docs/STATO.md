# SwapDeck — Stato

Piano: [PIANO.md](PIANO.md). Aggiornato dalla regia a ogni chiusura di step.

| Step | Stato | SHA | Note |
|---|---|---|---|
| 0 Fondamenta e contratto cswap | chiuso | bcbc8f3 | Rust gnullvm + llvm-mingw per utente; Tauri compila in locale (installer in CI con MSVC) |
| 1 swapdeck-core: client cswap | chiuso | 2e24a84 | 10 rilievi corretti, copertura 91,6% |
| 2 Motore, lock, storico, headless | chiuso | 83bb25a | 10 rilievi corretti, copertura 86,7%, dry-run reale sospeso per cswap-guard |
| 3 packages/ui | chiuso | 99ac73c | 118 test unitari, 10 E2E, copertura 97,6% |
| 4 App Tauri | chiuso | 1adc30c | 5 rilievi corretti, motore agganciato, prove con finto cswap |
| 5 Gestione completa in UI | chiuso | 1b7b82d | 6 schede, comandi Tauri+CLI+VS Code, copertura 97,5% |
| 6 Installazione cswap | chiuso | a83645a | 9 rilievi corretti, rollback automatico, procedura di primo avvio in app e VS Code |
| 7 Estensione VS Code | chiuso | 5860707 | 7 rilievi corretti; .vsix installato nel VS Code reale, motore spento |
| 8 Pacchetti e CI | chiuso | 2351569 | CI verde 3 OS; prove di rilascio verdi; firma separata dalla build; docs/RILASCIO.md |
| 9 Verifica finale e documenti | chiuso | aabb4ec (tag v0.1.0) | release in bozza con tutti gli asset; dist-locale aggiornata; vsix nel VS Code |

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
- Modalità senza interfaccia ed estensione VS Code: binario console `swapdeck-cli` (decisione provvisoria della notte, vedi PIANO §2).
- Incidenti della notte, senza effetti: (1) TUI di cswap aperta 60 s senza input nello step 0b; (2) avvio automatico SwapDeck registrato e subito rimosso in una prova dello step 8; (3) `cswap auto\` lanciato per errore di quoting nello step 6 e rifiutato da cswap. Nessuno switch reale: l'unico cambio della notte (03:53, 6 → 3) è di cswap-guard.
- Notifiche Windows: sul PC di sviluppo Windows tiene in cache l'AUMID `com.swapdeck.desktop` senza nome (primo toast prima della registrazione). Da provare: disconnessione e nuovo accesso. Su macchine nuove compare «SwapDeck» (verificato con un AUMID nuovo).

## 16/10 pomeriggio
- Aggiunte e integrate in v0.1.0: F4 esteso (mini-widget nascondi, comprimi, aggiorna, ora, Fable, trattini delle soglie, date di reset, posizioni separate), F14 flusso a lista, F15 criteri, F16 mappe, F17 tooltip, F18 notifiche (anche da headless, nome SwapDeck), F19 dettaglio utenza. SwapDeck reso generico (niente cswap-guard).
- Release v0.1.0 in bozza: https://github.com/Gian20luca/SwapDeck/releases/tag/untagged-d28007b80c5acbe01553
- In corso per v0.2.0: F20 Chi consuma (branch wip/f20).
- **GitHub Actions fermo per limite di spesa o pagamento non riuscito** dell'account: CI di 82f4ec1 (solo README) da rilanciare.
- Scheda Regole e VS Code: a motore spento compare un riquadro rosso «Motore sospeso. Il motore è spento.» (da decidere lo stile).
- Da provare su macchine reali: installer NSIS su Windows pulito, AppImage/.deb, .dmg, vsix Linux/macOS, updater dopo repo pubblico.
