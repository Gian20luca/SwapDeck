# SwapDeck per VS Code

Monitor e cambio degli account di `cswap` dentro VS Code.

- **Barra di stato:** alias e consumi 5h/7d dell'account attivo, colore alle soglie, dettagli nel tooltip. Un clic apre il cambio rapido.
- **Cambio rapido:** account migliore (`best`), prossimo disponibile (`next-available`) o un account preciso.
- **Comandi:** `SwapDeck: Apri pannello`, `Aggiorna`, `Cambia account`, `Avvia motore`, `Ferma motore`.
- **Pannello:** la dashboard di SwapDeck (schede, ritmo, storico).
- **Notifiche:** al superamento delle soglie dell'account attivo e quando l'account attivo cambia.
- **Motore (facoltativo, spento di default):** `swapdeck-cli headless` come processo figlio, fermato alla chiusura. Rispetta il lock: se il motore gira già altrove (app desktop o un'altra finestra), resta in attesa.

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
- `pnpm --filter swapdeck-vscode test`: vitest.
- `pnpm --filter swapdeck-vscode test:ext`: VS Code isolato (cartelle utente ed estensioni temporanee), finto cswap e cartella dati temporanea. Scarica VS Code in `.vscode-test/`.
- `pnpm --filter swapdeck-vscode package`: `.vsix` per `win32-x64` in `dist/`. Altri target: `node scripts/package.mjs --target <target> [--rust-target <triple> | --cli <binario>]`.
