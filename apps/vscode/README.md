# SwapDeck per VS Code

Monitor e cambio degli account di `cswap` dentro VS Code.

## Installazione

1. Serve `cswap` installato per l'utente (lo stesso usato dal terminale).
2. Da terminale: `code --install-extension swapdeck-vscode-<piattaforma>-<versione>.vsix`, oppure in VS Code «Estensioni» → «...» → «Installa da VSIX...».
3. Le finestre già aperte attivano l'estensione al prossimo «Ricarica finestra» (o alla riapertura); quelle nuove subito.

Il .vsix è per piattaforma (contiene `bin/swapdeck-cli`) e non richiede privilegi di amministratore. L'estensione gira sempre sulla macchina locale (`extensionKind: ui`), anche con Remote/WSL.

## Funzioni

- **Barra di stato:** alias e consumi 5h/7d dell'account attivo, colore alle soglie, dettagli nel tooltip. Un clic apre il cambio rapido.
- **Cambio rapido:** account migliore (`best`), prossimo disponibile (`next-available`) o un account preciso.
- **Comandi:** `SwapDeck: Apri pannello`, `Aggiorna`, `Cambia account`, `Avvia motore`, `Ferma motore`.
- **Pannello:** l'app completa di SwapDeck a schede, come nel desktop: Account (azioni su ogni account), Cartelle (`map`/`unmap` con selettore di cartella), Regole del motore (mappe personali e preset in sola lettura «Principale con riserve», Eco, Sport e Sicura, da clonare), Storico (grafici, cambi e decisioni), Strumenti (export, import, `unclaimed`, `purge`) e Impostazioni (intervallo, soglie, `config set`). Lingua e tema seguono VS Code; quelli salvati in Impostazioni valgono per l'app desktop.
- **Conferme:** le azioni che lo richiedono (rimozione, import/export, swap/move, unmap, disattivazione dell'account attivo...) chiedono conferma con un dialogo modale di VS Code; `purge` chiede anche di digitare la parola. Il cambio di account non chiede conferma.
- **Notifiche:** al superamento delle soglie dell'account attivo e quando l'account attivo cambia.

## Motore

Facoltativo e **spento di default**. Con `swapdeck.engine.enabled` (o `SwapDeck: Avvia motore`) la finestra avvia `swapdeck-cli headless` come processo figlio, che applica le regole della config di SwapDeck e cambia account da solo (le regole salvate dal pannello valgono dal giro successivo, senza riavvio); si ferma con `Ferma motore` o alla chiusura della finestra.

Un solo motore alla volta per utente, grazie al lock: se gira già nell'app desktop o in un'altra finestra, questa resta in attesa senza avvisi (barra: «motore attivo in un'altra istanza») e lo riavvia da sé quando il lock si libera. Le righe del motore sono nel canale di output «SwapDeck».

## Impostazioni

| Chiave                              | Default    | Significato                                                                                              |
| ----------------------------------- | ---------- | -------------------------------------------------------------------------------------------------------- |
| `swapdeck.refreshIntervalSeconds`   | 60         | lettura di `cswap list` ogni N secondi (minimo 15)                                                       |
| `swapdeck.cliPath`                  | vuoto      | percorso assoluto di `swapdeck-cli`; vuoto = quello incluso (impostabile solo a livello utente/macchina) |
| `swapdeck.engine.enabled`           | `false`    | avvia il motore mentre VS Code è aperto                                                                  |
| `swapdeck.notifications.thresholds` | `[80, 95]` | soglie delle notifiche; la più bassa colora la barra come avviso, la più alta come critica               |

Le regole del motore stanno nella config di SwapDeck (`swapdeck-cli config get|set`).

## Sviluppo

- `pnpm --filter swapdeck-vscode build`: bundle con esbuild in `out/`.
- `pnpm --filter swapdeck-vscode test`: vitest; `test:coverage` con soglia del 70% sulle righe (esclusi `extension.ts`, `panel.ts` e `webview/main.tsx`, provati da `test:ext`).
- `pnpm --filter swapdeck-vscode test:ext`: VS Code isolato (cartelle utente ed estensioni temporanee), finto cswap e cartella dati temporanea. Scarica VS Code in `.vscode-test/`.
- `pnpm --filter swapdeck-vscode package`: `.vsix` per `win32-x64` in `dist/`. Altri target: `node scripts/package.mjs --target <target> [--rust-target <triple> | --cli <binario>]`.
