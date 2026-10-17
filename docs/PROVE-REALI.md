# Prove reali

Guida per le prove che sul PC di sviluppo non si fanno (azioni di scrittura sul `cswap` vero, installazione pulita, altri sistemi operativi). Si segue su una VM o su un PC di prova. Per ogni prova: spunta la casella se l'esito atteso si verifica, altrimenti annota cosa è successo (vedi §9).

Riferimenti: installazione e motore in `README.md`, contratto di `cswap` in `docs/CSWAP-CONTRATTO.md`, funzioni F0–F20 in `docs/PIANO.md` §1, rilascio in `docs/RILASCIO.md`.

## 1. Preparazione

- [ ] PC o VM **senza sessioni importanti** di Claude Code: le prove cambiano account di continuo e il cambio di `cswap` è **globale** (vale per tutte le sessioni e le finestre della macchina).
- [ ] 2–3 account Claude **di prova** (usa e getta). Con un solo account molte prove sul motore non si possono fare.
- [ ] Nessuna attività pianificata o script che richiami `cswap` (altrimenti il motore si sospende, vedi §3).
- [ ] Una VM ha snapshot «pulita» da ripristinare tra una prova e l'altra.
- [ ] Scaricati gli asset della release (README, «Installazione»; repo privato: servono i permessi di collaboratore) e, se serve, `cswap` 0.26.0 o successivo.
- [ ] Prima di ogni sessione annota: versione di SwapDeck (`swapdeck-cli --version`), sistema operativo e versione.

## 2. Windows pulito (10 e 11)

Esegui tutto su Windows 10 e su Windows 11, utente **non amministratore**, senza `cswap`, senza VS Code.

| # | Prova | Esito atteso | OK |
|---|---|---|---|
| 2.1 | Esegui `SwapDeck_<versione>_x64-setup.exe` | SmartScreen «Windows ha protetto il PC» (app non firmata): «Ulteriori informazioni» → «Esegui comunque». Nessuna richiesta UAC | [ ] |
| 2.2 | Fine installazione | Installata in `%LOCALAPPDATA%\SwapDeck`; collegamento `SwapDeck` nel menu Start; `swapdeck.exe` e `swapdeck-cli.exe` presenti | [ ] |
| 2.3 | WebView2 assente (Windows 10 senza Edge aggiornato, se riesci) | L'installer lo scarica; l'app si apre | [ ] |
| 2.4 | Primo avvio **senza `cswap`** | Procedura guidata: propone di installare `uv` e `claude-swap` per utente, con conferma; nessun admin; poi controllo versione contro PyPI | [ ] |
| 2.5 | Rifiuta l'installazione guidata | L'app resta usabile con l'avviso «cswap non trovato», nessun errore bloccante | [ ] |
| 2.6 | Aggiungi un account con login (equivale a `cswap add`, serve Claude Code già loggato) | Compare la scheda dell'account con consumi 5h e 7 giorni | [ ] |
| 2.7 | Aggiungi un secondo account con token (`add-token`) | Compare la seconda scheda; il token non è mai visibile né nei messaggi di errore | [ ] |
| 2.8 | Icona nell'area di notifica | Anello con il consumo dell'account attivo; tooltip con lo stato; menu con mappe, cambio account, apertura dashboard, uscita | [ ] |
| 2.9 | Mini-widget | Sempre in primo piano e trascinabile; Aggiorna, Comprimi (striscia con %), Nascondi; posizioni ricordate dopo il riavvio | [ ] |
| 2.10 | Notifiche (provoca un avviso, per esempio «cambio avvenuto» o un test dalla pagina Notifiche) | Il toast compare con il nome **«SwapDeck»** (non «Windows PowerShell») | [ ] |
| 2.11 | Avvio automatico (default acceso al primo avvio) | Esci dalla sessione e rientra: l'app riparte da sola **nell'area di notifica, senza finestra**. Voce `SwapDeck` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` con valore `"<percorso>\swapdeck.exe" --minimized` (percorso tra virgolette). Nel log (2.17) la riga `avvio automatico: attivo (Enable)` | [ ] |
| 2.12 | Spegni l'avvio automatico dalle impostazioni | La voce di `Run` sparisce; al login successivo l'app non parte | [ ] |
| 2.13 | `swapdeck.exe --headless --once` da un'attività pianificata o da `Win+R` | Nessuna finestra né console; esce dopo un giro; se il motore è spento in config esce con errore chiaro, non con un crash | [ ] |
| 2.14 | Zip portatile: estrai in una cartella **nuova**, avvia `swapdeck.exe` | Parte senza installare; le notifiche, senza il collegamento Start, compaiono sotto «Windows PowerShell» (atteso, README «Modalità senza interfaccia») | [ ] |
| 2.15 | Disinstalla da «App installate» | Rimossi `%LOCALAPPDATA%\SwapDeck`, collegamento Start e voce `Run`; processo `swapdeck` chiuso | [ ] |
| 2.16 | Dopo la disinstallazione | `%APPDATA%\swapdeck` (config, storico) può restare; le credenziali di `cswap` (`~\.claude-swap-backup`) restano intatte. Annota cosa resta | [ ] |
| 2.17 | Log e diagnostica | `%APPDATA%\swapdeck\logs\swapdeck.log` esiste dopo il primo avvio, con `avvio dell'app: SwapDeck <versione> su Windows …`; Strumenti → «Apri cartella dei log» apre Esplora file lì; «Copia diagnostica» mette negli appunti versioni, sistema, motore, mappa attiva e ultime righe; nessuna email completa, la home è `~` | [ ] |

## 3. Motore con cambi veri

Con 2–3 account di prova caricati. Motore **spento** all'inizio. Le regole sono in README «Motore delle regole».

| # | Prova | Esito atteso | OK |
|---|---|---|---|
| 3.1 | Scheda Regole → **Solo osservazione** (`dryRun`) | Il motore decide e registra («Adesso andrebbe a…») ma **non** cambia account, anche oltre soglia | [ ] |
| 3.2 | Prima accensione (`enabled`) | Chiede **conferma** a video prima di accendere; annullando resta spento | [ ] |
| 3.3 | Accendi con lista di almeno due utenze | Stato «acceso»; trattini rossi alle soglie sulle barre; il motore tiene il lock | [ ] |
| 3.4 | Porta la soglia 5h dell'account attivo sotto il suo consumo attuale (per esempio 5h a 20 con consumo oltre) | Entro un giro (60 s minimo) passa alla riserva scelta dal criterio; notifica «cambio avvenuto» e riga nello storico con il motivo | [ ] |
| 3.5 | Stesso test sulla soglia 7 giorni (`sevenDayThreshold`) | Cambio come sopra, motivo sui 7 giorni | [ ] |
| 3.6 | Pausa minima | Un secondo cambio entro `minSwitchGapSecs` (300 s) non avviene | [ ] |
| 3.7 | Ritorno dopo il reset del principale (aspetta il reset reale della finestra o usa un account con reset vicino) | Torna al principale solo quando scende sotto la soglia meno l'isteresi (default 10; preset Principale con riserve 45) | [ ] |
| 3.8 | Dati mancanti o non aggiornati (spegni la rete un giro) | Nessun cambio alla cieca; avviso «dati non aggiornati»; il motore riprende al ritorno della rete | [ ] |
| 3.9 | Mappe: i preset **Principale con riserve, Eco, Sport, Sicura** sono in sola lettura | Non si modificano, rinominano né eliminano | [ ] |
| 3.10 | **Clona come mappa personale** su un preset | Chiede un nome (non vuoto, unico); apre la copia modificabile; il preset resta com'era | [ ] |
| 3.11 | Cambio mappa dal menu dell'icona, dal mini-widget e dalla scheda Regole | La mappa attiva cambia ovunque; la personale attiva non si elimina | [ ] |
| 3.12 | Preset senza lista | Non accende il motore e rimanda a «Clona» | [ ] |
| 3.13 | Sessione **Claude Code aperta** (terminale) mentre il motore cambia | Dopo il cambio la sessione passa al nuovo account (verifica con `/status` o col prossimo messaggio); nessun errore di login | [ ] |
| 3.14 | Consumo vero fino a **429** su un account di prova (se non lo raggiungi, annota «non provato») | Nessun crash, errore leggibile, dati marcati non aggiornati, il motore non entra in loop di cambi | [ ] |
| 3.15 | Conflitto: avvia `cswap auto` (fuori da SwapDeck, nel terminale) | Entro un giro il motore si **sospende**: icona «sospesa», riga nel tooltip, avviso sopra la dashboard | [ ] |
| 3.16 | Ferma `cswap auto` | Il motore riprende da solo (notifica «motore ripreso») | [ ] |
| 3.17 | `ignoredConflicts` con l'id del conflitto | Il motore non si sospende più per quel conflitto | [ ] |
| 3.18 | Un solo motore: apri una seconda istanza o `swapdeck-cli headless` | Esce con codice **3** (lock occupato) o resta in osservazione | [ ] |

## 4. Comandi che scrivono, dall'app

Solo con account **di prova**. Contratto in `docs/CSWAP-CONTRATTO.md` §3.

| # | Prova | Esito atteso | OK |
|---|---|---|---|
| 4.1 | `switch` diretto a un altro account (dashboard o menu icona) | Nessuna conferma; l'account attivo cambia; con `reason` diverso da `switched` (per esempio `already-active`) l'app mostra il messaggio e non un errore | [ ] |
| 4.2 | `switch` con criteri `best` e `next-available` | Resta fermo se non c'è margine migliore (`already-best`); salta gli account al limite | [ ] |
| 4.3 | Alias: imposta, cambia, duplicato | Alias mostrato ovunque; l'alias già usato dà errore leggibile | [ ] |
| 4.4 | `disable` di un account **non attivo**, poi `enable` | Disabilitato = escluso dalla rotazione e dal motore; `enable` lo riporta | [ ] |
| 4.5 | `disable` dell'account **attivo** | Chiede conferma; resta attivo fino al prossimo cambio | [ ] |
| 4.6 | `remove` di un account di prova | Conferma; l'account sparisce; con una sessione `cswap run` viva rifiuta con messaggio chiaro | [ ] |
| 4.7 | `purge` | **Doppia conferma** con la parola da digitare; mostra prima l'elenco di ciò che rimuove; il login corrente di Claude Code non viene toccato. Provalo solo a fine giornata, su VM | [ ] |
| 4.8 | `export` di un account | Avviso che il file contiene **credenziali in chiaro**; file creato; cancellalo a fine prova | [ ] |
| 4.9 | `import` dello stesso file | Senza sovrascrittura salta gli account già presenti; con sovrascrittura chiede conferma | [ ] |
| 4.10 | `swap`, `move`, `map`/`unmap` | Conferma per `swap`, `move`, `unmap`; la pagina si aggiorna | [ ] |
| 4.11 | «Gestisci utenza» di un account | Pagina di dettaglio con alias, attiva/disattiva, rimuovi, slot, terminale, «Indietro» | [ ] |
| 4.12 | `run` (apre un terminale come quell'account) | Si apre un nuovo terminale con Claude Code su quell'account, senza cambiare quello globale | [ ] |
| 4.13 | Aggiornamento di `cswap` dall'app | Su Windows usa `uv tool upgrade claude-swap`; dopo, `schemaVersion` verificato | [ ] |

## 5. Estensione VS Code (senza app desktop)

Installa solo il `.vsix` (`code --install-extension swapdeck-vscode-<piattaforma>-<versione>.vsix`), poi «Ricarica finestra». L'app desktop **non** è installata.

| # | Prova | Esito atteso | OK |
|---|---|---|---|
| 5.1 | Avvio | Nessun errore; barra di stato con i consumi; se manca `cswap` propone l'installazione o indica `swapdeck.cliPath` | [ ] |
| 5.2 | Icona SwapDeck nella barra laterale | Apre l'app completa a schede, uguale al desktop | [ ] |
| 5.3 | Clic sulla barra di stato | Apre la vista SwapDeck (il cambio di account resta in «SwapDeck: Cambia account» e nella vista) | [ ] |
| 5.4 | «SwapDeck: Cambia mappa» | Elenco con preset marcati «preset»; la scelta vale per tutte le finestre | [ ] |
| 5.5 | Apri **due o tre finestre** VS Code | Un solo motore (chi ha il lock); le altre in osservazione | [ ] |
| 5.6 | Chiudi la finestra che tiene il motore | Il motore passa a un'altra finestra entro un giro; nessun doppio cambio | [ ] |
| 5.7 | Notifiche con più finestre | Ogni avviso compare **una volta sola** (un solo mittente) | [ ] |
| 5.8 | Modifica un'impostazione in una finestra (alias, mappa, acceso) | Si propaga alle altre senza ricaricare, anche nella scheda aperta; se lì c'è un modulo in modifica compare «Modificato in un'altra finestra» con «Ricarica» | [ ] |
| 5.9 | Scheda **Chi consuma** | Classifica per progetto, sessione, agente e modello su 5h e 7 giorni; avanzamento sulla prima lettura; nessun testo di messaggi mostrato | [ ] |
| 5.10 | App desktop e VS Code insieme | Un solo motore; l'altro in osservazione, senza notifiche doppie | [ ] |
| 5.11 | App desktop aperta su Impostazioni (o Notifiche, Cartelle, configurazione di cswap); cambia lo stesso valore da VS Code o con `swapdeck-cli config set` | Entro un paio di secondi l'app mostra il nuovo valore; con un modulo in modifica compare «Modificato in un'altra finestra» con «Ricarica»; un salvataggio fatto dall'app non fa comparire l'avviso | [ ] |
| 5.12 | Motore sospeso (per esempio con `cswap auto` avviato a mano) con VS Code come titolare | Sopra le schede della vista laterale e del pannello: «Motore sospeso: altro motore attivo (id)»; sparisce quando il conflitto finisce | [ ] |

## 6. Linux e macOS

### Linux (Ubuntu 22.04 o successivo; prova su GNOME **e** KDE)

| # | Prova | Esito atteso | OK |
|---|---|---|---|
| 6.1 | AppImage: `chmod +x` e avvio | Parte senza `sudo`; procedura guidata se manca `cswap` | [ ] |
| 6.2 | `.deb`: `sudo dpkg -i` | Installa in `/usr/bin`; `swapdeck` e `swapdeck-cli` nel PATH | [ ] |
| 6.3 | Area di notifica | GNOME **non** mostra le icone di stato senza l'estensione «AppIndicator and KStatusNotifierItem Support» (`gnome-shell-extension-appindicator`; su Ubuntu è già attiva come «Ubuntu AppIndicators»). Controlla in «Estensioni» che sia attiva; se manca, l'app gira ma senza icona: annotalo e prova ad aprirla dal menu applicazioni (la seconda istanza mostra la finestra). KDE: icona e menu funzionano | [ ] |
| 6.4 | Notifiche | Compaiono via D-Bus; senza sessione grafica (SSH) si saltano senza errori | [ ] |
| 6.5 | Avvio automatico | File `~/.config/autostart/SwapDeck.desktop` (sempre `~/.config`, anche con `XDG_CONFIG_HOME` diverso) con `Exec=<percorso> --minimized`: `.deb` → `/usr/bin/swapdeck`; AppImage → il file `.AppImage` (non il punto di montaggio `/tmp/.mount_…`). Al login l'app parte nell'area di notifica senza finestra | [ ] |
| 6.5b | AppImage spostata o rinominata | La voce punta ancora al vecchio percorso e al login non parte nulla finché non riapri l'AppImage dal nuovo posto: a quell'avvio la voce si riscrive. Annota il comportamento | [ ] |
| 6.5c | Avvio automatico spento dal sistema (GNOME «Applicazioni d'avvio»/Tweaks, KDE «Avvio automatico») | Se il desktop toglie il file o vi scrive `Hidden=true` / `X-GNOME-Autostart-enabled=false`, al riavvio SwapDeck lo legge come spento e **non** lo riattiva; l'interruttore dell'app si allinea | [ ] |
| 6.5d | `.deb` avviato da un terminale aperto dentro un'altra AppImage (es. editor in AppImage) | La voce punta a `/usr/bin/swapdeck`, non all'AppImage dell'editor (`APPIMAGE` ereditata si ignora se l'app non gira dentro `APPDIR`) | [ ] |
| 6.6 | Dati | `~/.config/swapdeck` e `~/.local/share/swapdeck`; `cswap` in `~/.local/share/claude-swap` | [ ] |
| 6.7 | `swapdeck-cli headless --once --notify=stdout` | Righe JSON; codice di uscita 0 | [ ] |
| 6.7b | Log | `~/.local/share/swapdeck/logs/swapdeck.log` (cartella `0700`, file `0600`) con le righe di 6.7; «Apri cartella dei log» usa `xdg-open` | [ ] |

### macOS (Apple Silicon e Intel, se hai entrambi)

| # | Prova | Esito atteso | OK |
|---|---|---|---|
| 6.8 | `.dmg`: trascina in `/Applications` | Copiata; al primo avvio Gatekeeper blocca (sviluppatore non identificato) | [ ] |
| 6.9 | Tasto destro → «Apri» → conferma (oppure Privacy e sicurezza → «Apri comunque») | L'app si apre; dal secondo avvio non chiede più | [ ] |
| 6.10 | Barra dei menu | Icona con il consumo; menu con mappe e cambio account | [ ] |
| 6.11 | Notifiche | Centro notifiche, col nome «SwapDeck» se l'app è in `/Applications` | [ ] |
| 6.12 | Portachiavi | `cswap` può chiedere accesso al Portachiavi: l'app non si blocca e non perde l'account | [ ] |
| 6.13 | Avvio al login | LaunchAgent `~/Library/LaunchAgents/SwapDeck.plist` (`Label` `SwapDeck`, `ProgramArguments` = `/Applications/SwapDeck.app/Contents/MacOS/swapdeck` e `--minimized`, `RunAtLoad`, niente `KeepAlive`); non si carica subito: vale dal login successivo, e l'app parte nella barra dei menu senza finestra. Da macOS 13 compare la notifica «Elemento di sfondo aggiunto» | [ ] |
| 6.13b | Impostazioni → Generali → Elementi login → «Consenti in background»: spegni `swapdeck` | Al login l'app non parte, ma il file resta e l'interruttore di SwapDeck resta acceso (limite noto: macOS non dice all'app che l'ha spenta). Annota il nome mostrato | [ ] |
| 6.13c | Primo avvio direttamente dal `.dmg` o da Download (App Translocation) | Nessuna voce scritta con il percorso casuale in quarantena: nel log `avvio automatico non aggiornato: app avviata in quarantena…`; dopo averla spostata in `/Applications` e riaperta, la voce si scrive | [ ] |
| 6.14 | Log | `~/Library/Application Support/swapdeck/logs/swapdeck.log`; «Apri cartella dei log» apre il Finder lì | [ ] |

## 7. Aggiornamento automatico 0.1.0 → 0.2.0

Richiede il repo **pubblico** con release pubblicata (`docs/RILASCIO.md`, «Attivare l'updater»): finché è privato l'endpoint risponde 404.

| # | Prova | Esito atteso | OK |
|---|---|---|---|
| 7.1 | Installa la 0.1.0 e aggiungi `"updates": { "enabled": true }` al `config.json` | L'updater è acceso solo per questa installazione | [ ] |
| 7.2 | Pubblica la 0.2.0 e lascia l'app aperta | Scarica e **verifica la firma** in background; notifica di aggiornamento | [ ] |
| 7.3 | Esci dall'app | La 0.2.0 si installa all'uscita, senza riaprirsi; il riavvio mostra 0.2.0 | [ ] |
| 7.4 | Configurazione dopo l'aggiornamento | Mappe della 0.1.0 migrate (un preset modificato diventa «<Nome> (personale)»), account e storico intatti, nessuna mappa persa | [ ] |
| 7.5 | Linux | Si aggiorna solo l'AppImage; il `.deb` no (atteso) | [ ] |
| 7.6 | Firma errata o `latest.json` alterato (prova negativa, se puoi) | L'aggiornamento viene rifiutato e l'app continua a funzionare | [ ] |

## 8. Casi difficili

| # | Prova | Esito atteso | OK |
|---|---|---|---|
| 8.1 | Utente Windows con **spazi e accenti** nel nome (per esempio `C:\Users\Maria Rossì`) | Installazione, `cswap`, config e notifiche funzionano; percorsi con spazi gestiti | [ ] |
| 8.2 | Percorso del profilo su altro disco o sincronizzato (OneDrive) | Config e storico si scrivono; nessun errore di lock | [ ] |
| 8.3 | **Proxy aziendale** (variabili `HTTPS_PROXY` o proxy di sistema) | `uv` / PyPI / consumi raggiungibili; in caso contrario messaggio chiaro, non un blocco | [ ] |
| 8.4 | Proxy con autenticazione o ispezione TLS | Annota l'errore esatto (senza credenziali); l'updater può fallire: deve restare silenzioso | [ ] |
| 8.5 | **Antivirus** (Defender e, se presente, quello aziendale) | Installer e `swapdeck.exe` non bloccati; se segnalati, annota nome della minaccia e della soluzione | [ ] |
| 8.6 | Orologio del sistema avanti/indietro di qualche ora | Nessun cambio spurio; gli orari di reset restano coerenti | [ ] |
| 8.7 | Disco quasi pieno o cartella dati in sola lettura | Errore leggibile, nessun crash | [ ] |

## 9. Come riportare un problema

Il modo più rapido: **Strumenti → Diagnostica → «Copia diagnostica»** (in VS Code anche il comando «SwapDeck: Copia diagnostica» o il menu «…» della vista laterale; da terminale `swapdeck-cli diagnostics`) e incolla il testo nella segnalazione. Contiene versioni di SwapDeck e `cswap`, sistema operativo, stato del motore, mappa attiva e le ultime 200 righe del log, già ripulite (niente token, email mascherate, home sostituita da `~`). Rileggilo comunque prima di inviarlo.

Il log completo è `logs/swapdeck.log` (più `swapdeck.log.1` … `.4`) nella cartella dati; «Apri cartella dei log» lo mostra:

| Sistema | Cartella dei log |
|---|---|
| Windows | `%APPDATA%\swapdeck\logs` |
| Linux | `~/.local/share/swapdeck/logs` |
| macOS | `~/Library/Application Support/swapdeck/logs` |
| Con `SWAPDECK_HOME` | `$SWAPDECK_HOME/data/logs` |

Per un problema difficile da riprodurre rilancia con `SWAPDECK_LOG=debug` (variabile d'ambiente del processo) e allega le righe del momento.

Includi inoltre:

1. **Versione** di SwapDeck (`swapdeck-cli version`) e di `cswap` (`cswap --version`), **sistema operativo** e versione, forma di installazione (NSIS, zip, AppImage, `.deb`, `.dmg`, solo `.vsix`): tutto già nella diagnostica, tranne la forma di installazione.
2. **Numero della prova** di questa guida e **passi** per riprodurla, con esito atteso e osservato.
3. **Cosa ti aiuta a diagnosticare**, nell'ordine:
   - schermata dell'avviso o della scheda;
   - sezione `engine` di `config.json` (cartelle in README, «Dove stanno config e dati»);
   - righe pertinenti di `swapdeck.log` (decisioni e cambi del motore iniziano con `motore:`; errori di `cswap` con classe, codice e tipo);
   - output di `swapdeck-cli headless --once --dry-run --notify=stdout` per i problemi del motore;
   - righe pertinenti di `claude-swap.log` nella cartella di backup di `cswap` (Windows e macOS `~/.claude-swap-backup`; Linux `~/.local/share/claude-swap`);
   - lo storico (`history/AAAA-MM-GG.jsonl` nella cartella dati) solo se serve.
4. **Prima di allegare, oscura**: token e chiavi (mai nei log o nei messaggi), **email complete** (scrivi `a***@dominio` o l'alias dell'account) e percorsi con il tuo nome. Non allegare mai `~/.claude-swap-backup/credentials/`, `~/.claude/.credentials.json` né i file di `export`.
5. Se hai fatto `export` per una prova, cancella il file.

Una prova non fatta si segna «non provato», con il motivo: non conta come superata.
