# SwapDeck 0.2.4 per Linux: installazione

Per sapere cosa fa l'app e come si usa, leggi **COME-FUNZIONA.md** in questo stesso zip.

## Cosa c'è nello zip

| File                                   | A cosa serve                                                         |
| -------------------------------------- | -------------------------------------------------------------------- |
| `SwapDeck_0.2.4_amd64.AppImage`        | App per qualsiasi distribuzione, senza installare e senza `sudo`     |
| `SwapDeck_0.2.4_amd64.deb`             | Pacchetto per Ubuntu e Debian (richiede `sudo`)                      |
| `swapdeck-vscode-linux-x64-0.2.4.vsix` | Estensione per VS Code (facoltativa)                                 |
| `swapdeck-cli-linux-x64`               | Solo riga di comando, per chi la vuole usare da script (facoltativa) |
| `LEGGIMI.md`, `COME-FUNZIONA.md`       | Queste guide                                                         |

Scegli **una** tra AppImage e `.deb`. Senza `sudo` usa l'AppImage.

## Prima di iniziare

- Linux a 64 bit (x86_64) con un desktop grafico. Costruita per Ubuntu 22.04 e successive.
- **Claude Code** installato e già usato almeno una volta.
- Non serve installare `cswap` a mano: se manca, SwapDeck lo installa al primo avvio.
- **Icona nell'area di notifica:** su GNOME serve l'estensione «AppIndicator» (Ubuntu standard la ha già); su KDE, Xfce e Cinnamon c'è già. Senza, l'app funziona lo stesso ma l'icona non si vede: usa la finestra o il mini-widget.

## 1. Estrai lo zip

```bash
unzip SwapDeck-0.2.4-linux.zip -d ~
cd ~/SwapDeck-0.2.4-linux
```

Lo zip non conserva il permesso di esecuzione: nei passi sotto c'è `chmod +x`.

## 2a. Installa con l'AppImage (consigliato, senza sudo)

```bash
mkdir -p ~/Applicazioni
mv SwapDeck_0.2.4_amd64.AppImage ~/Applicazioni/
chmod +x ~/Applicazioni/SwapDeck_0.2.4_amd64.AppImage
~/Applicazioni/SwapDeck_0.2.4_amd64.AppImage
```

Tienila in una cartella fissa: l'avvio automatico punta a quel percorso. Se poi la sposti, avviala una volta dalla nuova posizione.

Se non parte con un errore su **FUSE**, installa `libfuse2` (su Ubuntu 22.04: `sudo apt install libfuse2`, su 24.04: `sudo apt install libfuse2t64`). Senza `sudo` puoi avviarla con `--appimage-extract-and-run`.

## 2b. Installa con il .deb (Ubuntu e Debian)

```bash
sudo apt install ./SwapDeck_0.2.4_amd64.deb
```

`apt` installa anche le dipendenze mancanti. Poi avvia **SwapDeck** dal menu delle applicazioni oppure con `swapdeck`. Il pacchetto contiene già anche `swapdeck-cli`.

## 3. Primo avvio: cswap

1. Se `cswap` non c'è, SwapDeck propone di installarlo. Conferma: installa `uv` e `claude-swap` in `~/.local/bin`, senza `sudo`. Ci vuole qualche minuto.
2. Se fallisce, di solito è un proxy o `curl` mancante. Copia il messaggio d'errore e chiedi a chi ti ha dato SwapDeck.

L'app si avvia da sola al login nel desktop.

## 4. Aggiungi le tue utenze

Per ogni account Claude:

1. In Claude Code scrivi `/login` e accedi con quell'account.
2. In SwapDeck premi **Aggiungi account**.

Il resto (dashboard, cambio di utenza, Rifai login, motore, massimo per utenza) è spiegato in **COME-FUNZIONA.md**.

## 5. Estensione per VS Code (facoltativa)

```bash
code --install-extension swapdeck-vscode-linux-x64-0.2.4.vsix
```

oppure in VS Code: **Estensioni** → menu **…** → **Installa da VSIX…**. Poi Command Palette (Ctrl+Shift+P) → **Developer: Reload Window**.

## 6. Riga di comando (facoltativa, solo con l'AppImage)

```bash
mkdir -p ~/.local/bin
mv swapdeck-cli-linux-x64 ~/.local/bin/swapdeck-cli
chmod +x ~/.local/bin/swapdeck-cli
swapdeck-cli --help
```

## Aggiornare

Le nuove versioni non arrivano da sole. AppImage: sostituisci il file con quello nuovo nella stessa cartella e rendilo eseguibile. `.deb`: `sudo apt install ./<nuovo>.deb`. Impostazioni, storico e utenze restano.

## Disinstallare

- AppImage: chiudi l'app con **Esci** e cancella il file. Togli anche `~/.config/autostart/SwapDeck.desktop` se resta.
- `.deb`: `sudo apt remove swap-deck`.

Impostazioni e storico restano in `~/.config/swapdeck` e `~/.local/share/swapdeck`: cancellali a mano se vuoi, ma solo se non usi SwapDeck anche da VS Code o da una IDE JetBrains (sono condivisi). Le utenze di cswap e il login di Claude Code non vengono mai toccati.

### Togliere anche cswap e uv

Al primo avvio SwapDeck può aver installato **uv** e **cswap** (pacchetto `claude-swap`) per il tuo utente. Toglili solo se non li usi fuori da SwapDeck (né da VS Code o dalle IDE JetBrains):

1. **cswap:** `uv tool uninstall claude-swap`. Toglie il comando `cswap`; le utenze salvate restano.
2. **Utenze salvate in cswap** (facoltativo, **irreversibile**): cancella la cartella `~/.local/share/claude-swap`, che contiene i login salvati di tutte le utenze. Il login attivo di Claude Code resta quello dell'ultima utenza usata.
3. **uv** (facoltativo, solo se l'ha installato SwapDeck e non lo usi per altro: toglie anche tutti gli altri strumenti e i Python di uv):
   ```bash
   uv cache clean
   rm -r "$(uv python dir)" "$(uv tool dir)"
   rm ~/.local/bin/uv ~/.local/bin/uvx
   ```
