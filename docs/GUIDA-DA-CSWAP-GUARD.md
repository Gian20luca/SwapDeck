# Dal guard PowerShell a SwapDeck

Per chi oggi ha uno script (per esempio `cswap-guard.ps1`) lanciato ogni pochi minuti da un'attività pianificata e vuole passare al motore di SwapDeck. Il passaggio è reversibile e il vecchio guard si spegne **solo alla fine**.

## 1. Corrispondenza delle regole

| Regola del guard | `engine` di SwapDeck |
|---|---|
| account principale (es. 6) | `primary: 6` |
| riserve (es. 1 e 3) | `reserves: [1, 3]` |
| scegli la riserva con la % **settimanale** più bassa | `reserveCriterion: "lowest-weekly"` |
| lascia il principale al 95% delle 5 ore | `fiveHourThreshold: 95` |
| account non utilizzabile se 7d >= 98% | `sevenDayThreshold: 98` |
| riserva al limite: passa all'altra | automatico (si passa alla successiva) |
| ritorno al principale dopo il reset (5h sotto il 50%) | `returnToPrimary: true`, `returnHysteresis: 45` |
| giro ogni 2 minuti | `pollIntervalSeconds: 120` |

Preset «come cswap-guard» (numeri di esempio, sostituisci con i tuoi), in un file `preset.json`:

```json
{"engine":{"primary":6,"reserves":[1,3],"reserveCriterion":"lowest-weekly","fiveHourThreshold":95,"sevenDayThreshold":98,"returnToPrimary":true,"returnHysteresis":45,"pollIntervalSeconds":120,"dryRun":true}}
```

Applicalo con `swapdeck-cli config set -` leggendo da stdin (PowerShell: `Get-Content preset.json -Raw | swapdeck-cli config set -`; bash: `swapdeck-cli config set - < preset.json`). Il comando fa un merge patch: tocca solo i campi indicati. `dryRun: true` è voluto (passo 2).

Differenze note: SwapDeck aggiunge una pausa minima tra due cambi (`minSwitchGapSecs`, 300) e scarta consumi più vecchi di `maxUsageAgeSecs` (900); il guard no. Il ritorno usa «punti sotto la soglia»: 95 - 45 = 50, come il guard.

## 2. Prova in sola osservazione

1. Lascia il guard com'è. Con il guard attivo il motore si **sospende**: è voluto (due automatismi insieme si contraddirebbero) e non è un errore.
2. Per vedere le decisioni durante la prova, ignora il conflitto solo temporaneamente: aggiungi `"ignoredConflicts":["task:cswap-guard"]` alla sezione `engine` (l'id esatto è nell'avviso sopra la dashboard) e tieni `dryRun: true`. In sola osservazione SwapDeck non cambia mai account.
3. Confronta per qualche giorno: storico e righe del motore (`swapdeck-cli headless --once --dry-run`) dicono cosa SwapDeck avrebbe fatto; il log del guard dice cosa ha fatto lui. Le decisioni devono coincidere (salvo la pausa tra due cambi).
4. Finita la prova togli `ignoredConflicts`.

## 3. Come far girare il motore

Scegli **una** strada. Il lock garantisce un solo motore per utente: le altre forme restano in osservazione.

### A. App sempre aperta

Installa l'app, tieni l'avvio automatico al login e porta `enabled: true` e `dryRun: false`. Il motore gira nel ciclo di lettura dell'app.

### B. Attività pianificata o servizio utente con `swapdeck-cli headless`

Senza interfaccia e senza app. Usa `--once` con uno scheduler, oppure il ciclo continuo con un servizio. Sostituisci i percorsi del binario con i tuoi.

**Windows** (per utente, senza admin), un giro ogni 2 minuti:

```
schtasks /Create /SC MINUTE /MO 2 /TN "SwapDeck" /TR "\"%LOCALAPPDATA%\Programs\swapdeck\swapdeck-cli.exe\" headless --once" /F
```

Oppure con PowerShell:

```powershell
$a = New-ScheduledTaskAction -Execute "$env:LOCALAPPDATA\Programs\swapdeck\swapdeck-cli.exe" -Argument "headless --once"
$t = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 2)
Register-ScheduledTask -TaskName "SwapDeck" -Action $a -Trigger $t
```

Il nome dell'attività non deve contenere `cswap`, altrimenti SwapDeck la scambia per un guard e si sospende.

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

L'etichetta non deve contenere `cswap` (stessa ragione di Windows).

## 4. Passaggio definitivo

Solo quando la prova combacia.

1. Disattiva il vecchio guard, senza cancellarlo: `schtasks /Change /TN "cswap-guard" /DISABLE` (Windows), oppure disabilita il timer, il cron o il LaunchAgent equivalente. Conserva lo script.
2. Togli `ignoredConflicts` se ancora presente e riavvia l'app o il processo headless (le regole si rileggono all'avvio): lo stato «sospeso» sparisce.
3. Accendi il motore: `swapdeck-cli config set '{"engine":{"enabled":true,"dryRun":false}}'`.
4. Verifica dalla dashboard o con `swapdeck-cli engine status` che il motore sia attivo e senza conflitti.

## 5. Tornare indietro

1. Spegni SwapDeck: `swapdeck-cli config set '{"engine":{"enabled":false}}'` e ferma l'attività o il servizio di SwapDeck.
2. Riabilita il vecchio guard: `schtasks /Change /TN "cswap-guard" /ENABLE`.

Con il guard riattivato e SwapDeck spento non c'è conflitto e `cswap` resta quello di sempre.
