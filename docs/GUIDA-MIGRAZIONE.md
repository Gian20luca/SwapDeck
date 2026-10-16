# Passare a SwapDeck da script o attività pianificate con cswap, oppure da `cswap auto`

Per chi oggi cambia account con uno script lanciato da un'attività pianificata (o da un timer, un cron, un LaunchAgent) che richiama `cswap`, oppure con `cswap auto`, e vuole passare al motore di SwapDeck. Il passaggio è reversibile e il vecchio meccanismo si spegne **solo alla fine**.

## 0. Riconoscere un altro motore attivo

SwapDeck lo rileva da solo e mette il motore in **«sospeso»** (icona dedicata, riga nel tooltip, avviso sopra la dashboard). Non è un errore: due automatismi insieme si contraddirebbero. Sono considerati:

- il processo `cswap auto` in esecuzione;
- un'attività pianificata, un timer, un cron o un LaunchAgent il cui nome, programma o argomenti citano `cswap` o `claude-swap`.

Un'attività solo disabilitata dà un avviso e non sospende il motore. L'id esatto del conflitto (per esempio `task:` seguito dal percorso dell'attività) è nell'avviso; maiuscole e minuscole non contano. Uno script dal nome neutro che chiama `cswap` all'interno non si riconosce: se non c'è l'avviso ma i cambi arrivano lo stesso, cerca nello scheduler del sistema.

## 1. Riprodurre la propria regola con le mappe

Una **mappa** è un insieme di regole del motore (la scheda Regole ne gestisce più di una). Le regole ricalcano quelle tipiche di uno script a soglie:

| Regola tipica | Campo di `engine` |
|---|---|
| ordine di priorità: la prima è la principale, poi le riserve | `order` (lista di numeri di account) |
| scelta della riserva con la % **settimanale** più bassa | `reserveCriterion: "lowest-weekly"` |
| lascia l'account alla soglia delle 5 ore | `fiveHourThreshold` |
| account non utilizzabile oltre la soglia settimanale | `sevenDayThreshold` |
| riserva al limite: si passa alla successiva | automatico |
| ritorno alla principale dopo il reset | `returnToPrimary: true`, `returnHysteresis` |
| intervallo del giro | `pollIntervalSeconds` (in cima alla config, fuori da `engine`) |

Preset della scheda Regole: ognuno è una mappa, che si sceglie dal selettore «Mappa» (anche nel mini-widget) e si riporta ai valori di partenza con «Ripristina preset». Cambiano criterio e soglie; lista ed esclusi, conflitti ignorati, `enabled` e `dryRun` restano i tuoi. In tutti il ritorno alla principale è acceso, la pausa tra due cambi è di 300 s e i consumi valgono per 900 s.

| Preset | Criterio (`reserveCriterion`) | 5 ore | 7 giorni | `returnHysteresis` |
|---|---|---|---|---|
| **Principale con riserve** (la regola tipica) | Settimanale più bassa (`lowest-weekly`) | 95 | 98 | 45: torna con la 5 ore sotto 50 |
| **Eco** | Reset settimanale più vicino (`reset-soonest`) | 90 | 98 | 10 |
| **Sport** | 5 ore più libera (`lowest5h`) | 98 | 98 | 10 |
| **Sicura** | Ritmo sicuro (`pace-safe`) | 95 | 98 | 10 |

Gli altri criteri sono Migliore (`best`), Ordine fisso (`fixed-order`), Punteggio bilanciato (`balanced-score`), Settimanali livellate (`even-spread`) e Più margine sul modello (`model-headroom`, con il modello in `headroomModel`): il dettaglio è nel [README](../README.md#motore-delle-regole). Nella scheda «Adesso andrebbe a» dice quale utenza sceglierebbe il criterio in questo momento.

Da riga di comando, in un file `preset.json` (sostituisci i numeri con quelli dei tuoi account):

```json
{"pollIntervalSeconds":120,"engine":{"order":[1,2,3],"reserveCriterion":"lowest-weekly","fiveHourThreshold":95,"sevenDayThreshold":98,"returnToPrimary":true,"returnHysteresis":45,"dryRun":true}}
```

Applicalo con `swapdeck-cli config set -` leggendo da stdin (PowerShell: `Get-Content preset.json -Raw | swapdeck-cli config set -`; bash: `swapdeck-cli config set - < preset.json`). Il comando fa un merge patch: tocca solo i campi indicati. `dryRun: true` è voluto (passo 2).

Differenze note rispetto a uno script semplice:

- SwapDeck aggiunge una pausa minima tra due cambi (`minSwitchGapSecs`, 300) e scarta consumi più vecchi di `maxUsageAgeSecs` (900).
- Il ritorno usa «punti sotto la soglia» (5h sotto 95 - 45 = 50) e aspetta l'ora di reset della principale.
- L'isteresi vale solo sulle finestre che hanno fatto lasciare la principale: se è uscita per la settimana (7d >= 98), SwapDeck torna con 7d sotto 53 (98 - 45) e 5h sotto 95. Uno script senza isteresi torna appena 7d < 98 e 5h < 50.

## 2. Prova in sola osservazione

1. Lascia il vecchio meccanismo com'è: il motore si sospende (passo 0).
2. Per vedere le decisioni durante la prova, ignora il conflitto solo temporaneamente: aggiungi l'id letto nell'avviso (per esempio `"ignoredConflicts":["task:nome-attività"]`) alla sezione `engine` e tieni `dryRun: true`. In sola osservazione SwapDeck non cambia mai account.
3. Confronta per qualche giorno: storico e righe del motore (`swapdeck-cli headless --once --dry-run`) dicono cosa SwapDeck avrebbe fatto; il log del vecchio meccanismo dice cosa ha fatto lui. Le decisioni devono coincidere (salvo la pausa tra due cambi).
4. Durante la prova il mini-widget mostra la mappa attiva e, nel tooltip del trattino sulla barra, l'utenza a cui andrebbe il motore; nella pagina **Notifiche** (dalle impostazioni) tieni accesi «Preavviso di cambio» e «Motore sospeso o ripreso» per sapere quando l'attiva si avvicina alla soglia o il motore si ferma.
5. Finita la prova togli il conflitto da `ignoredConflicts` (passo 4).

## 3. Come far girare il motore

Scegli **una** strada. Il lock garantisce un solo motore per utente: le altre forme restano in osservazione.

### A. App sempre aperta

Installa l'app, tieni l'avvio automatico al login e porta `enabled: true` e `dryRun: false`. Il motore gira nel ciclo di lettura dell'app.

### B. Attività pianificata o servizio utente con `swapdeck-cli headless`

Senza interfaccia. Usa `--once` con uno scheduler, oppure il ciclo continuo con un servizio. Dove sta il binario:

- Windows: `%LOCALAPPDATA%\SwapDeck` dopo l'installer, con `swapdeck.exe` e `swapdeck-cli.exe`;
- Linux: `/usr/bin/swapdeck-cli` con il `.deb`, oppure il file `swapdeck-cli-x86_64-unknown-linux-gnu` della release copiato in `~/.local/bin/swapdeck-cli` e reso eseguibile;
- macOS: `/Applications/SwapDeck.app/Contents/MacOS/swapdeck-cli`, oppure il file `swapdeck-cli-<arch>-apple-darwin` della release.

**Windows** (per utente, senza admin), un giro ogni 2 minuti. `swapdeck-cli.exe` è un programma da console e a ogni giro farebbe comparire una finestra: nell'attività conviene `swapdeck.exe --headless --once`, stesse opzioni e nessuna finestra. Da `cmd`:

```
schtasks /Create /SC MINUTE /MO 2 /TN "SwapDeck" /TR "\"%LOCALAPPDATA%\SwapDeck\swapdeck.exe\" --headless --once" /F
```

Oppure con PowerShell:

```powershell
$a = New-ScheduledTaskAction -Execute "$env:LOCALAPPDATA\SwapDeck\swapdeck.exe" -Argument "--headless --once"
$t = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 2)
Register-ScheduledTask -TaskName "SwapDeck" -Action $a -Trigger $t
```

Né il nome dell'attività né il programma o gli argomenti devono contenere `cswap` o `claude-swap`: SwapDeck guarda tutti e tre, la scambierebbe per un altro motore e si sospenderebbe. Lo stesso vale per il nome del timer e l'`ExecStart` del suo servizio su Linux, per le righe di crontab e per l'etichetta del LaunchAgent su macOS.

**Linux** (`systemd --user`), file `~/.config/systemd/user/swapdeck.service`:

```ini
[Unit]
Description=SwapDeck engine

[Service]
ExecStart=%h/.local/bin/swapdeck-cli headless
Restart=on-failure
RestartPreventExitStatus=3

[Install]
WantedBy=default.target
```

```
systemctl --user daemon-reload
systemctl --user enable --now swapdeck.service
loginctl enable-linger "$USER"   # facoltativo: parte anche senza login
```

**macOS** (LaunchAgent), file `~/Library/LaunchAgents/it.example.swapdeck.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>Label</key><string>it.example.swapdeck</string>
  <key>ProgramArguments</key><array>
    <string>/Users/TUO_UTENTE/.local/bin/swapdeck-cli</string>
    <string>headless</string>
    <string>--once</string>
  </array>
  <key>StartInterval</key><integer>120</integer>
</dict></plist>
```

```
launchctl load ~/Library/LaunchAgents/it.example.swapdeck.plist
```

L'etichetta non deve contenere `cswap` né `claude-swap` (stessa ragione di Windows).

## 4. Passaggio definitivo

Solo quando la prova combacia.

1. Disattiva il vecchio meccanismo, senza cancellarlo: `schtasks /Change /TN "<nome attività>" /DISABLE` (Windows), oppure disabilita il timer, il cron o il LaunchAgent equivalente; se era `cswap auto`, spegnilo. Conserva lo script. Un'attività disabilitata resta come semplice avviso, senza sospendere il motore.
2. Togli il conflitto da `ignoredConflicts` (dalla scheda Regole, oppure `{"engine":{"ignoredConflicts":[]}}` con `config set`): l'app e il processo headless rileggono le regole al giro successivo, senza riavvio.
3. Accendi il motore con `{"engine":{"enabled":true,"dryRun":false}}`: in PowerShell `'{"engine":{"enabled":true,"dryRun":false}}' | swapdeck-cli config set -` (Windows PowerShell 5.1 toglie le virgolette dagli argomenti, lo stdin no); in bash `swapdeck-cli config set '{"engine":{"enabled":true,"dryRun":false}}'`.
4. Verifica dalla dashboard o con `swapdeck-cli engine status` che il motore sia attivo e senza conflitti.

## 5. Tornare indietro

1. Spegni SwapDeck con `{"engine":{"enabled":false}}` (stessa forma del passo 4.3) e ferma l'attività o il servizio di SwapDeck.
2. Riabilita il vecchio meccanismo: `schtasks /Change /TN "<nome attività>" /ENABLE` (oppure il timer, il cron, il LaunchAgent o `cswap auto`).

Con il vecchio meccanismo riattivato e SwapDeck spento non c'è conflitto e `cswap` resta quello di sempre.
