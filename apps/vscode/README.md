# SwapDeck per VS Code

Monitor e gestione degli account di `cswap` tutto dentro VS Code: non serve l'app desktop. Il `swapdeck-cli` incluso nel .vsix fa motore, storico, notifiche e installazione di cswap.

## Installazione

1. Serve `cswap` installato per l'utente (lo stesso usato dal terminale); se manca, la procedura guidata della vista lo installa.
2. Da terminale: `code --install-extension swapdeck-vscode-<piattaforma>-<versione>.vsix`, oppure in VS Code «Estensioni» → «...» → «Installa da VSIX...».
3. Le finestre già aperte attivano l'estensione al prossimo «Ricarica finestra» (o alla riapertura); quelle nuove subito.

Il .vsix è per piattaforma (contiene `bin/swapdeck-cli`) e non richiede privilegi di amministratore. L'estensione gira sempre sulla macchina locale (`extensionKind: ui`), anche con Remote/WSL.

## Funzioni

- **Vista SwapDeck:** icona ad anello nella barra delle attività. Contiene l'app completa a schede, adattata alla larghezza della barra laterale (le schede vanno a capo, niente scroll orizzontale); la scheda aperta si ricorda quando la vista si nasconde. Nel titolo della vista: «Aggiorna» e «Apri in una scheda» (la stessa app in una scheda dell'editor).
- **Schede:** Account (azioni su ogni account), Cartelle (`map`/`unmap` con selettore di cartella), Regole del motore (mappe personali e preset in sola lettura «Principale con riserve», Eco, Sport e Sicura, da clonare), Storico (grafici, cambi e decisioni), Chi consuma, Strumenti (export, import, `unclaimed`, `purge`), Impostazioni (intervallo, soglie, `config set`, pagina Notifiche). Al primo avvio, se cswap manca, la procedura guidata lo installa (uv e claude-swap per l'utente); le nuove versioni di cswap si segnalano sopra le schede, e così il motore sospeso (motivo e, per i conflitti, gli id), come nell'app desktop. Lingua e tema seguono VS Code; quelli salvati in Impostazioni valgono per l'app desktop.
- **Barra di stato:** alias e consumi 5h/7d dell'account attivo, colore alle soglie, dettagli nel tooltip (motore in questa finestra, in un'altra finestra o istanza, fermo; cambio automatico acceso o spento; mappa attiva). Un clic apre la vista SwapDeck: il cambio di account resta nel comando «Cambia account» e dentro la vista.
- **Cambio rapido** (`SwapDeck: Cambia account`): account migliore (`best`), prossimo disponibile (`next-available`) o un account preciso.
- **Comandi:** `SwapDeck: Mostra SwapDeck`, `Apri in una scheda`, `Aggiorna`, `Cambia account`, `Cambia mappa`, `Avvia motore`, `Ferma motore`.
- **Conferme:** le azioni che lo richiedono (rimozione, import/export, swap/move, unmap, disattivazione dell'account attivo...) chiedono conferma con un dialogo modale di VS Code; `purge` chiede anche di digitare la parola. Il cambio di account non chiede conferma.
- **Notifiche:** quelle scelte nella pagina Notifiche, come notifiche di VS Code. Le manda solo la finestra che tiene il motore: una per evento, non una per finestra.
- **Avvio:** insieme a VS Code (`onStartupFinished`), senza altro da configurare.

## Motore

Ogni finestra avvia `swapdeck-cli headless` come processo figlio (`swapdeck.engine.enabled`, acceso di default): registra lo storico, manda le notifiche e, se il motore è acceso nella scheda Regole («Accendi»), cambia account secondo le regole salvate (valgono dal giro successivo, senza riavvio). `SwapDeck: Avvia motore` e `Ferma motore` fanno lo stesso di «Accendi» e «Motore acceso» in Regole: cambiano `engine.enabled` delle regole, condiviso da tutte le finestre. La prima accensione chiede conferma con un dialogo (Accendi / Solo osservazione / Annulla), una volta sola per l'app e tutte le finestre.

Un solo motore attivo per utente, grazie al lock: le altre finestre osservano (tooltip: «attivo in un'altra finestra o istanza», con il PID) e, quando la finestra titolare si chiude, una di loro lo prende entro un giro (`pollIntervalSeconds`). Le righe del motore sono nel canale di output «SwapDeck».

## Più finestre

Tutte le finestre aperte, anche di istanze diverse di VS Code, mostrano gli stessi dati entro pochi secondi senza ricaricare:

- ogni finestra osserva la cartella della config di SwapDeck: una modifica di `config.json` (regole, mappe, preferenze, notifiche) le fa rileggere tutte;
- dopo un cambio di account o un'altra azione di cswap, la finestra che l'ha fatta riscrive il segnale `vscode-sync.json` (solo `source` e `at`, in modo atomico) e le altre rileggono; lo stesso fa la finestra titolare a ogni notifica del motore;
- il polling (`refreshIntervalSeconds`) e la rilettura al ritorno del focus restano come rete di sicurezza;
- le schede aperte (Cartelle, Regole, Impostazioni, configurazione di cswap, Notifiche) si aggiornano da sole, anche nell'app desktop quando la modifica arriva da VS Code o dalla CLI; se stai modificando un modulo non salvato, resta com'è e compare «Modificato in un'altra finestra» con «Ricarica» (che scarta le tue modifiche);
- le scritture passano dalla CLI, che sotto un lock di file per utente (`.config.json.lock`, attesa massima 5 s) rilegge la config, cambia solo la parte toccata e la riscrive in modo atomico: due salvataggi simultanei da finestre diverse, dall'app o dalla CLI non si perdono;
- la barra di stato ha lo stesso testo e colore in tutte le finestre; cambia solo la riga del motore nel tooltip.

## Impostazioni

| Chiave                              | Default    | Significato                                                                                              |
| ----------------------------------- | ---------- | -------------------------------------------------------------------------------------------------------- |
| `swapdeck.refreshIntervalSeconds`   | 60         | lettura di `cswap list` ogni N secondi (minimo 15)                                                       |
| `swapdeck.cliPath`                  | vuoto      | percorso assoluto di `swapdeck-cli`; vuoto = quello incluso (impostabile solo a livello utente/macchina) |
| `swapdeck.engine.enabled`           | `true`     | esegue il motore (storico, notifiche, cambi se acceso in Regole) mentre VS Code è aperto                 |
| `swapdeck.notifications.thresholds` | `[80, 95]` | soglie dei colori della barra di stato; la più bassa avvisa, la più alta è critica                       |

Regole, mappe, preferenze e notifiche stanno nella config di SwapDeck (`swapdeck-cli config get|set`), condivisa con l'app desktop se c'è.

## Sviluppo

- `pnpm --filter swapdeck-vscode build`: bundle con esbuild in `out/`.
- `pnpm --filter swapdeck-vscode test`: vitest; `test:coverage` con soglia del 70% sulle righe (esclusi `extension.ts`, `panel.ts` e `webview/main.tsx`, provati da `test:ext`).
- `pnpm --filter swapdeck-vscode test:ext`: VS Code isolato (cartelle utente ed estensioni temporanee), finto cswap e cartella dati temporanea. Scarica VS Code in `.vscode-test/`.
- `pnpm --filter swapdeck-vscode package`: `.vsix` per `win32-x64` in `dist/`. Altri target: `node scripts/package.mjs --target <target> [--rust-target <triple> | --cli <binario>]`.
