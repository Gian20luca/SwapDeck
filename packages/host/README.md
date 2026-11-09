# @swapdeck/host

Parte comune agli host che mostrano il pannello di `@swapdeck/ui` dentro un IDE: estensione VS Code (`apps/vscode`) e plugin JetBrains (`apps/jetbrains`). Le due estensioni non si riferiscono mai l'una all'altra: usano solo `packages/*` e `swapdeck-cli` (controllo: `scripts/check-app-isolation.mjs`, in `pnpm lint`).

## Contenuto

| Percorso                                           | Cosa                                                                                                                                                                                                               |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `protocol.json`                                    | fonte unica di elenchi e limiti del protocollo, letta anche dal plugin Kotlin                                                                                                                                      |
| `src/protocol.ts` (`@swapdeck/host`)               | messaggi pannello ↔ host (`WebviewMessage`, `HostMessage`), `RequestParams`, `REQUEST_METHODS`, `isRequestMethod`, `viewLocale`, `suspensionOf`                                                                    |
| `src/rules.ts` (`@swapdeck/host`)                  | regole di validazione dal JSON: `ACTION_KINDS`, `SAFE_CLAUDE_ARGS`, `CONFIG_KEY`, `VERSION`, `MAX_RELOGIN_CODE_CHARS`, `MAX_COPY_CHARS`, `PICK_LIMITS`, `USAGE_WINDOWS`, `USAGE_GROUPS`, `DEFAULT_HISTORY_SECONDS` |
| `src/sync.ts` (`@swapdeck/host`)                   | segnale di sincronia: `SYNC_FILE`, `LEGACY_SYNC_FILES`, `CONFIG_FILE`, `SYNC_DEBOUNCE_MS`, `MAX_SIGNAL_BYTES`                                                                                                      |
| `src/MessageBackend.ts` (`@swapdeck/host/backend`) | `Backend` del pannello: ogni chiamata è un messaggio all'host, tramite le funzioni `post` e `subscribe` date da chi lo crea; `hostConfirms = true`                                                                 |

`MessageBackend` dipende da `@swapdeck/ui` e sta in un sottopercorso a parte, così il lato host (Node o JVM) non carica la UI.

## `protocol.json`

| Campo                         | Uso                                                                                                                                      |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `requestMethods`              | metodi che il pannello può chiedere; un altro nome è `invalidUsage`                                                                      |
| `actionKinds`                 | `kind` di `CswapAction` ammessi in `confirmationFor` e `runAction` (deve coincidere con l'unione del Rust: lo controlla `rules.test.ts`) |
| `safeClaudeArgs`              | argomenti di `claude` ammessi per l'azione `run`                                                                                         |
| `limits.configKeyPattern`     | chiave di `getConfigValue`                                                                                                               |
| `limits.versionPattern`       | versione di `dismissCswapUpdate`                                                                                                         |
| `limits.reloginCodeMaxLength` | codice di `submitReloginCode`, sempre su una riga sola (niente `\r` o `\n`)                                                              |
| `limits.copyTextMaxLength`    | testo di `copyText`                                                                                                                      |
| `limits.pick`                 | selettori: campi di testo ammessi e loro lunghezza, numero di filtri, lunghezza del nome, estensioni                                     |
| `limits.usage`                | `window` e `groupBy` di `getUsageBreakdown`                                                                                              |
| `defaults.historySeconds`     | periodo di `getHistory` senza `sinceSeconds`                                                                                             |
| `sync`                        | segnale di sincronia (sotto)                                                                                                             |

Lunghezze in unità UTF-16 (`String.length` in JavaScript e in Kotlin). I pattern sono espressioni regolari ancorate, valide sia in JavaScript sia in Java. Gli interi dei parametri (`account`, `sinceSeconds`, `fromSeconds`, `toSeconds`) sono interi sicuri non negativi; `startRelogin` vuole `account >= 1`.

## Segnale di sincronia `ide-sync.json`

Più host aperti sulla stessa cartella della config di SwapDeck (finestre e istanze di VS Code, IDE JetBrains, app desktop) si allineano così:

- ognuno osserva la cartella della config e reagisce solo a `config.json` e al segnale; gli eventi si raggruppano (`sync.debounceMs`, 750 ms) e fanno rileggere lo stato; il polling resta come rete di sicurezza;
- dopo una propria azione che non tocca la config (cambio di account, azioni di cswap, cambi del motore, installazione o aggiornamento) l'host riscrive il segnale `ide-sync.json` nella cartella della config;
- contenuto: `{"source": "<id opaco del processo>", "at": <ms dall'epoch>}`, nessun dato dell'utente. Un host ignora il segnale con il proprio `source`; un file più grande di `sync.maxSignalBytes` (4096 byte), non regolare o non leggibile vale come segnale altrui (si rilegge);
- scrittura atomica: file temporaneo `.ide-sync.json.<casuale>.tmp` nella stessa cartella, creato in modo esclusivo con permessi 0600, poi rinominato sopra il segnale (su Windows qualche tentativo se la rinomina dà `EPERM`, `EBUSY` o `EACCES`); in caso di errore il temporaneo si cancella e resta il polling. I link non si seguono;
- l'app desktop osserva lo stesso file (solo in lettura).

Compatibilità: fino alla 0.2.4 VS Code scriveva `vscode-sync.json`. Gli host lo osservano ancora (`sync.legacyFiles`), senza scriverlo, per una versione; nella seconda versione dopo la 0.2.4 si toglie da `legacyFiles` e da `config_watch.rs` dell'app.
