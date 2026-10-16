# Chi consuma (F20)

Classifica dei consumi di Claude Code di questo PC per progetto, sessione, agente o modello, attribuiti all'utenza attiva in quel momento, più la stima di quanto consumano gli altri dispositivi. Codice in `crates/swapdeck-core/src/usage*`.

## Fonte e privacy

- Si leggono in **sola lettura** `<claude>/projects/**/*.jsonl`, sotto-agenti compresi (`<progetto>/<sessione>/subagents/[workflows/<wf>/]agent-<id>.jsonl`). `<claude>` è `CLAUDE_CONFIG_DIR` se impostata e assoluta, altrimenti `~/.claude` (`%USERPROFILE%\.claude` su Windows). I profili di `cswap run` (`<backup>/sessions/`) non si leggono.
- Di ogni riga si tengono solo orario, id del messaggio, modello e i quattro conteggi di token (input, output, scrittura e lettura della cache). Le righe senza la chiave `"usage":` non si analizzano; nelle altre il parser salta i campi sconosciuti (testo, strumenti, `cwd`) senza copiarli. Righe oltre 16 MB si saltano senza tenerle in memoria.
- Progetto = nome della cartella; sessione = nome del file o della cartella; agente = id del file e, dai metadati `agent-<id>.meta.json`, solo `agentType` e `description` (160 caratteri al massimo, caratteri di controllo tolti).
- Link simbolici e reparse point mai seguiti; file aperti per handle con condivisione completa (Claude Code può continuare a scrivere).

## Indice

`<dati>/usage/index.json`, scritto in modo atomico. Per file: posizione letta, lunghezza, ora di modifica, identità (inode o ora di creazione), ultimo istante contato e gli ultimi 16 id di messaggio. Totali per ora, utenza, sorgente (progetto, sessione, agente) e modello; serie al minuto per utenza e modello (serve alla calibrazione).

- Lettura incrementale: un file con lunghezza e ora di modifica invariate non si apre; uno cresciuto si legge dalla posizione salvata. Una riga finale senza a capo si rilegge la volta dopo, a meno che sia già un JSON completo.
- Claude Code scrive lo stesso messaggio su più righe: si conta una volta, prendendo il massimo di ogni conteggio.
- File sostituito o accorciato: si rilegge da capo contando solo i messaggi successivi all'ultimo già contato.
- Conservazione di 30 giorni come lo storico: file modificati prima non si leggono, ore e minuti più vecchi si tolgono. Indice illeggibile o di un'altra versione: si rifà da capo.
- La classifica ha risoluzione oraria: l'ora in cui cade l'inizio della finestra entra intera.

## Attribuzione all'utenza

Dallo storico di SwapDeck: campione con `active: true`, cambio riuscito (`SwitchRecord` con `switched`), campione con `active: false` dell'utenza ritenuta attiva (da lì non si sa più quale lo sia). L'utenza in `t` è quella dell'ultimo evento non successivo a `t`, purché lo storico non sia muto da più di 30 minuti prima di `t` (SwapDeck spento); altrimenti «sconosciuta», mostrata a parte. Si usano solo righe passate, quindi attribuire subito o più tardi dà lo stesso risultato. Un cambio fatto fuori da SwapDeck si vede al campione dopo.

## Token equivalenti

`input × 1 + output × 5 + scrittura cache × 1,25 + lettura cache × 0,1` (rapporti dei prezzi delle API). La classifica ordina per token equivalenti; la differenza tra modelli la assorbe la calibrazione per modello.

## Stima degli altri dispositivi

Per ogni utenza e finestra del server (5h, 7d):

1. **Intervalli.** I campioni con lo stesso `resetsAt` si uniscono finché la percentuale sale di almeno 3 punti (5h) o 2 (7d), entro 3 ore (5h) o 24 (7d); l'istante è `usageFetchedAt`. Per ogni intervallo: aumento sul server `Δp` e token equivalenti locali `U` dell'utenza, per modello. Intervalli con più del 10% di consumo di utenza sconosciuta si scartano.
2. **Calibrazione (mediana dei rapporti).** `r = U / Δp`. Dove consuma anche un altro dispositivo `Δp` è più alto e `r` più basso, quindi si cercano i rapporti alti: si parte dall'80° percentile, si tengono gli `r` entro ±40% e si ricalcola la mediana finché il gruppo non cambia; poi `k = ΣU / ΣΔp` sul gruppo tenuto (token equivalenti per punto). Un `k` per modello dove il modello fa almeno l'80% dell'intervallo, altrimenti quello dell'utenza. Servono almeno 3 intervalli coerenti: sotto, «stima non ancora disponibile» (`notCalibrated`).
3. **Stima.** Finestra corrente da `resetsAt − durata` (non prima del reset precedente) all'ultima lettura: punti locali `Σ U_m / k_m`, altri dispositivi `max(0, p_server − punti locali)`, anche in token equivalenti (`× k`).
4. **Fiducia.** Alta con almeno 8 intervalli e dispersione (MAD/mediana) fino a 0,2; media con almeno 5 e fino a 0,35; altrimenti bassa. Bassa anche se nella finestra il consumo di utenza sconosciuta supera il 10% (`historyGaps`).

Limiti: le percentuali sono intere (errore fino a un punto per lettura); un altro dispositivo che consuma sempre insieme a questo PC sposta la calibrazione verso il basso e la stima verso zero.

## Interfaccia

- Core: `usage::usage_breakdown(UsagePaths, UsageQuery, now)` → `UsageBreakdown`; `breakdown` è la parte pura.
- `UsageQuery`: `account?`, `window` (`5h` | `7d` | `custom` con `fromSeconds` e `toSeconds`), `groupBy` (`project` | `session` | `agent` | `model`). Con l'utenza scelta, 5h e 7d sono la finestra corrente del server; senza, finestre mobili.
- Righe ordinate per token equivalenti (al massimo 200, `truncatedRows`), con `sharePct` sul totale locale più gli altri dispositivi stimati (solo con l'utenza scelta e la stima disponibile), `lastUsed`; `unattributed` a parte; `otherDevices` per utenza e finestra; `scan` con le statistiche della lettura.
- Tauri `get_usage_breakdown`, CLI `swapdeck-cli usage breakdown`, `Backend.getUsageBreakdown` facoltativo.
