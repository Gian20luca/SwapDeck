# SwapDeck 0.2.3 per Windows: installazione

Non servono permessi di amministratore: tutto si installa solo per il tuo utente. Per sapere cosa fa l'app e come si usa, leggi **COME-FUNZIONA.md** in questo stesso zip.

## Cosa c'è nello zip

| File                                   | A cosa serve                                               |
| -------------------------------------- | ---------------------------------------------------------- |
| `SwapDeck_0.2.3_x64-setup.exe`         | Installer (consigliato)                                    |
| `portatile\`                           | Versione portatile, da usare solo se l'installer non parte |
| `swapdeck-vscode-win32-x64-0.2.3.vsix` | Estensione per VS Code (facoltativa)                       |
| `LEGGIMI.md`, `COME-FUNZIONA.md`       | Queste guide                                               |

## Prima di iniziare

- Windows 10 o 11 e una connessione a internet.
- **Claude Code** installato e già usato almeno una volta (terminale o estensione di VS Code).
- Non serve installare `cswap` a mano: se manca, SwapDeck lo installa al primo avvio.

## 1. Estrai lo zip

Tasto destro sullo zip → **Estrai tutto…** → scegli una cartella, per esempio Documenti. Non avviare i file direttamente dall'anteprima dello zip.

## 2. Installa l'app

1. Doppio clic su `SwapDeck_0.2.3_x64-setup.exe`.
2. Windows mostra **«Windows ha protetto il PC»**, perché l'installer non è firmato con un certificato commerciale. Clicca **Ulteriori informazioni** → **Esegui comunque**.
3. Segui l'installazione fino alla fine. Non chiede password né permessi di amministratore; si installa in `%LOCALAPPDATA%\SwapDeck`.
4. Alla fine SwapDeck parte da solo e compare un'**icona ad anello** nell'area di notifica, vicino all'orologio. Se non la vedi, clicca la freccia **^** accanto all'orologio: puoi trascinarla sulla barra.

### Se l'installer non parte: versione portatile

1. Copia la cartella `portatile` in un posto fisso, per esempio `%LOCALAPPDATA%\SwapDeck-portatile` (incollalo nella barra degli indirizzi di Esplora file). Non lasciarla in Download.
2. Avvia `swapdeck.exe` da lì. I file della cartella devono restare tutti insieme.
3. Se poi sposti la cartella, riavvia `swapdeck.exe` dalla nuova posizione: l'avvio automatico si aggiorna da solo.

Se Windows blocca anche la portatile, il PC ha una regola aziendale che impedisce i programmi non firmati: serve l'IT.

## 3. Primo avvio: cswap

1. Se `cswap` non c'è, SwapDeck propone di installarlo. Conferma: installa `uv` e `claude-swap` nella tua cartella utente (`%USERPROFILE%\.local\bin`), senza amministratore. Ci vuole qualche minuto.
2. Se l'installazione fallisce, di solito è un proxy aziendale o PowerShell bloccato. Copia il messaggio d'errore e chiedi a chi ti ha dato SwapDeck.

## 4. Aggiungi le tue utenze

Per ogni account Claude:

1. In Claude Code scrivi `/login` e accedi con quell'account.
2. In SwapDeck premi **Aggiungi account**.

Il resto (dashboard, cambio di utenza, Rifai login, motore, massimo per utenza) è spiegato in **COME-FUNZIONA.md**.

## 5. Estensione per VS Code (facoltativa)

1. In VS Code apri **Estensioni** (Ctrl+Shift+X) → menu **…** in alto → **Installa da VSIX…** → scegli `swapdeck-vscode-win32-x64-0.2.3.vsix`.
2. Command Palette (Ctrl+Shift+P) → **Developer: Reload Window**.
3. Compare l'icona SwapDeck nella barra laterale e una voce in basso nella barra di stato.

L'estensione funziona anche senza l'app. Se le usi entrambe, il cambio automatico lo fa una sola delle due per volta.

## Aggiornare

Le nuove versioni non arrivano da sole: quando ricevi un nuovo installer, eseguilo sopra quello vecchio. Impostazioni, storico e utenze restano. Per l'estensione, installa il nuovo `.vsix` allo stesso modo e ricarica la finestra.

## Disinstallare

**Impostazioni → App → App installate → SwapDeck → Disinstalla.** «Cancella i dati dell'applicazione» toglie solo la cache della finestra. Impostazioni e storico restano in `%APPDATA%\swapdeck`: se vuoi eliminarli, cancella quella cartella a mano. Le utenze salvate in cswap e il login di Claude Code non vengono mai toccati.

Per la portatile: chiudi l'app con **Esci** dal menu dell'icona e cancella la cartella.
