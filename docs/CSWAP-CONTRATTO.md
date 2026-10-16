# Contratto di `cswap` (claude-swap 0.26.0 – 0.27.2)

Fonte: sorgenti installati con `uv tool` (`claude_swap/cli.py`, `switcher.py`, `json_output.py`, `usage_store.py`, `poll_policy.py`, `autoswitch.py`, `settings.py`, `update_check.py`), più prove in sola lettura (0.26.0 e 0.27.2) sul profilo reale e prove d'errore su un profilo vuoto e temporaneo (15/10/2026, Windows 11). Le differenze della 0.27.2 rispetto alla 0.26.0 sono ricavate dal confronto dei sorgenti (`json_output.py`, `cli.py`, `transfer.py`, `switcher.py`) e segnate «0.27». Le fixture citate stanno in [`fixtures/cswap/`](../fixtures/cswap/README.md).

Per ogni voce: **[V]** = verificato eseguendo cswap, **[S]** = ricavato dai sorgenti.

## 1. Regole generali per chi invoca cswap

- **Mai `cswap` senza argomenti.** Se stdin e stdout sono un terminale apre la TUI. Su Windows il dispositivo `NUL` conta come terminale: `cswap <NUL >NUL` ha aperto la TUI ed è rimasto appeso fino al timeout [V]. Il client passa sempre un comando e usa **pipe** su stdin, stdout e stderr, mai `NUL` né l'ereditarietà della console.
- **Sempre `--json` dove esiste** (`list`, `status`, `switch`, `config list|get`, `auto`). In JSON cswap non chiede mai input e non fa effetti collaterali impliciti [S].
- **Processo:** su Windows `cswap.exe` è un trampolino uv che avvia `…\uv\tools\claude-swap\Scripts\python.exe "…\cswap.exe" <args>` e una `conhost.exe` [V]. Da una GUI va lanciato con `CREATE_NO_WINDOW`, altrimenti lampeggia una console. Su Linux `~/.local/bin/cswap` è uno script Python [S].
- **Tempo di avvio:** 2–4 s anche per `--version` [V]. Timeout consigliato 30 s per `list`/`status`/`switch` (vedi §4) e per i comandi che scrivono (attesa del lock fino a 10 s più l'avvio), 10 s per le altre letture; allo scadere si chiude l'albero del processo (trampolino e figlio `python.exe`). Dopo il timeout di un comando che scrive l'esito è incerto: SwapDeck rilegge `list --json` e restituisce l'errore `uncertain` con l'elenco riletto.
- **Codifica:** stdout e stderr sono forzati a UTF-8 con `errors="replace"` [S][V]. Decodificare in UTF-8 con sostituzione. In JSON i caratteri non ASCII arrivano come `\uXXXX`.
- **Colori:** disattivati quando stdout non è un terminale; `NO_COLOR` li spegne comunque [S]. Impostare `NO_COLOR=1` per sicurezza.
- **Ambiente:** non ereditare `CLAUDE_CONFIG_DIR` che punta dentro `<backup>/sessions/` (shell di `cswap run`): ogni comando che modifica fallisce con `SwitchError` «This shell is inside a cswap run session profile…» [S]. Non ereditare `CLAUDE_SECURESTORAGE_CONFIG_DIR`.
- **Avviso di aggiornamento:** dopo ogni comando non JSON (tranne `purge` e `upgrade`) cswap può scrivere su **stderr** `A newer version of claude-swap is available (X.Y.Z). You are using 0.27.2. Run … to update.` Interroga PyPI al massimo una volta ogni 24 h (cache `cache/update_check.json`, timeout 2 s) [S][V]. Non è un errore.

## 2. Codici di uscita

| Codice | Significato | Note |
|---|---|---|
| 0 | Riuscito | **Anche quando l'utente annulla un prompt** (`Cancelled` su stdout), `unmap` senza mappatura, `config unset` di una chiave non impostata [S] |
| 1 | Errore gestito (`ClaudeSwitchError`) | Testo: stderr `Error: <messaggio>`; in `--json`: busta d'errore su stdout [S][V] |
| 1 | Eccezione non gestita | Traceback Python su stderr. Caso tipico: `input()` con stdin chiuso o vuoto → `EOFError` (vedi §6) [S] |
| 2 | Uso errato (argparse) | stderr `usage: …` poi `cswap: error: <messaggio>` [V] |
| 130 | Interrotto (Ctrl-C) | In `--json` la nota va su stderr [S] |
| 0/1/2/3 | `auto --once` | 0 cambiato, 1 errore, 2 nessuna azione, 3 bloccato (nessuna destinazione valida) [S] |
| varia | `run` | Su Windows il codice di `claude`; su POSIX `exec` sostituisce il processo [S] |
| 1 | `upgrade` su Windows | Sempre: stampa solo il comando da eseguire (§9) [S] |

### Busta d'errore JSON (`list|status|switch --json`, `config --json`, `auto --json`)

```json
{"schemaVersion": 1, "error": {"type": "ConfigError", "message": "No accounts are managed yet"}}
```

`type` è il nome della classe: `ConfigError`, `ValidationError`, `AccountNotFoundError`, `SwitchError`, `SessionError`, `LockError`, `ClaudeCodeLockTimeout`, `CredentialReadError`, `CredentialWriteError`, `TransferError`, `MigrationError`, `MigrationIncomplete` [S]. Fixture: `error-*.json`. In `auto --json` la busta è su una riga sola.

### Messaggi riconoscibili (stderr `Error: …` o `error.message`)

| Messaggio esatto o regex | Classe | Quando |
|---|---|---|
| `No accounts are managed yet` | ConfigError | Nessun `sequence.json` o nessun account [V] |
| `^No account found with identifier: (.+)$` | AccountNotFoundError | Numero, email o alias sconosciuti [V] |
| `^Account-(\d+) does not exist$` | AccountNotFoundError | `alias`, `swap`, `move`, `map`, `disable` su slot vuoto [V] |
| `^Invalid account identifier: (.+)$` | ValidationError | Né numero, né email, né alias [V] |
| `No active Claude account found. Please log in first.` | ConfigError | `add` senza login in Claude Code [V] |
| `Token cannot be empty` | ValidationError | `add-token -` con stdin vuoto [V] |
| `^Invalid email format: (.+)$` | ValidationError | `add-token --email` non valido [S] |
| `^Alias '(.+)' is already used by account (\d+)$` | ConfigError/ValidationError | Alias duplicato [S] |
| `Cannot swap an account with itself` | ValidationError | `swap a a` [S] |
| `Slot number must be >= 1` | ConfigError | `--slot 0` [S] |
| `^unknown setting '(.+)'` | ConfigError | `config get|set|unset` con chiave errata [V] |
| `^no unclaimed entry (.+)$` | — (stampa diretta) | `unclaimed --purge` con id inesistente, uscita 1 [V] |
| `no accounts to export — run cswap --add-account first` | TransferError | `export` senza account [V] |
| `Failed to acquire lock - another instance may be running` | LockError | Lock `<backup>/.lock` occupato oltre 10 s [S] |
| `^Could not acquire .+ — Claude Code appears to be refreshing credentials\. Retry in a few seconds\.$` | ClaudeCodeLockTimeout | Lock di Claude Code oltre 9 s: **ritentare** [S] |
| `has a live session-mode Claude instance \(PID [\d, ]+\)` | SessionError | `remove`, `purge` con una sessione `cswap run` viva [S] |
| `This shell is inside a cswap run session profile` | SwitchError | `CLAUDE_CONFIG_DIR` dentro un profilo di sessione [S] |
| `No accounts remain in rotation. Re-enable one with: cswap enable <num\|email>` | ConfigError | `switch` senza login e tutti disabilitati [S] |
| `^cswap(?: \w+)?: error: (.+)$` (stderr, uscita 2) | argparse | Opzioni incompatibili, es. `--json can only be used with 'list', 'status', or 'switch'`, `--model can only be used with 'switch --strategy best' or 'switch --strategy next-available'`, `--token-status cannot be combined with --json`, `no command given — try 'cswap help'` [V] |

## 3. Comandi

Identificatore `<id>` = numero di slot, email o alias (gli alias valgono ovunque). Un'email condivisa da più slot (stessa email in organizzazioni diverse) fa partire un **prompt** in modalità testo; in `--json` dà invece un `ConfigError` che elenca gli slot [S].

### 3.1 `list` — `cswap list --json` [V]

Schema (fixture `list.json`, `list-*.json`):

| Campo | Tipo | Note |
|---|---|---|
| `schemaVersion` | int | `1`. Cambia solo con modifiche incompatibili |
| `activeAccountNumber` | int \| null | `null` se nessun account attivo gestito |
| `accounts[]` | array | Vuoto se non c'è `sequence.json` (mai prompt in JSON) |
| `accounts[].number` | int | Slot; i numeri possono avere buchi (1, 2, 3, 5, 6) |
| `.email` | string | Per `add-token` senza email: `setup-token-N@token.local` o `api-key-N@token.local` |
| `.organizationName`, `.organizationUuid` | string | `""` per gli account personali |
| `.isOrganization` | bool | `organizationUuid != ""` |
| `.active` | bool | |
| `.usageStatus` | string | `ok`, `unavailable`, `token_expired`, `relogin_required`, `api_key`, `keychain_unavailable`, `foreign_credential`, `no_credentials` |
| `.usage` | object \| null | Non null solo con `ok` |
| `.alias` | string | **Assente** se non impostato |
| `.disabled` | `true` | **Assente** se abilitato |
| `.loginExpiresAt` | string ISO `Z` | 0.27 [V]. Scadenza del refresh token (`refreshTokenExpiresAt`): dopo serve un nuovo `/login`. **Assente** se il login non la registra (= sconosciuta, non «adesso»). Può essere futura anche con `relogin_required` |
| `.usageError`, `.usageRetryAt` (ISO `Z`) | string | 0.27 [S]. Solo con `usageStatus: "unavailable"`: causa dell'ultimo fetch fallito (`http-429`, `timeout`, `network`, `http-NNN`) e, durante il backoff, quando riprova |
| `.usageFetchedAt` (ISO `Z`), `.usageAgeSeconds` (float) | | Solo con `usage` non null |
| `.lastGoodUsage`, `.lastGoodFetchedAt`, `.lastGoodAgeSeconds` | | Solo con `usage` null e una misura precedente: dati **da mostrare**, non da usare per decidere |
| `duplicateAccountWarnings`, `lockstepUsageWarnings` | string[] | Assenti se vuoti |
| `unclaimedCredentials` | string[] | Id delle credenziali parcheggiate; assente se vuoto |

`usage` (e `lastGoodUsage`): ogni sotto-chiave è facoltativa.
- `fiveHour`: `{pct, resetsAt?, countdown?, clock?}`. `resetsAt` manca quando la finestra è a 0% [V].
- `sevenDay`: come sopra più i campi di ritmo, quando calcolabili: `expectedPct` (float, 1 decimale), `aheadOfPace` (bool), `projectedExhaustionAt` (ISO `Z`, solo se si esaurisce prima del reset), `willLastToReset` (bool).
- `scoped[]`: finestre settimanali per modello, con gli stessi campi di `sevenDay` più `name` (es. `"Fable"`).
- `spend`: `{used, limit, pct, currency, resetsAt?, countdown?, clock?}` per il pay-as-you-go.
- `pct` è float 0–100. `resetsAt` è ISO con offset `+00:00` e microsecondi. `countdown` (`"3d 21h"`, `"1h 57m"`, `"0m"`) e `clock` (`"23:29"`, `"Oct 19 18:59"`, ora locale, in inglese) sono stringhe di comodo: il client ricalcola da `resetsAt`.

Semantica di `usageStatus`:
- `ok`: dato utilizzabile per decidere. Normalmente ha al massimo 300 s, ma dopo errori o un 429 può essere più vecchio (fiducia estesa: fino a 3600 s, dopo un 429 fino al reset della finestra). Leggere sempre `usageAgeSeconds` [S].
- `unavailable`: fetch fallito o dato troppo vecchio; può esserci `lastGoodUsage`.
- `token_expired`: token attivo scaduto e rinnovo rimandato; si risolve da solo.
- `relogin_required`: refresh token morto; serve `/login` in Claude Code e poi `cswap add` [V].
- `api_key`: account a chiave API, senza quote.
- `foreign_credential`: la credenziale attiva appartiene a un altro account; uno `switch` la ripara.

Non esiste uno stato «rate limited» in JSON: un 429 si vede come `ok` vecchio oppure `unavailable` con `lastGoodUsage` (fixture `list-rate-limited.json`). Fino alla 0.26 la causa (`http-429`, `timeout`, `network`, `http-NNN`) appariva solo negli eventi `poll` di `auto --json` (`fetchErrors`) e nel log; dalla 0.27 le righe `unavailable` la portano in `usageError` [S].

### 3.2 `status` — `cswap status --json` [V]

Tre forme (fixture `status.json`, `status-no-active.json`, `status-unmanaged.json`):
- nessun login: `{"schemaVersion":1,"active":null}`;
- login non gestito: `active = {email, managed:false}`;
- gestito: `active` ha i campi di una riga di `list` (senza `active`, `disabled` e `loginExpiresAt`) più `managed:true`; c'è anche `totalManagedAccounts` (int). Dalla 0.27 anche `usageError`/`usageRetryAt` come in `list` [S].

Recupera i consumi solo dell'account attivo [S].

### 3.3 `switch`

| Forma | Effetto |
|---|---|
| `switch --json` | Rotazione semplice al prossimo slot abilitato (`strategy:"rotation"`) |
| `switch <id> --json` | Diretto (`strategy:"direct"`). Dai sorgenti **non** controlla `disabled` |
| `switch <id> --force --json` | Riscrive il login dal backup senza salvare quello corrente; sullo stesso slot `reason:"activated"` |
| `switch --strategy best [--model N] --json` | Account con più margine; resta fermo se non è dimostrabile un margine migliore |
| `switch --strategy next-available [--model N] --json` | Rotazione che salta gli account al limite |

`--model` accetta nomi separati da virgola o `all`; solo con `--strategy`. Senza `--model` vale `autoswitch.model` di `config` [S].

Risultato (fixture `switch-*.json`): `schemaVersion`, `switched` (bool), `from` e `to` (`{number:int|null, email}` oppure `to:null` in `only-one-account` senza slot), `strategy` (`rotation|direct|best|next-available`), `reason`, `message`, `warnings[]`, più `models[]` e `modelSource` (`cli|autoswitch.model`) solo se sono stati usati modelli.

`reason`: `switched`, `already-active`, `activated`, `unmanaged-account`, `only-one-account`, `usage-unavailable`, `already-best`, `candidates-exhausted`, `no-valid-target`. **Uscita 0 anche quando `switched` è false**: decide il campo, non il codice. Avvisi tipici: `Skipped Account-N (disabled)`, `Skipped Account-N (no stored credentials/config)`, `Skipped Account-N (at 5h/7d limit)` [S].

**Da evitare senza `--json`:** su un login non gestito `switch` lo **aggiunge da solo** (`add`) e non cambia account [S].

Il cambio vale per tutte le sessioni di Claude Code della macchina (riscrive `~/.claude/.credentials.json` e `oauthAccount` di `~/.claude.json`); usa il lock `<backup>/.lock` (10 s) e i lock di Claude Code (9 s) [S].

### 3.4 `add` — richiede il login in Claude Code [S][V]

`cswap add [--slot N] [--alias NAME]`. Cattura l'account **attualmente loggato in Claude Code** (`~/.claude/.credentials.json` e `~/.claude.json`): senza login esce con `No active Claude account found. Please log in first.` (uscita 1). Flusso GUI: l'utente fa `/login` in Claude Code, poi SwapDeck lancia `cswap add`.
- Senza `--slot` e account già presente: aggiorna le credenziali sul posto (utile dopo `relogin_required`).
- Con `--slot N` occupato da un altro account: stampa `Slot N already occupied` e chiede `Overwrite slot N? [y/N] ` (vedi §6).
- Rifiuta una credenziale che non corrisponde all'identità (protezione da cattura «estranea») con `ConfigError` [S].
- Nota [V]: `add` crea `sequence.json` vuoto **prima** di verificare il login.

### 3.5 `add-token` [S][V]

`cswap add-token - [--email E] [--slot N]`. Con `-` legge **una riga** da stdin (fino a `\n`, spazi tolti). Il token **non va mai in argv** (visibile nell'elenco dei processi). Senza argomento usa `getpass` (prompt `Token: ` sul terminale): da evitare. Riconosce `sk-ant-api…` come chiave API, il resto come setup-token OAuth. Non fa chiamate di rete. Email di default `setup-token-N@token.local` / `api-key-N@token.local`. Slot occupato: stesso prompt di `add`, che legge dallo **stesso stdin** dopo la riga del token.

### 3.6 `remove`, `disable`, `enable`

- `remove <id>`: chiede **sempre** `Are you sure you want to permanently remove Account-N (<email>)? [y/N] `. Solo `y` (senza distinzione maiuscole) conferma. Rifiuta se c'è una sessione `cswap run` viva [S].
- `disable <id>` / `enable <id>`: nessun prompt. Disabilitare l'account attivo lo lascia attivo fino al prossimo cambio. Già nello stato richiesto: `Account-N (<email>) is already disabled.` con uscita 0 [S].

### 3.7 `alias`, `swap`, `move`, `map`, `unmap`, `unclaimed`

| Comando | Effetto | Output |
|---|---|---|
| `alias` | Elenca | `Aliases:` poi `  N: alias (email)`, oppure `No aliases set` [V] |
| `alias <id> <name>` / `alias <id> --unset` | Imposta / toglie (lettere, cifre, `.`, `-`, `_`, non solo cifre) | `Set alias 'x' for Account N` / `Removed alias for Account N` |
| `swap <a> <b>` | Scambia i numeri di slot | Nessun prompt |
| `move <id> <slot>` | Sposta (scambia se occupato) | Nessun prompt |
| `map` | Elenca | `No directory mappings yet.` [V] |
| `map <id> [path]` | Mappa una cartella (default: cwd) | `Mapped <path> → Account-N (<email>)` |
| `unmap [path]` | Toglie la mappatura | `Unmapped <path>` o `No mapping for <path>` (uscita 0) [V] |
| `unclaimed` | Elenca credenziali parcheggiate | `<id>  slot <n>  <motivo>` o `No unclaimed credential entries` [V] |
| `unclaimed --purge <id>` | Cancella una voce | `Purged <id>`; id ignoto → uscita 1 [V] |

Nessuno ha `--json`: il client rilegge `list --json` dopo l'azione.

### 3.8 `config` [V]

`cswap config [list|get KEY|set KEY VALUE|unset KEY|path] [--json]`; `--json` solo con `list` e `get` (altrimenti uscita 2).
- `config --json`: `{schemaVersion:1, path, settings:[{key, value, isSet}]}` (fixture `config-list.json`).
- `config get K --json`: `{schemaVersion:1, key, value, isSet}`.
- `set` stampa `K = V`; `unset` stampa `K unset (default: V)`.

| Chiave | Tipo | Valori | Default |
|---|---|---|---|
| `autoswitch.threshold` | float | 50–99.9 | 90 |
| `autoswitch.intervalSeconds` | float | 15–3600 | 60 |
| `autoswitch.cooldownSeconds` | float | 0–86400 | 300 |
| `autoswitch.hysteresisPct` | float | 0–50 | 10 |
| `autoswitch.strategy` | choice | `best`, `consume-first` | `best` |
| `autoswitch.includeApiKeyAccounts` | bool | `true/false`, `yes/no`, `1/0` | false |
| `autoswitch.unhealthyTicks` | int | 1–100 | 3 |
| `autoswitch.model` | string | nomi separati da virgola o `all` | null (`(none)`) |
| `ui.theme` | choice | `dark`, `light`, `auto` | `auto` |

`set` valida in modo rigido (errore se fuori intervallo); il caricamento invece riporta nei limiti i valori scritti a mano [S].

### 3.9 `auto` [S]

`cswap auto [--once] [--json] [--interval S] [--threshold P] [--cooldown S] [--model N] [--[no-]include-api-key-accounts] [--strategy best|consume-first] [--dry-run]`. Le opzioni sovrascrivono `config`.
- Senza `--once`: ciclo in primo piano; si ferma con SIGTERM o Ctrl-C (130). SwapDeck lo chiude dopo l'evento `sleep` (pausa tra due giri, al massimo 20 s di attesa) per non interrompere un cambio; poi SIGTERM e SIGKILL su Unix, `taskkill /T /F` su Windows (un processo senza console non riceve CTRL_BREAK). Con `--once` le uscite 2 e 3 non sono errori, salvo `usage:` o `: error:` su stderr (argparse esce anch'esso con 2).
- `--json`: una riga JSON per evento (fixture `auto-events.jsonl`). Campi comuni `schemaVersion`, `event`, `ts`. Eventi: `poll` (`active`, `headroomPct{num:pct|null}`, `threshold`, `fetchErrors?{num:causa}`, `windowsPct?`), `switch` (`trigger`: `proactive|at-limit|failover|consume-first`, `from`, `to`, `warnings`, `dryRun`), `no-switch` (`reason`, `detail`), `account-quarantined` / `account-unquarantined` (`number`, `email`, `reason`), `all-exhausted` (`earliestResetAt|null`), `sleep` (`seconds`, `until`), `error` (`message`, `transient`), `config-warning` (`message`). Eventi e campi sconosciuti vanno ignorati.
- Motivi di `no-switch`: `unmanaged-active-account`, `no-active-account`, `active-api-key`, `below-threshold`, `active-idle`, `active-usage-unknown`, `cooldown`, `no-candidates`, `no-comparison`, `reset-unknown`, `already-consuming-soonest`, `no-qualifying-candidate`, `stale-usage`, `no-viable-target`.

**Rilevare `cswap auto` in esecuzione:** cswap non ha né file PID né battito. `autoswitch_state.json` (con il lock `.autoswitch_state.lock`) si scrive solo su quarantene e cooldown, non a ogni giro. L'unico metodo è l'elenco dei processi [S][V]:
- Windows: processi `cswap.exe`/`claude-swap.exe`, o `python.exe` con il percorso di `cswap.exe` in argv, con argomento `auto`.
- Linux: `/proc/*/cmdline` con un elemento di nome `cswap` o `claude-swap` seguito da `auto`.
- Lo stesso motore gira anche dentro `cswap tui`, `cswap watch` (pagina auto) e `cswap menubar` (macOS): un processo `tui`, `watch`, `menubar` o `cswap` senza argomenti va trattato come **possibile** conflitto (avviso, non blocco certo).
- `auto --once` lanciato da un'attività pianificata dura pochi secondi: va cercato anche tra le attività (`schtasks` / `crontab -l`), come da piano.

### 3.10 `export` / `import` [S]

- `export <path|-> [--account <id>] [--full]`: JSON con le **credenziali in chiaro**; il file viene creato con `0600` (su Windows non cambia le ACL). Con `-` scrive su stdout. Senza account: `TransferError` (uscita 1).
- `import <path|-> [--force]`: valida tutto prima di scrivere; senza `--force` salta gli account già presenti, con `--force` li sovrascrive. `-` legge tutto stdin. Nessun prompt.

### 3.11 `run` [S]

`cswap run [<id>] [--no-share] [--share-history] [--require-session] [-- <argomenti di claude>]`. Sperimentale. Avvia Claude Code come l'account indicato **solo in quel terminale**, con un profilo in `<backup>/sessions/`. Senza `<id>` usa la mappatura della cartella corrente. Su Windows lancia `claude` come sottoprocesso e ne restituisce il codice; `--share-history` non è supportato su Windows. Per la GUI: aprire un **nuovo terminale** che esegue `cswap run <id>`.

### 3.12 `purge` [S]

Stampa l'elenco di ciò che rimuove (cartella di backup, credenziali, profili di sessione), poi chiede `Are you sure you want to purge all data? [y/N] `. Rifiuta se ci sono sessioni `cswap run` vive o illeggibili. Non tocca il login corrente di Claude Code.

### 3.13 `--version`, `--token-status`

- `cswap --version` → `cswap 0.27.2` (il prefisso è il nome con cui è stato invocato). Regex: `^\S+ (\d+)\.(\d+)\.(\d+)` [V].
- `--token-status` vale solo con `list` in testo; con `--json` dà uscita 2 [V]. Righe `• <fonte>: <fresh|expired>, refresh token <yes|no>, expires <clock> in <countdown>` (fonti: `active profile`, `session profile`, `stored backup`). Non contiene token (fixture `list-token-status.txt`).

### 3.14 `import-usage` — 0.27 [S]

`cswap import-usage <path|-> [--hold S]`. Adotta le letture dei consumi fatte da un'altra macchina (il suo `list --json`): le righe `ok` sono abbinate agli slot locali per (email, `organizationUuid`); le altre e gli account non gestiti si saltano. `--hold S` impedisce i fetch locali di quegli account per S secondi (0 toglie un blocco precedente); `--hold` senza `import-usage` o negativo → uscita 2. Esito su stderr (`Adopted usage for … → slot N`); file mancante o `schemaVersion` diverso → `TransferError` (`usage file not found: …`), uscita 1 [V]. Non fa parte di F0: SwapDeck non lo usa.

## 4. Consumi: cadenza, cache e rete [S]

- Ogni `list`, `status` e `switch --strategy` passa da un archivio condiviso (`<backup>/cache/usage.json` con `.usage.lock`). **Una `list` non chiama sempre la rete:** recupera solo gli account il cui dato è più vecchio di 180 s **e** il cui piano di interrogazione lo consente (`next_poll_at` salvato). Più processi (GUI, estensione, TUI, `auto`) si coordinano con prenotazioni atomiche: mai due fetch in parallelo per lo stesso slot.
- Cadenza dei piani: minima 180 s; 60 s solo per l'account attivo vicino alla soglia e in movimento; inattivi fino a 300–600 s; esauriti 600 s; dopo un 429 almeno 360 s, con backoff moltiplicativo. Il budget del server è di circa 28–30 richieste all'ora per identità, su una finestra mobile di un'ora: una raffica lo satura per un'ora intera.
- Errori senza `Retry-After`: backoff 30 s · 2^(n−1), massimo 600 s.
- `GET https://api.anthropic.com/api/oauth/usage` con timeout 5 s, richieste in parallelo (thread); rinnovo token `POST https://platform.claude.com/v1/oauth/token` con timeout 10 s. Il proxy segue le variabili standard (`HTTPS_PROXY`, `NO_PROXY`) e su Windows le impostazioni di sistema (urllib); TLS con il verificatore del sistema operativo (`truststore`).
- **Conseguenza per SwapDeck:** `list --json` ogni 60 s è sicuro e non aumenta le chiamate di rete. È però **inutile scendere sotto 60 s** (il dato cambia al massimo ogni 180 s, salvo modalità urgente) e va evitato lanciare più `list` in parallelo (2–4 s di CPU ciascuna). Durata misurata: 2–4 s, fetch compresi. Nel caso peggiore (rinnovo token da 10 s più fetch da 5 s) la stima è di 15–20 s.
- Per lo storico (F1) si registrano solo le misure con `usageFetchedAt` nuovo, per non duplicare punti.

## 5. Dove cswap tiene i dati [S][V]

- Cartella di backup: Windows e macOS `~/.claude-swap-backup`; Linux/WSL `$XDG_DATA_HOME/claude-swap` (default `~/.local/share/claude-swap`), con migrazione automatica dalla vecchia cartella.
- Contenuto: `sequence.json` (account, ordine, `activeAccountNumber`, alias, `disabled`), `configs/` (copie di `oauthAccount`), `credentials/` (**base64, non cifrate**, permessi 0600 su POSIX; su macOS anche nel Portachiavi), `cache/usage.json`, `cache/update_check.json`, `settings.json` (solo se è stato fatto un `config set`), `autoswitch_state.json`, `sessions/`, `claude-swap.log`, `.lock`, `.migrations.json`.
- Login live di Claude Code: `~/.claude/.credentials.json` (o `$CLAUDE_CONFIG_DIR`) e `~/.claude.json`.
- **SwapDeck non legge né copia mai `credentials/`, `configs/` e i file di Claude Code**: usa solo l'uscita dei comandi. L'unica eccezione possibile, in sola lettura, è il nome dei file `.lock` per diagnosi.

## 6. Comandi interattivi e come pilotarli da un processo [S]

| Comando | Prompt (su stdout, senza a capo) | Risposta che conferma | Con stdin vuoto o chiuso |
|---|---|---|---|
| `remove <id>` | `Are you sure you want to permanently remove Account-N (<email>)? [y/N] ` | `y\n` | `EOFError` → traceback, uscita 1 |
| `purge` | `Are you sure you want to purge all data? [y/N] ` | `y\n` | `EOFError` → traceback, uscita 1 |
| `add --slot N`, `add-token --slot N` (slot occupato) | `Overwrite slot N? [y/N] ` | `y\n` o `yes\n` | `Cancelled`, uscita 0 |
| `remove`, `switch` (testo) con email ambigua | `Enter account number to remove: ` / `… to switch to: ` | numero + `\n` | `EOFError` → traceback |
| `list` (testo) senza `sequence.json` e con login attivo | `No managed accounts found. Add current account (<email>) to managed list? [Y/n] ` | **default Sì**: una riga vuota aggiunge l'account | `EOFError` |
| `add-token` senza argomento | `Token: ` (getpass, terminale) | — | da non usare: usare `-` |

Regole per il client:
1. La conferma la chiede SwapDeck nella propria interfaccia, poi scrive la risposta su stdin **prima** di chiuderlo (`y\n`). Non serve leggere il prompt.
2. Per `add`/`add-token` senza conferma dell'utente si scrive `n\n`: slot occupato → `Cancelled`, uscita 0. L'esito si verifica sempre rileggendo `list --json`, perché l'uscita 0 non distingue riuscito da annullato. In alternativa si cerca `^Cancelled$` su stdout.
3. Gli identificatori passati sono sempre numeri di slot (mai email), così il prompt di ambiguità non scatta.
4. `list`, `status`, `switch` sempre con `--json`.

## 7. Versione minima supportata

- **Minima 0.26.0**; provate 0.26.0 e 0.27.2. Il client legge `--version` all'avvio. Sotto la 0.26.0: versione non supportata (avviso, azioni bloccate). Con una maggiore e `schemaVersion` = 1: procede, ignorando i campi sconosciuti (lo schema è dichiarato additivo). Con `schemaVersion` ≠ 1: si ferma e avvisa.
- 0.27.2 (15/10/2026): solo aggiunte (`loginExpiresAt`, `usageError`, `usageRetryAt`, comando `import-usage`); `schemaVersion` resta 1.
- Requisiti: Python ≥ 3.12 (lo fornisce uv); dipendenze `textual`, `truststore`, `keyring` (solo Windows).

## 8. Installazione

- `uv tool install claude-swap` installa `cswap` e `claude-swap` in `~/.local/bin` (Windows `%USERPROFILE%\.local\bin\cswap.exe`) e l'ambiente in `uv tool dir` (Windows `%APPDATA%\uv\tools\claude-swap`). Nessun privilegio di amministratore.
- Rilevamento in SwapDeck: `SWAPDECK_CSWAP` (assoluto), poi `UV_TOOL_BIN_DIR`, poi il PATH (solo voci assolute), poi `$XDG_BIN_HOME` e `~/.local/bin/cswap[.exe]`; `uv` nel PATH, in `~/.local/bin`, in `~/.cargo/bin` e su macOS in `/opt/homebrew/bin` e `/usr/local/bin`. In alternativa `uv tool list` (riga `claude-swap vX.Y.Z`).
- Installazione da SwapDeck (`Setup::install`, step 6): se uv manca, scarica lo script ufficiale (`https://astral.sh/uv/install.ps1` o `install.sh`, al massimo 2 MB) con il client interno (solo HTTPS anche nei redirect); se non riesce ripiega su PowerShell `Invoke-WebRequest` con controllo di schema HTTPS e dominio finale, oppure su `curl --proto =https --proto-redir =https --max-filesize 2097152` e poi `wget --https-only`. Lo script si salva in una cartella privata dentro la cartella dati, se ne controlla la dimensione e lo si esegue da file (`powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File`, oppure `/bin/sh`). Poi `uv tool install claude-swap` e la verifica (`--version`, `list --json`).
- Con `SWAPDECK_CSWAP` impostata installazione e aggiornamento sono rifiutati (`invalidUsage`): toccherebbero il claude-swap reale del profilo, non quello indicato.

## 9. Aggiornamento

- `cswap upgrade` (alias `update`): rileva uv o pipx da `sys.prefix`. Su Linux e macOS esegue `uv tool upgrade claude-swap` e ne restituisce il codice. **Su Windows non aggiorna**: stampa `To upgrade claude-swap on Windows, run:\n  uv tool upgrade claude-swap` ed esce con 1, perché `cswap.exe` in esecuzione è bloccato.
- SwapDeck quindi lancia direttamente `uv tool upgrade claude-swap`, **dopo** aver atteso o chiuso i propri processi cswap. Se gira un `cswap auto`, `tui` o `watch` esterno, avvisa: su Windows l'aggiornamento può fallire. Un'uscita non riuscita di uv (che usa 2 per quasi tutti gli errori) è `failed`, oppure `busy` se un file è in uso (`os error 32`, «used by another process»).
- Prima di aggiornare (`Setup::upgrade`): se il lock del motore (`engine.lock`) è vivo e di un altro processo, rifiuta con `busy` («ferma il motore in quella SwapDeck»). Nello stesso processo l'app ferma `cswap auto` e tiene il turno delle chiamate, così ciclo di lettura e motore restano fermi.
- Per `upgrade` sono `busy` anche `os error 5`, «access is denied» e «accesso negato» (su Windows `cswap.exe` in esecuzione).
- Dopo l'aggiornamento si verificano versione e `schemaVersion`. Se lo `schemaVersion` non è supportato, SwapDeck torna da sola alla versione precedente con `uv tool install claude-swap==<precedente> --force` (passo `rollbackCswap`) e restituisce comunque `unsupportedSchema`, con l'esito del ripristino nel messaggio.
- Controllo della versione su PyPI (`https://pypi.org/pypi/claude-swap/json`, `info.version`): cache di 6 ore in `cswap-update.json` della cartella dati, riletta prima di ogni scrittura. Nel confronto una pre-release (`a`, `b`, `rc`, `dev`) è più vecchia della stabile con lo stesso numero; post-release (`.post1`) e versioni locali (`+x`) valgono come la stabile.

## 10. Rischi per SwapDeck

1. **TUI accidentale:** `cswap` senza comando, o con stdio su `NUL` su Windows, apre la TUI e resta appeso. Sempre un comando, sempre pipe, sempre timeout con chiusura dell'albero dei processi.
2. **Uscita 0 non significa riuscito:** i prompt annullati, `switched:false`, `unmap` senza mappatura escono con 0. Il client verifica l'effetto rileggendo `list --json`.
3. **Prompt con stdin chiuso = traceback** (`remove`, `purge`, ambiguità). Il client scrive sempre la risposta su stdin; un traceback (`Traceback (most recent call last)` su stderr) è un errore interno, non un rifiuto dell'utente.
4. **Effetti impliciti fuori da JSON:** `switch` in testo aggiunge un login non gestito; `list` in testo può chiedere di aggiungerlo (default Sì). Il client non usa mai la modalità testo per `list` e `switch`.
5. **Cambio globale:** ogni `switch` vale per tutte le sessioni di Claude Code della macchina e corre con `cswap auto`, `cswap-guard` o altri motori: rilevamento conflitti (§3.9) e lock di SwapDeck prima di ogni cambio automatico.
6. **Dati vecchi presentati come `ok`:** dopo un 429 `ok` può avere ore di età. Le soglie del motore usano `usageAgeSeconds`; l'interfaccia mostra l'età.
7. **Budget di rete condiviso** (~28–30 richieste/ora per identità tra tutte le macchine): non forzare refresh; nessuna opzione di cswap li forza, e non va cercato un modo per aggirare la cache.
8. **Credenziali in chiaro (base64)** nella cartella di backup e negli export: SwapDeck non le legge, non le copia e non le registra. Per `export` chiede conferma e propone un percorso nel profilo utente; per `add-token` usa solo stdin.
9. **Costo di avvio** 2–4 s per chiamata e una `conhost.exe` su Windows: niente chiamate in parallelo, `CREATE_NO_WINDOW`, coda delle azioni.
10. **Stringhe in inglese e non stabili:** messaggi, `countdown` e `clock` possono cambiare tra versioni. Ci si basa su `schemaVersion`, `error.type`, `reason` e `usageStatus`; le regex del §2 servono solo per i comandi senza JSON.
11. **Aggiornamento su Windows** (`cswap upgrade` esce con 1 senza aggiornare): usare `uv tool upgrade claude-swap` con cswap fermo.
12. **Comportamento non provato dal vivo** (vietato sul profilo reale): formato reale di `switch --json` riuscito, prompt di `remove`/`purge`, `auto --json`, 429 reali. Le fixture costruite a mano vanno confermate su una VM o su un profilo di prova con account usa e getta.
