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
| v0.2.0 | chiuso | 5ecd7fd (tag v0.2.0) | suite verde; CI e release bloccate dal billing di GitHub Actions; dist-locale 0.2.0; app e vsix installati col motore spento |

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
- «Accendi» del motore (17/10, deciso): conferma solo alla prima accensione, con Accendi / Solo osservazione / Annulla; poi parte subito. Da fare dopo le mappe, insieme alle «i» della pagina Regole.

## 17/10
- Integrate in v0.2.0: Chi consuma (F20), mappe con preset in sola lettura e «Clona come mappa personale», vista completa di VS Code nella barra laterale con sincronia tra finestre, icone «i», conferma alla prima accensione del motore, log e diagnostica, lock della config, aggiornamento automatico delle schede, autostart Linux/macOS corretto. Novità in `docs/RILASCIO.md`.
- Suite finale verde: `cargo test --workspace`, `lint:rust`, `gen:types` senza differenze, `pnpm lint`, `pnpm test` (types 3, ui 377, desktop 21, vscode 78), `check:tauri`, Playwright 32/32, `test:ext` col finto cswap.
- Tag `v0.2.0` pubblicato; CI e Release falliti subito per il billing di GitHub Actions (pagamento o limite di spesa): da rilanciare dall'utente.
- `dist-locale/` rifatta con i file 0.2.0 (setup NSIS per utente, zip portatile, vsix win32-x64; `swapdeck-cli` e `libunwind.dll` del vsix partono col PATH ridotto). Corretto `dist:local`, che copiava il primo .vsix trovato in `apps/vscode/dist` (anche di una versione vecchia).
- Sul PC dell'utente: chiusa la 0.1.0 portatile in esecuzione da `dist-locale` (motore acceso nella config reale, avvio automatico registrato); config reale portata a `autostart:false` e motore spento (copia `config.json.bak-*` accanto), voce Run tolta; installata la 0.2.0 per utente in `%LOCALAPPDATA%\SwapDeck` e avviata minimizzata (log: avvio automatico spento, motore sospeso perché spento); .vsix 0.2.0 installato con `swapdeck.engine.enabled: false` (copia `settings.json.bak-*`), finestre VS Code da ricaricare a cura dell'utente.
- Prove reali (macchine pulite, motore con cambi veri, Linux e macOS): `docs/PROVE-REALI.md`.
- Domande aperte: invariate, più rileggere i testi delle «i» e confermare il motore spento sul PC (nella config reale era acceso, con la 0.1.0 in esecuzione; si riaccende dalla scheda Regole).

## Correzioni alla 0.2.0 → v0.2.1 (19/10)
- Cartella del progetto rinominata da `WidgetCswap` a `SwapDeck`: dipendenze reinstallate (i link di pnpm puntavano al vecchio percorso), schema in `PIANO.md` corretto.
- «Configurazione di cswap» → «Impostazioni di cswap auto», chiusa sotto il pulsante «Avanzate» (con tooltip; la config di cswap si legge solo all'apertura) e testo che dice che SwapDeck non usa questi valori.
- Vista stretta: la causa vera era un nome di organizzazione lungo e senza spazi («email's Organization») che allargava la griglia di `.sd-app`. Corretto con `grid-template-columns: minmax(0, 1fr)` e `overflow-wrap: anywhere` sull'email; nuovo E2E in inglese a 280 px con organizzazione lunga.
- Lingua in VS Code: l'estensione ora usa `language` della config comune (`preferredLocaleOf`/`viewLocale` in `protocol.ts`, riletta all'avvio, dopo `setPreferences` e a ogni cambio della config) e la manda alle viste; prima seguiva solo la lingua di VS Code.
- Verifiche: lint, unit (types 3, ui 378, desktop 21, vscode 81), Playwright 33/33, `test:ext` tutto ok. Vitest con `--pool=threads`: col pool predefinito oggi i worker non si chiudono (`kill EPERM`), problema dell'ambiente.
- v0.2.1 (commit 6409881, tag non pubblicato): installata sul PC (setup NSIS per utente, app avviata nascosta) e .vsix 0.2.1 in VS Code; config reale lasciata com'è su richiesta dell'utente (motore acceso dall'utente il 19/10 alle 13:12, cswap-guard ancora attivo: SwapDeck si sospende per conflitto salvo «Ignora»).
- `dist:local`: l'NSIS mancava per os error 32. Causa: cargo mette `target/release` nel PATH degli script di build e lo script di Tauri (gnullvm) caricava il `libunwind.dll` rimasto lì, poi non poteva sovrascriverlo copiando le risorse. Ora le DLL vecchie si cancellano prima; `build.rs` stampa la catena completa dell'errore. `dist-locale/linux/` resta tra una build e l'altra.
- Distribuzione senza GitHub Actions (billing: pagamenti falliti o limite di spesa; repo resta privato per scelta dell'utente): Linux con `pnpm dist:linux` (Docker); macOS dal collega con `docs/BUILD-MAC.md` (guida per il suo Claude, non provata su un Mac). Run `Release` solo macOS (`platforms: macos`, `dry_run`) da lanciare quando l'account GitHub è sbloccato.
- `dist:linux` provato fino a clone e `pnpm install` nel container: la compilazione non è finita per mancanza di spazio (Docker usa ~82 GB del disco C:, quasi tutti di altri progetti). Da rifare con almeno 10 GB liberi.
- Disco C: pieno durante la prima build Linux (Docker ~82 GB): liberati `target/debug`, `target/release` e la cache di build di Docker; Docker Desktop riavviato con il consenso dell'utente (bloccava anche altre sessioni).

## 20/10
- Mini-widget: tolto il doppio clic che apriva la finestra principale (si apre dall'icona nell'area di notifica). Da committare e installare.
- **F21 «Rifai login» (decisioni del 20/10, da sviluppare dopo il via):** pulsante sulla scheda dell'utenza (app e VS Code), con login scaduto o in scadenza. Flusso: pausa del motore di SwapDeck, `cswap add` sull'utenza attiva (salva le sue credenziali), `claude auth login --claudeai --email <email>` con stdin in pipe, link mostrato con «Copia link» (lo apre l'altra persona), campo «Codice» scritto sullo stdin, controllo che l'email collegata sia quella dell'utenza (altrimenti niente `add` e messaggio), `cswap add`, `cswap switch` all'utenza di prima, motore ripreso. Attesa massima 5 minuti, sempre «Annulla». Claude Code cercato nel PATH, in ~/.local/bin e nell'estensione di VS Code; se manca, messaggio e percorso in Impostazioni → Avanzate. «Aggiungi account» resta solo per le utenze nuove.
- Prova del 20/10 (consenso dell'utente, chiusa senza codice): `claude auth login` con stdin in pipe stampa «If the browser didn't open, visit: <url>» (redirect su platform.claude.com con codice da copiare) e «Paste code here if prompted >»; apre anche il browser locale (da evitare se possibile).
- F21 sviluppato (client-desktop) e rivisto (revisore: 6 rilievi, sicurezza pulita); 10 correzioni fatte (secondo agente client-desktop). Decisioni aggiunte il 20/10: `add` finale fallito → si torna sempre all'utenza di prima; chiusura della vista in VS Code → annulla subito; login avviato con `BROWSER=none` (efficacia da verificare nelle prove reali: l'utente non ha controllato la seconda prova); ricerca anche in /opt/homebrew/bin e /usr/local/bin su macOS; login non gestito in Claude Code → si blocca prima di iniziare.
- VS Code: vista laterale e scheda con `retainContextWhenHidden` (deciso dall'utente: nascondere la vista non annulla «Rifai login»; si annulla solo chiudendola). Versione 0.2.2 (Cargo.toml, apps/vscode/package.json). Da verificare dal vivo: `PROVE-REALI.md` §7bis (link su stdout o stderr, codice sbagliato, `BROWSER=none`).
- Richiesta dell'utente (20/10): nessuna credenziale va tolta. F21 non usa remove/purge/disable; l'unico login scartato è quello appena fatto con un'email diversa, che non è salvato da nessuna parte.
- Installer NSIS: la traduzione italiana di Tauri 2.12 aveva tre segnaposto rotti (`{product_name}`, per esempio in «SwapDeck è in esecuzione!»). Copia corretta in `apps/desktop/src-tauri/windows/Italian.nsh` (con `${PRODUCTNAME}`, senza BOM: Tauri lo aggiunge da sé) collegata con `customLanguageFiles`.
