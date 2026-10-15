# Fixture di `cswap` 0.26.0

Campioni per i test del client (`swapdeck-core`, estensione VS Code). Il contratto completo è in [`docs/CSWAP-CONTRATTO.md`](../../docs/CSWAP-CONTRATTO.md).

Anonimizzazione: email → `userN@example.com` (N = numero di slot), UUID → `00000000-0000-4000-8000-00000000000N`, nomi di organizzazione → `Example Org N`, alias → `aliasN`, percorsi utente → `C:\Users\user`, cartelle dei progetti → `c:\work\project-a`, `~/work/project-b`. Nessun token né credenziale.

## Reali (profilo reale, Windows, 15/10/2026)

| File | Comando |
|---|---|
| `list.json` | `cswap list --json` (5 account; `ok` e `relogin_required` con `lastGoodUsage`; finestre `scoped` «Fable») |
| `status.json` | `cswap status --json` |
| `config-list.json` | `cswap config --json` (tutti i valori di default) |
| `list.txt`, `list-token-status.txt` | `cswap list`, `cswap list --token-status` (testo: `--token-status` non esiste in JSON) |
| `status.txt`, `alias.txt`, `map-empty.txt`, `config-list.txt`, `unclaimed-empty.txt`, `version.txt` | `status`, `alias`, `map`, `config`, `unclaimed`, `--version` |

## Reali (profilo vuoto e temporaneo)

| File | Comando | Uscita |
|---|---|---|
| `list-no-accounts.json` | `list --json` | 0 |
| `status-no-active.json` | `status --json` | 0 |
| `config-get.json` | `config get autoswitch.threshold --json` | 0 |
| `error-no-accounts.json` | `switch --json`, `switch 3 --json`, `switch --strategy best --json` | 1 |
| `error-account-not-found.json` | `switch nobody@example.com --json` | 1 |
| `error-invalid-identifier.json` | `switch "bad id!" --json` | 1 |

## Costruite a mano dai sorgenti (`json_output.py`, `switcher.py`, `autoswitch.py`)

| File | Caso |
|---|---|
| `switch-switched.json` | `switch --strategy next-available --json` riuscito, con avvisi di account saltati |
| `switch-direct-already-active.json` | `switch 6 --json` sull'account già attivo (`strategy: direct`) |
| `switch-best-usage-unavailable.json` | `switch --strategy best --model Fable --json` senza consumi (es. dopo un 429): nessun cambio |
| `switch-candidates-exhausted.json` | tutte le riserve al limite |
| `switch-unmanaged.json` | login attivo non gestito da cswap (`number: null`) |
| `status-unmanaged.json` | `status --json` con login non gestito |
| `list-disabled.json` | account disabilitato (`disabled: true`), account personale (organizzazione vuota) |
| `list-usage-stale.json` | consumi vecchi (`unavailable` + `lastGoodUsage`), mai letti (`unavailable` senza `lastGood*`), `api_key`, `no_credentials` |
| `list-rate-limited.json` | dopo un 429: `ok` con `usageAgeSeconds` oltre 300 (fiducia estesa fino al reset) e `unavailable` con `lastGoodUsage` |
| `list-token-expired.json` | token attivo scaduto (`token_expired`) e lineage morta (`relogin_required`) |
| `auto-events.jsonl` | eventi di `cswap auto --json` (uno per riga) e busta d'errore finale |
