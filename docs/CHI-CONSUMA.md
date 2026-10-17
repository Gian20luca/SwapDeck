# Chi consuma (F20)

Classifica dei consumi di Claude Code di questo PC per progetto, sessione, agente o modello, attribuiti all'utenza attiva in quel momento, più la stima di quanto consumano gli altri dispositivi. Codice in `crates/swapdeck-core/src/usage*`.

## Fonte e privacy

- Si leggono in **sola lettura** `<claude>/projects/**/*.jsonl`, sotto-agenti compresi (`<progetto>/<sessione>/subagents/[workflows/<wf>/]agent-<id>.jsonl`). `<claude>` è `CLAUDE_CONFIG_DIR` se impostata e assoluta, altrimenti `~/.claude` (`%USERPROFILE%\.claude` su Windows). I profili di `cswap run` (`<backup>/sessions/`) non si leggono.
- Di ogni riga si tengono solo orario, id del messaggio, modello e i quattro conteggi di token (input, output, scrittura e lettura della cache). Le righe senza la chiave `"usage":` non si analizzano; nelle altre il parser salta i campi sconosciuti (testo, strumenti, `cwd`) senza copiarli. Il `cwd` si legge solo per il nome da mostrare del progetto (sotto, «Interfaccia») e resta in memoria. Righe oltre 16 MB si saltano senza tenerle in memoria.
- Progetto = nome della cartella; sessione = nome del file o della cartella; agente = id del file e, dai metadati `agent-<id>.meta.json`, solo `agentType` e `description` (160 caratteri al massimo, caratteri di controllo tolti). La descrizione è testo generato dal modello a partire dal prompt: resta nell'indice locale e nell'app, non esce dal PC e non va mai in log o messaggi d'errore (decisione del 16/10).
- Link simbolici e reparse point mai seguiti; file aperti per handle con condivisione completa (Claude Code può continuare a scrivere).

## Indice

`<dati>/usage/index.json`, scritto in modo atomico. Per file: posizione letta, lunghezza, ora di modifica, identità (inode o ora di creazione), ultimo istante contato e gli ultimi 16 id di messaggio. Totali per ora, utenza, sorgente (progetto, sessione, agente) e modello; serie al minuto per utenza e modello (serve alla calibrazione).

- Lettura incrementale: un file con lunghezza e ora di modifica invariate non si apre; uno cresciuto si legge dalla posizione salvata. Una riga finale senza a capo si rilegge la volta dopo, a meno che sia già un JSON completo.
- Claude Code scrive lo stesso messaggio su più righe: si conta una volta, prendendo il massimo di ogni conteggio.
- File sostituito o accorciato: si rilegge da capo contando solo i messaggi successivi all'ultimo già contato (limite tenuto nell'indice finché la rilettura non finisce); le righe successive di un messaggio scartato si scartano anche loro.
- Lettura a budget: al massimo 256 MB o 8 s per chiamata, fermandosi a fine riga; l'indice e le posizioni si salvano a ogni chiamata e la risposta porta `partial: { scannedBytes, totalBytes }` (byte già nell'indice sul totale dei file): la UI mostra l'avanzamento in percentuale e richiama finché `partial` non manca (al massimo 200 chiamate; se una chiamata non avanza si ferma e mostra quanto letto con un avviso). Ogni chiamata dura pochi secondi; l'estensione VS Code le dà 2 minuti.
- Storico illeggibile: la lettura si ferma con l'errore prima di toccare l'indice (niente salvataggio, posizioni ferme).
- Conservazione di 30 giorni come lo storico: file modificati prima non si leggono, ore e minuti più vecchi si tolgono. Indice illeggibile o di un'altra versione: si rifà da capo.
- La classifica ha risoluzione oraria: l'ora in cui cade l'inizio della finestra entra intera.

## Attribuzione all'utenza

Dallo storico di SwapDeck: battito del giro (`HeartbeatRecord { ts, active?, intervalSecs }`, scritto a ogni giro del motore e del ciclo di lettura dell'app da chi tiene il lock, anche senza letture nuove), campione con `active: true`, cambio riuscito (`SwitchRecord` con `switched`), campione con `active: false` dell'utenza ritenuta attiva (da lì non si sa più quale lo sia). L'utenza in `t` è quella dell'ultimo evento non successivo a `t`, purché lo storico non sia muto prima di `t` più del silenzio ammesso, `2 × intervallo + 120 s` con l'intervallo dell'ultimo battito (30 minuti per lo storico senza battiti); altrimenti «sconosciuta», mostrata a parte. Si usano solo righe passate, quindi attribuire subito o più tardi dà lo stesso risultato. Un cambio fatto fuori da SwapDeck si vede al giro dopo. I battiti non escono da `swapdeck-cli history`.

## Token equivalenti

`input × 1 + output × 5 + scrittura cache × 1,25 + lettura cache × 0,1` (rapporti dei prezzi delle API). La classifica ordina per token equivalenti; la differenza tra modelli la assorbe la calibrazione per modello.

## Stima degli altri dispositivi

Per ogni utenza e finestra del server (5h, 7d):

1. **Intervalli.** I campioni con lo stesso `resetsAt` si uniscono finché la percentuale sale di almeno 3 punti (5h) o 2 (7d), entro 3 ore (5h) o 24 (7d); l'istante è `usageFetchedAt`. Per ogni intervallo: aumento sul server `Δp` e token equivalenti locali `U` dell'utenza, per modello. Si scartano gli intervalli con più del 10% di consumo di utenza sconosciuta e quelli in cui l'utenza non è stata attiva, senza cambi e con lo storico senza silenzi, per tutto l'intervallo più un giro di margine per lato (l'intervallo più recente entra quando il giro dopo l'ha testimoniato).
2. **Calibrazione (mediana dei rapporti).** `r = U / Δp`. Dove consuma anche un altro dispositivo `Δp` è più alto e `r` più basso, quindi si cercano i rapporti alti: si parte dall'80° percentile, si tengono gli `r` entro ±40% e si ricalcola la mediana finché il gruppo non cambia; poi `k = ΣU / ΣΔp` sul gruppo tenuto (token equivalenti per punto). Un `k` per modello dove il modello fa almeno l'80% dell'intervallo e con almeno 5 intervalli tenuti, altrimenti quello dell'utenza. Servono almeno 3 intervalli coerenti: sotto, «stima non ancora disponibile» (`notCalibrated`).
3. **Stima.** Finestra corrente da `resetsAt − durata` (non prima del reset precedente) all'ultima lettura esclusa (come negli intervalli): punti locali `Σ U_m / k_m`, altri dispositivi `max(0, p_server − punti locali)`, anche in token equivalenti (`× k`).
4. **Fiducia.** Alta con almeno 8 intervalli e dispersione (MAD/mediana) fino a 0,2; media con almeno 5 e fino a 0,35; altrimenti bassa. Bassa anche se nella finestra il consumo di utenza sconosciuta supera il 10% (`historyGaps`); un grado sotto se più del 10% del consumo locale della finestra è di modelli senza `k` proprio.

Limiti: le percentuali sono intere (errore fino a un punto per lettura); un altro dispositivo che consuma sempre insieme a questo PC sposta la calibrazione verso il basso e la stima verso zero.

## Interfaccia

- Core: `usage::usage_breakdown(UsagePaths, UsageQuery, now)` → `UsageBreakdown` (budget predefinito; `usage_breakdown_with` con un `ScanBudget`); `breakdown` è la parte pura.
- `UsageQuery`: `account?`, `window` (`5h` | `7d` | `custom` con `fromSeconds` e `toSeconds`), `groupBy` (`project` | `session` | `agent` | `model`). Con l'utenza scelta, 5h e 7d sono la finestra corrente del server; senza, finestre mobili.
- Righe ordinate per token equivalenti (al massimo 200, `truncatedRows`), con `sharePct` sul totale locale più gli altri dispositivi stimati (solo con l'utenza scelta e la stima disponibile), `lastUsed`; `unattributed` a parte; `otherDevices` per utenza e finestra; `scan` con le statistiche della lettura; `partial` finché la lettura a budget non è finita.
- Righe con `project` (nome della cartella di Claude Code, cioè il percorso intero codificato: resta nell'indice e serve solo da chiave) e `projectLabel` (solo l'ultima cartella). Il nome si prende dal `cwd` delle righe di sessione quando il percorso, codificato come fa Claude Code (ogni carattere non alfanumerico ASCII → `-`), ridà esattamente il nome della cartella del progetto: si leggono al massimo i primi 256 KB delle tre sessioni principali più recenti del progetto, a ogni classifica; il `cwd` resta in memoria, non va nell'indice né nei log. Altrimenti (nessun `cwd`, un `cd` in una sottocartella, nomi lunghi accorciati da Claude Code) si ricostruisce dal nome della cartella, dove un trattino del nome originale non si distingue da un separatore (`my-app` → `app`). Nella UI e nei log si mostra solo `projectLabel`, mai il percorso intero.
- Tauri `get_usage_breakdown`, CLI `swapdeck-cli usage breakdown`, `Backend.getUsageBreakdown` facoltativo.
