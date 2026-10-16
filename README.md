# SwapDeck

Interfaccia grafica per [`cswap`](https://pypi.org/project/claude-swap/) (claude-swap): mostra i limiti di tutti i tuoi account Claude Code, li gestisce e, se vuoi, cambia account da sola secondo regole configurabili.

Si installa in **una** di queste forme (a scelta):

| Forma                     | Per chi                               | Cosa dà                                                                                      |
| ------------------------- | ------------------------------------- | -------------------------------------------------------------------------------------------- |
| App nell'area di notifica | chi vuole un widget sempre visibile   | icona con anello dei consumi, dashboard, mini-widget flottante, notifiche, avvio al login    |
| Estensione VS Code        | chi lavora tutto il giorno in VS Code | consumi nella barra di stato, cambio rapido, pannello con la stessa app a schede del desktop |

Entrambe usano lo stesso motore (Rust) e la stessa config, e coprono i comandi di `cswap` (list, switch, add, remove, alias, swap, map, export/import, run...). Interfaccia in italiano e inglese.

## Requisiti

- Windows (priorità), Linux o macOS. Nessun permesso di amministratore: tutto è per utente.
- `cswap` 0.26.0 o successivo. Se manca, SwapDeck lo installa al primo avvio (vedi sotto).
- Su Windows serve WebView2 (di norma già presente; l'installer lo scarica se manca).
- Accesso al repo GitHub `Gian20luca/SwapDeck`: è **privato**, quindi vanno chiesti i permessi di collaboratore per scaricare le release.

## Installazione

Scarica gli asset dalla pagina Releases del repo (serve essere loggati con un account abilitato).

| Sistema | App                                                                                          | Installata in                                        | Estensione VS Code                                                                            |
| ------- | -------------------------------------------------------------------------------------------- | ---------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| Windows | `SwapDeck_<versione>_x64-setup.exe` (NSIS, per utente)                                       | `%LOCALAPPDATA%\SwapDeck`                            | `swapdeck-vscode-win32-x64-<versione>.vsix`                                                   |
| Linux   | `SwapDeck_<versione>_amd64.AppImage` (si aggiorna da sola) o `SwapDeck_<versione>_amd64.deb` | dove metti l'AppImage; `/usr/bin` con il `.deb`      | `swapdeck-vscode-linux-x64-<versione>.vsix`                                                   |
| macOS   | `SwapDeck_<versione>_aarch64.dmg` (Apple Silicon) o `SwapDeck_<versione>_x64.dmg` (Intel)    | `/Applications/SwapDeck.app` (trascinata dal `.dmg`) | `swapdeck-vscode-darwin-arm64-<versione>.vsix` o `swapdeck-vscode-darwin-x64-<versione>.vsix` |

L'installazione contiene `swapdeck` e `swapdeck-cli` (su macOS in `SwapDeck.app/Contents/MacOS/`). `swapdeck-cli` c'è anche come file a sé nelle release, con il target nel nome (per esempio `swapdeck-cli-x86_64-pc-windows-msvc.exe`, `swapdeck-cli-x86_64-unknown-linux-gnu`, `swapdeck-cli-aarch64-apple-darwin`).

Estensione: `code --install-extension swapdeck-vscode-<piattaforma>-<versione>.vsix`, oppure «Estensioni» → «...» → «Installa da VSIX...». Poi «Ricarica finestra».

Avvisi attesi, perché i pacchetti non sono firmati con certificati dei produttori dei sistemi:

- **Windows SmartScreen:** «Windows ha protetto il PC» → «Ulteriori informazioni» → «Esegui comunque».
- **macOS Gatekeeper:** l'app è di «sviluppatore non identificato». Tasto destro sull'app → «Apri» → conferma; oppure Impostazioni di Sistema → Privacy e sicurezza → «Apri comunque».
- **Linux:** rendi eseguibile l'AppImage (`chmod +x`). Il `.deb` richiede `sudo` solo per `dpkg`; l'AppImage no.

Aggiornamenti automatici: spenti di default finché il repo è privato; si scarica a mano la nuova release. Su Linux il `.deb` si aggiorna comunque a mano.

## Primo avvio

1. SwapDeck cerca `cswap`. Se non lo trova propone di installare `uv` e `claude-swap` **per utente** (con conferma): nessun admin, nessun file di sistema.
2. Controlla la versione installata contro l'ultima su PyPI; se ce n'è una nuova avvisa e aggiorna con conferma (`uv tool upgrade claude-swap`).
3. Se non hai ancora account, aggiungili dalla dashboard (equivale a `cswap add`).
4. L'app parte da sola al login (si spegne con `"autostart": false` in `config.json`).

## Dashboard, mini-widget e notifiche

- **Icona nell'area di notifica** (app): anello con il consumo dell'account attivo; il tooltip riassume lo stato. Cambia aspetto se il motore è sospeso.
- **Dashboard:** una scheda per account con consumi a 5 ore e 7 giorni, ritmo atteso rispetto a quello reale, stima di esaurimento, storico con grafici, gestione completa degli account.
- **Mini-widget:** piccola finestra sempre in primo piano, trascinabile, con l'account attivo: barre 5 ore, 7 giorni e per modello, ora dell'ultima lettura e selettore della mappa attiva. Il trattino sulla barra è la soglia del motore e nel suo tooltip c'è l'utenza a cui andrebbe adesso. In alto a destra: **Aggiorna** (rilegge i consumi; `cswap` li rinnova al massimo ogni 3 minuti), **Comprimi** (striscia con alias e % delle 5 ore; dalla striscia si torna con «Espandi») e **Nascondi** (resta l'icona nell'area di notifica). Le posizioni della striscia e del widget esteso si ricordano a parte.
- **Notifiche di sistema:** si scelgono nella pagina **Notifiche** (dalle impostazioni, «Notifiche…»), divisa in tre gruppi:
  - _Cambi di utenza_: preavviso di cambio (punti o minuti prima della soglia), cambio avvenuto, ritorno alla prima utenza, nessuna riserva;
  - _Consumi e reset_: soglie sulle 5 ore e sui 7 giorni (fino a 5 per finestra, default 80 e 95), esaurimento previsto, reset imminente, reset avvenuto, limite per modello;
  - _Stato e manutenzione_: motore sospeso o ripreso, login da rifare o in scadenza, dati non aggiornati, aggiornamenti di `cswap` o di SwapDeck.

  Per gli avvisi su consumi e reset si sceglie se seguire solo l'utenza attiva o tutte. In più: **Non disturbare** a orari (anche a cavallo della mezzanotte, con i critici che possono passare) e un **riepilogo giornaliero** all'ora scelta. Le regole stanno nella sezione `notifications` di `config.json` e valgono dalla lettura successiva.

- **Conferme:** le azioni delicate (`remove`, `import`, `export`, `swap`/`move`, `unmap`, `disable` dell'account attivo, `purge`) chiedono conferma; `purge` chiede di digitare una parola. Il cambio di account non chiede nulla.

## Motore delle regole

Cambia account al posto tuo. È **spento di default**.

| Impostazione (`engine`) | Significato                                                           | Default         |
| ----------------------- | --------------------------------------------------------------------- | --------------- |
| `enabled`               | acceso: decide e cambia                                               | `false`         |
| `dryRun`                | **sola osservazione**: decide e registra, non cambia                  | `false`         |
| `order`                 | utenze in ordine: la prima è la principale, poi le riserve            | nessuno         |
| `primary`               | account principale (derivato da `order` quando c'è)                   | nessuno         |
| `reserves`              | riserve in ordine (derivate da `order`); vuoto = tutti gli altri      | vuoto           |
| `excluded`              | account mai scelti                                                    | vuoto           |
| `fiveHourThreshold`     | soglia di uscita sulle 5 ore (%)                                      | 95              |
| `sevenDayThreshold`     | soglia sui 7 giorni (`null` = solo blocco al 100%)                    | 98              |
| `reserveCriterion`      | criterio di scelta della riserva (vedi sotto)                         | `lowest-weekly` |
| `returnToPrimary`       | torna al principale dopo il suo reset                                 | `true`          |
| `returnHysteresis`      | punti sotto la soglia richiesti al principale per tornarci (isteresi) | 10              |
| `minSwitchGapSecs`      | pausa minima tra due cambi                                            | 300             |
| `maxUsageAgeSecs`       | età massima di un consumo per fidarsene                               | 900             |
| `ignoredConflicts`      | id dei conflitti da ignorare (vedi sotto), senza maiuscole/minuscole  | vuoto           |

L'intervallo di controllo non sta in `engine` ma in cima alla config: `pollIntervalSeconds` (default e minimo 60, in secondi).

I 9 criteri di `reserveCriterion` (nome nella scheda Regole e valore in config). A parità vale l'ordine della lista; se il criterio non può scegliere (dati mancanti, modello assente) ripiega sulla settimanale più bassa e lo scrive nel motivo.

| Criterio                     | Valore           | Sceglie                                                                                 |
| ---------------------------- | ---------------- | --------------------------------------------------------------------------------------- |
| Settimanale più bassa        | `lowest-weekly`  | la % 7 giorni più bassa; a parità la 5 ore più bassa                                    |
| Migliore (più margine)       | `best`           | più margine rispetto alle soglie (come `cswap switch --strategy best`)                  |
| Ordine fisso                 | `fixed-order`    | sempre la prima utenza libera nell'ordine, anche al ritorno                             |
| Reset settimanale più vicino | `reset-soonest`  | la finestra 7 giorni che si azzera prima; a parità la 7 giorni più bassa                |
| Ritmo sicuro                 | `pace-safe`      | chi arriva al reset settimanale senza esaurirsi al ritmo attuale, col margine maggiore  |
| 5 ore più libera             | `lowest5h`       | la finestra 5 ore più libera; a parità la 7 giorni più bassa                            |
| Punteggio bilanciato         | `balanced-score` | punteggio pesato di margine 5 ore, margine 7 giorni, vicinanza del reset e ritmo        |
| Settimanali livellate        | `even-spread`    | la più sotto la media settimanale della lista; a parità la 5 ore più bassa              |
| Più margine sul modello      | `model-headroom` | più margine sul limite settimanale del modello in `headroomModel` (per esempio «Fable») |

**Mappe.** Una mappa è un insieme di regole con nome; la scheda Regole ne gestisce più di una (Nuova, Rinomina, Duplica, Elimina, Ripristina preset) e la mappa attiva si cambia anche dal mini-widget e, in VS Code, con «SwapDeck: Cambia mappa». Al primo avvio ci sono «Personale» (le regole attuali) e quattro preset, che cambiano criterio e soglie ma lasciano lista, esclusi, conflitti ignorati, `enabled` e `dryRun`:

| Preset                 | Criterio                     | 5 ore | 7 giorni | Isteresi del ritorno |
| ---------------------- | ---------------------------- | ----- | -------- | -------------------- |
| Principale con riserve | Settimanale più bassa        | 95    | 98       | 45 (5 ore sotto 50)  |
| Eco                    | Reset settimanale più vicino | 90    | 98       | 10                   |
| Sport                  | 5 ore più libera             | 98    | 98       | 10                   |
| Sicura                 | Ritmo sicuro                 | 95    | 98       | 10                   |

In tutti il ritorno alla principale è acceso, la pausa tra due cambi è di 300 s e i consumi valgono per 900 s. La scheda mostra anche «Adesso andrebbe a» (l'utenza che il motore sceglierebbe se l'attiva superasse la soglia ora) e gli avvisi sulla lista, per esempio un modello che nessuna utenza riporta.

Se la riserva scelta è al limite si passa alla successiva. L'isteresi vale solo sulle finestre che hanno fatto lasciare il principale (5 ore, 7 giorni o entrambe). Le regole stanno nella sezione `engine` di `config.json`, l'app e il processo headless le rileggono a ogni giro, e si leggono/scrivono anche dalla scheda «Regole» o con `swapdeck-cli config get|set`.

**Un solo motore per utente.** Un lock impedisce che app, finestra VS Code o processo headless cambino account insieme: chi non ha il lock resta in osservazione.

**Stato «sospeso» e conflitti.** Se rileva `cswap auto` in esecuzione o un'attività, timer, cron o LaunchAgent che richiama `cswap` (per esempio lo script pianificato di un altro motore che usa `cswap`), il motore si **sospende** da solo: icona «sospeso», riga nel tooltip e avviso sopra la dashboard. Non è un errore: evita due automatismi che si contraddicono. Per riattivarlo ferma l'altro automatismo (vedi la [guida alla migrazione](docs/GUIDA-MIGRAZIONE.md)). Conta un'attività o un timer che cita `cswap` o `claude-swap` nel nome o nel comando; un'attività disabilitata dà solo un avviso. Per ignorare un conflitto noto si mette il suo id (per esempio `task:nome-attività`) in `ignoredConflicts`. Provare prima in sola osservazione (`dryRun`) è sempre sicuro.

## Modalità senza interfaccia

Per attività pianificate, server e WSL: `swapdeck-cli headless` (accanto all'app installata, nel `.vsix` in `bin/` e come file separato nelle release). L'app accetta anche `swapdeck --headless` con le stesse opzioni: su Windows non apre console né stampa nulla, quindi va bene per un'attività pianificata che non deve far comparire finestre.

```
swapdeck-cli headless [--once] [--dry-run] [--notify=os|stdout|both]
```

- senza opzioni: ciclo continuo, si ferma con Ctrl+C;
- `--once`: un solo giro e poi esce (adatto a uno scheduler);
- `--dry-run`: solo osservazione;
- `--notify`: dove vanno le notifiche (sezione `notifications` di `config.json`). `os` (default) le mostra a schermo come notifiche di sistema, anche senza app né VS Code aperti; `stdout` le scrive come righe JSON `{"event":"notification",…}` per chi ha avviato il motore (l'estensione VS Code usa questa, così non ci sono doppioni); `both` fa entrambe le cose.

Notifiche di sistema per piattaforma:

- **Windows**: toast. Se SwapDeck è installato (collegamento `SwapDeck.lnk` nel menu Start dell'utente o di tutti gli utenti) usano l'identificativo dell'app (`com.swapdeck.desktop`) e compaiono col nome SwapDeck; altrimenti, per esempio dallo zip portatile, usano l'identificativo di Windows PowerShell e compaiono sotto quel nome.
- **Linux**: D-Bus (`org.freedesktop.Notifications`). Senza bus di sessione o senza sessione grafica (cron, SSH, server) si saltano senza errori.
- **macOS**: Centro notifiche, col nome dell'app se `SwapDeck.app` è in `/Applications` o `~/Applications`.

Una notifica che non si riesce a mostrare lascia una riga nel log (una sola finché il problema resta lo stesso) e non ferma mai il motore.

Codici di uscita: 0 ok, 1 errore, 2 uso errato, 3 lock occupato (un altro motore sta già girando). Il motore deve comunque essere acceso in config (`enabled` o `dryRun`).

## Dove stanno config e dati

| Sistema | Posizione                                        |
| ------- | ------------------------------------------------ |
| Windows | `%APPDATA%\swapdeck`                             |
| Linux   | `~/.config/swapdeck` e `~/.local/share/swapdeck` |
| macOS   | `~/Library/Application Support/swapdeck`         |

Contengono `config.json`, il lock del motore (`engine.lock`, nella cartella dati) e lo storico dei consumi. Con `SWAPDECK_HOME` (percorso assoluto) si usa un'altra radice, con `config/` e `data/` sotto, utile per le prove. Le credenziali restano quelle di `cswap`.

## Problemi comuni

| Sintomo                                  | Causa probabile e rimedio                                                             |
| ---------------------------------------- | ------------------------------------------------------------------------------------- |
| Motore «sospeso»                         | c'è `cswap auto` o un altro motore attivo: ferma l'altro automatismo (guida sopra)    |
| «motore attivo in un'altra istanza»      | un'altra finestra o l'app tiene il lock: è voluto                                     |
| Il motore non cambia mai                 | spento (`enabled: false`), in sola osservazione (`dryRun: true`) o `primary` mancante |
| `cswap` non trovato                      | riapri l'app per l'installazione guidata, oppure imposta `swapdeck.cliPath` / il PATH |
| Consumi non aggiornati, errori 429       | `cswap list` ha limiti di frequenza: l'intervallo minimo è 60 s                       |
| Account con «re-login»                   | accedi di nuovo con `cswap` a quell'account                                           |
| `headless` esce con 3                    | lock occupato: un altro motore è attivo, è corretto                                   |
| Estensione inattiva dopo l'installazione | «Ricarica finestra»                                                                   |

## Per chi sviluppa

Monorepo: `crates/swapdeck-core` (Rust), `apps/desktop` (Tauri 2), `apps/cli`, `apps/vscode`, `packages/ui`, `packages/types`. Servono Node >= 22, pnpm e Rust.

```
pnpm install
pnpm build            # tutti i pacchetti JS
pnpm test             # test JS
pnpm test:rust        # cargo test --workspace
pnpm lint             # prettier + typecheck
pnpm lint:rust        # fmt + clippy
pnpm dist:local       # build Windows locale in dist-locale/
```

Prove locali: mai col motore acceso sul profilo reale; usa `SWAPDECK_HOME` e il finto cswap (`apps/desktop/README.md`). Documentazione: [docs/](docs/) (`PIANO.md`, `STATO.md`, `MAPPA-CODICE.md`, `CSWAP-CONTRATTO.md`, `RILASCIO.md`).
