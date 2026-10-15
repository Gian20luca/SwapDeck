# SwapDeck — Piano

GUI per `cswap` (claude-swap): monitora i limiti di tutti gli account Claude, li gestisce e cambia account in automatico secondo regole configurabili.
Si distribuisce in due forme, a scelta di chi installa: **app nell'area di notifica (Tauri 2)** ed **estensione VS Code**.
Gira su Windows (priorità), Linux e macOS, senza permessi di amministratore, con un consumo minimo di risorse.

Stato: **piano, in attesa del via**. Prontezza circa 95% (il contratto di `cswap` si verifica allo step 0).

## 1. Funzionalità confermate

| # | Funzionalità | Fonte |
|---|---|---|
| F0 | Tutti i comandi di `cswap` in GUI: list/status, switch (diretto, `best`, `next-available`, `--model`), add, add-token, remove, disable/enable, alias, swap, move, map/unmap, config, auto, unclaimed, export/import, run (apre un terminale come account), upgrade, purge | cswap 0.26.0 |
| F1 | Storico dei consumi con grafici, più la curva del ritmo atteso (`expectedPct`, `projectedExhaustionAt`) | confermata |
| F2 | Notifiche di sistema alle soglie (default 80% e 95%) e a ogni cambio di account | confermata |
| F3 | Motore di regole che generalizza `cswap-guard` | confermata (punti 3 e 9) |
| F4 | Mini-widget flottante sempre in primo piano, oltre all'icona nell'area di notifica | confermata |
| F5 | Rileva `cswap` e, se manca, installa `uv` e `claude-swap` per utente. Controlla la versione installata contro l'ultima su PyPI, avvisa se ce n'è una nuova e la aggiorna con conferma (`uv tool upgrade claude-swap`; `cswap upgrade` su Windows non funziona). Dopo l'aggiornamento verifica `schemaVersion` | confermata |
| F6 | Avvio automatico al login (per utente) | confermata |
| F7 | Estensione VS Code con monitoraggio **e** azioni | confermata |
| F8 | Aggiornamento automatico dell'app | confermata (dipende dal repo) |
| F10 | Modalità senza interfaccia `swapdeck --headless [--once]`, per attività pianificate, WSL e server | confermata |
| F11 | Lock unico: il motore gira in un'istanza sola; avviso se sono attivi `cswap auto` o un guard esterno | confermata |
| F13 | Interfaccia in italiano e in inglese | confermata |
| — | Conferma a video prima delle azioni delicate | confermata |

Esclusa: esportazione dello storico in CSV (punto 12).

### Regole del motore (F3)
- Account principale, elenco ordinato delle riserve, account esclusi.
- Soglia di uscita sulle 5 ore (default 95%), facoltativa anche sui 7 giorni.
- Criterio di scelta della riserva: settimanale più bassa, più margine (`best`) oppure ordine fisso. Se la riserva scelta è al limite si passa alla successiva.
- Ritorno al principale dopo il suo reset, con isteresi.
- Intervallo di controllo (default 60 s) e pausa minima tra due cambi.
- La regola che usi oggi diventa la configurazione: principale 6, riserve [1, 3], criterio «settimanale più bassa», soglia 95%, ritorno dopo il reset.

### Azioni con conferma
- `remove`, `import` (sovrascrive), `export` (il file contiene credenziali), `swap`/`move`, `unmap`, `disable` dell'account attivo, `unclaimed --purge`, installazione di uv/cswap: conferma semplice.
- `purge`: doppia conferma, con la parola da digitare.
- `switch` manuale: nessuna conferma.

## 2. Architettura

```
WidgetCswap/
  crates/swapdeck-core/   Rust: client cswap, config, motore regole, lock, storico, rilevamento conflitti
  apps/desktop/           Tauri 2: area di notifica, finestre, notifiche, autostart, updater, modalità --headless
  apps/vscode/            estensione VS Code (TypeScript, esbuild)
  packages/ui/            React + TypeScript + Vite: dashboard, grafici, gestione account, regole, i18n
  packages/types/         tipi TS generati dal Rust (ts-rs), unica fonte dei formati
  docs/
```

- **Una sola fonte di logica: il Rust.** Il motore, il lock e lo storico vivono in `swapdeck-core`. L'app Tauri li usa con l'interfaccia; `swapdeck --headless` è lo stesso binario senza finestre.
- **Interfaccia condivisa.** `packages/ui` parla con un'interfaccia `Backend` che ha due implementazioni: `invoke` di Tauri e `postMessage` del webview VS Code.
- **Estensione VS Code.** Lancia `cswap` direttamente per leggere i dati e per le azioni manuali. Il cambio automatico non lo rifà: se è installato `swapdeck`, avvia `swapdeck --headless`, che rispetta il lock; altrimenti propone di installarlo. Così la logica delle regole non esiste in due copie.
- **Dati per utente** (nessun percorso di sistema):
  - Windows: `%APPDATA%\swapdeck`
  - Linux: `~/.config/swapdeck` e `~/.local/share/swapdeck`
  - macOS: `~/Library/Application Support/swapdeck`
- **Config:** `config.json` (regole, soglie, intervallo, lingua, tema).
- **Storico:** un file JSONL al giorno, letto anche dall'estensione; conservazione di default 30 giorni.
- **Lock:** file con PID e battito periodico; è scaduto dopo tre intervalli senza battito. Lo storico lo scrive solo chi tiene il lock.
- **Conflitti:** cerca processi `cswap auto` e attività pianificate con «cswap» nel nome (`schtasks` / `crontab -l` / `launchctl list`, tutto senza admin). Se ne trova, sospende il proprio motore e avvisa.
- **Interrogazione di cswap:** `cswap list --json` ogni 60 s, con timeout. Si tiene conto di `usageAgeSeconds` per non martellare l'endpoint dei consumi.
- **Grafici:** uPlot, piccolo e veloce sulle serie temporali.
- **Installer:**
  - Windows: NSIS `currentUser` con bootstrapper WebView2.
  - Linux: AppImage e .deb.
  - macOS: .dmg (non firmato: al primo avvio clic destro → Apri).
  - VS Code: .vsix interno.
- **Sicurezza:** i token di `add-token` passano via stdin (`-`) e non finiscono mai in log, storico o messaggi di errore. Nei log niente email complete.

## 3. Step

Gli agenti si scelgono secondo le regole globali. Step 2 e 3 possono andare in parallelo dopo lo step 1, e lo step 7 parte dopo il 3.

| Step | Contenuto | Agente | Uscita verificabile |
|---|---|---|---|
| 0 | Fondamenta: monorepo pnpm e workspace Cargo, toolchain (rustup per utente; Build Tools MSVC se si installano senza admin, altrimenti build Windows in CI), `CLAUDE.md` di progetto. Verifica del contratto di cswap: `list`/`status`/`switch --json`, codici di uscita, errori (token scaduto, 429, rete), campi null. Fixture anonimizzate | `client-desktop` | `docs/CSWAP-CONTRATTO.md`, fixture, build vuota verde |
| 1 | `swapdeck-core`: ricerca di cswap (PATH, `~/.local/bin`, override `SWAPDECK_CSWAP`), esecuzione con timeout, parsing, classificazione degli errori, config, cartelle dati. Finto `cswap` per i test | `client-desktop` | test unitari e di integrazione sul finto cswap |
| 2 | Motore delle regole (funzione pura: stato + regole → azione), lock, storico JSONL con rotazione, rilevamento conflitti, `swapdeck --headless [--once]` | `client-desktop` | test sugli scenari del guard attuale e sui casi limite |
| 3 | `packages/ui`: dashboard (schede account, barre 5h/7d, ritmo, limiti per modello, età del dato), grafici dello storico, finestra di conferma, i18n it/en, tema chiaro/scuro, backend finto per lo sviluppo | `frontend` | vitest + anteprima con backend finto |
| 4 | App Tauri: icona nell'area di notifica (anello colorato secondo la %, tooltip, menu con cambio rapido), finestra principale, mini-widget sempre in primo piano, notifiche, autostart, istanza singola, comandi `invoke` | `client-desktop` | app funzionante su Windows contro il cswap reale |
| 5 | Gestione completa in UI: tutte le azioni di F0, editor delle regole, impostazioni, conferme | `frontend` (Rust: `client-desktop`) | ogni comando di cswap raggiungibile dalla GUI |
| 6 | Procedura di avvio: rileva cswap; se manca, con conferma installa uv (script ufficiale per utente) e `uv tool install claude-swap`, poi verifica; pulsante di aggiornamento | `client-desktop` | prova su un profilo Windows senza cswap |
| 7 | Estensione VS Code: barra di stato, lista rapida per il cambio, pannello con `packages/ui`, impostazioni, avvio di `--headless`, pacchetto .vsix | `client-desktop` + `frontend` | .vsix installabile, test dell'estensione |
| 8 | Pacchetti e CI: matrice Windows/Linux/macOS, installer, updater (chiave di firma degli aggiornamenti, endpoint sulle release) | `client-desktop` | artefatti per i tre sistemi dalla CI |
| 9 | Verifica finale: suite completa ed E2E una volta sola, revisione, README e guida all'installazione per i colleghi, guida per passare da `cswap-guard` | `revisore` + `documentazione` | tutto verde, documenti pronti |

## 4. Decisioni provvisorie (reversibili)
- Il motore esiste solo in Rust; l'estensione delega a `swapdeck --headless`.
- Storico in JSONL anziché SQLite: più leggero e leggibile anche dalla parte TypeScript.
- Grafici con uPlot.
- Nessuna firma del codice per ora (avviso di SmartScreen e Gatekeeper accettato).
- Build Windows locale se i Build Tools MSVC si installano senza admin; in ogni caso c'è la CI.
- L'estensione si distribuisce come .vsix, senza Marketplace.
- `cswap-guard` resta com'è finché non decidi di passare a `swapdeck --headless`. I file in `~/.claude` non si toccano senza il tuo permesso.

## 5. Repo
- GitHub privato: https://github.com/Gian20luca/SwapDeck. CI con GitHub Actions; artefatti e updater sulle GitHub Releases.
- Aperto: con il repo privato l'updater (F8) e i download dei colleghi richiedono un accesso al repo. Scelta provvisoria: updater predisposto ma spento di default finché il repo è privato; i colleghi vengono aggiunti come collaboratori.
