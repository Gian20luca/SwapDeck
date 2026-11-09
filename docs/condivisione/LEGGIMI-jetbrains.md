# SwapDeck per le IDE JetBrains: installazione

Plugin per IntelliJ IDEA, WebStorm, PyCharm, GoLand, Rider, CLion e le altre IDE JetBrains, **versione 2024.3 o successiva**. Porta nell'IDE lo stesso pannello dell'app e dell'estensione VS Code (dashboard, account, regole, storico, strumenti), un indicatore nella barra di stato, le azioni nel menu Strumenti e le notifiche.

Basta da solo: contiene `swapdeck-cli` e non richiede né l'app né l'estensione VS Code. Si può usare insieme a loro: la configurazione è la stessa e restano sincronizzati.

Per sapere cosa fa SwapDeck e come si usa, leggi **COME-FUNZIONA.md**.

## Quale zip

Uno zip per sistema operativo. **Non va estratto**: l'IDE lo installa così com'è.

| Sistema             | File                                             |
| ------------------- | ------------------------------------------------ |
| Windows (64 bit)    | `swapdeck-jetbrains-win32-x64-<versione>.zip`    |
| Linux (x86_64)      | `swapdeck-jetbrains-linux-x64-<versione>.zip`    |
| macOS Apple Silicon | `swapdeck-jetbrains-darwin-arm64-<versione>.zip` |
| macOS Intel         | `swapdeck-jetbrains-darwin-x64-<versione>.zip`   |

Lo zip di un altro sistema si installa lo stesso, ma il plugin non funziona (`swapdeck-cli` non parte). Su un Mac, «Informazioni su questo Mac» dice se il chip è Apple (M1, M2…) o Intel.

## Prima di iniziare

- **Claude Code** installato e già usato almeno una volta.
- Non serve installare `cswap` a mano: se manca, il pannello SwapDeck propone di installarlo (per utente, senza permessi di amministratore).
- Nessun permesso di amministratore: il plugin si installa nella cartella dell'IDE del tuo utente.

## 1. Installa da disco

1. Nell'IDE apri **Settings** (su macOS **Settings…** dal menu dell'IDE; scorciatoia Ctrl+Alt+S, su macOS ⌘,).
2. **Plugins** → icona a ingranaggio **⚙** in alto → **Install Plugin from Disk…**.
3. Scegli lo zip del tuo sistema e conferma.
4. Se l'IDE lo chiede, **Restart IDE**.

Con più IDE JetBrains il plugin va installato in ciascuna.

## 2. Primo avvio

- A destra compare la finestra **SwapDeck** (icona ad anello): è il pannello completo. Se manca: **View** → **Tool Windows** → **SwapDeck**.
- Nella barra di stato in basso c'è l'account attivo con i consumi delle 5 ore e dei 7 giorni; il colore diventa giallo o rosso vicino ai limiti. Un clic apre il pannello.
- Se `cswap` non c'è, il pannello propone l'installazione: conferma e aspetta qualche minuto.
- Le azioni sono anche in **Tools** → **SwapDeck** e in **Find Action** (Ctrl+Shift+A, su macOS ⇧⌘A) cercando «SwapDeck»: Aggiorna, Cambia account, Cambia mappa, Avvia e Ferma motore, Copia diagnostica, Apri cartella dei log.
- Impostazioni del plugin: **Settings** → **Tools** → **SwapDeck** (intervallo di lettura, percorso di `swapdeck-cli`, motore, soglie della barra di stato).

Il **motore** che cambia account da solo è spento finché non lo accendi dalla scheda Regole del pannello (o con «SwapDeck: Avvia motore»): la prima volta chiede conferma e propone **Solo osservazione**. C'è un solo motore per utente fra app, finestre di VS Code e IDE JetBrains: gli altri osservano.

I testi del plugin seguono la lingua scelta in SwapDeck (Impostazioni del pannello); i nomi dei menu dell'IDE seguono la lingua dell'IDE.

## 3. macOS: blocco di `swapdeck-cli`

`swapdeck-cli` dentro lo zip non è firmato con un certificato Apple. Di solito parte senza avvisi, perché lo estrae l'IDE. Se invece il pannello o la barra di stato segnalano un errore di `swapdeck-cli`, oppure macOS mostra un avviso su `swapdeck-cli` di «sviluppatore non identificato», togli la quarantena dalla cartella del plugin e riavvia l'IDE:

```bash
xattr -dr com.apple.quarantine ~/Library/Application\ Support/JetBrains/*/plugins/swapdeck-jetbrains
```

In alternativa: Impostazioni di Sistema → Privacy e sicurezza → «Apri comunque» per `swapdeck-cli`.

## 4. Problemi comuni

| Cosa succede                                | Cosa fare                                                                                                                                                                                                          |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| «Plugin is incompatible» all'installazione  | L'IDE è più vecchia della 2024.3: aggiornala.                                                                                                                                                                      |
| Il pannello dice che JCEF non è disponibile | L'IDE gira senza JCEF (runtime alternativo). Barra di stato, azioni e notifiche funzionano lo stesso; per il pannello usa il runtime JetBrains predefinito (Find Action → «Choose Boot Java Runtime for the IDE»). |
| Errore di `swapdeck-cli`                    | Zip di un altro sistema: installa quello giusto. Su macOS vedi il punto 3.                                                                                                                                         |
| Qualcosa non va e non si capisce perché     | **Tools** → **SwapDeck** → **Copia diagnostica** e incolla il testo a chi ti ha dato SwapDeck.                                                                                                                     |

## 5. Aggiornare

Installa da disco lo zip della nuova versione (punto 1): sostituisce quella vecchia. Configurazione, storico e account restano.

## 6. Disinstallare

**Settings** → **Plugins** → scheda **Installed** → **SwapDeck** → freccia accanto a «Disable» → **Uninstall**, poi **Restart IDE**.

Restano la configurazione e lo storico di SwapDeck (condivisi con app ed estensione) e `cswap` con i tuoi account: il plugin non li tocca.
