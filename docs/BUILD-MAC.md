# Build e installazione di SwapDeck 0.2.2 su macOS

Guida per un agente Claude Code su un Mac. Costruisce tutto in locale dai sorgenti (la CI di GitHub è ferma). Pacchetti **non firmati** e **non notarizzati**; updater spento. Fonti: `.github/workflows/release.yml` (job `app` e `vsix`), `apps/vscode/scripts/package.mjs`, `README.md`.

Convenzioni: ogni passo ha una verifica; se fallisce non andare avanti, leggi solo le ultime 30 righe dell'errore e vedi «Se fallisce». Tutti i comandi in bash/zsh dalla radice del repo, salvo diversa indicazione.

## 0. Regole per l'agente (leggere prima)

- **Motore delle regole: non accenderlo** finché il collega non lo decide a voce. Cambia account di `cswap` per tutte le sessioni Claude del Mac.
- **Sul `cswap` reale solo comandi in lettura** (per esempio `cswap list`, sempre con sottocomando esplicito e pipe; senza argomenti parte la TUI). Mai switch, add, remove, disable/enable, alias, swap, move, map/unmap, config set, auto, import, purge, upgrade.
- Prove dell'app con home isolata e finto cswap: `SWAPDECK_HOME=<cartella temporanea>` e `SWAPDECK_CSWAP=<finto cswap>` (vedi `apps/desktop/README.md`, «Prove locali senza toccare il cswap reale»). Nella cartella di prova metti `config/config.json` con `"autostart": false`.
- Nessun segreto, token o email completa in log, messaggi o file. Non incollare il contenuto di `~/.claude` o di file di credenziali.
- Niente `sudo`. Se un passo lo chiede, fermati e chiedi al collega.

## 1. Prerequisiti

1. Architettura del Mac e target Rust (`uname -m`: `arm64` = Apple Silicon, `x86_64` = Intel):

```bash
if [ "$(uname -m)" = arm64 ]; then
  export TARGET=aarch64-apple-darwin VSTARGET=darwin-arm64
else
  export TARGET=x86_64-apple-darwin VSTARGET=darwin-x64
fi
echo "$TARGET $VSTARGET"
```

Le variabili valgono solo nella shell corrente: rilanciale se apri un nuovo terminale.

2. Xcode Command Line Tools (non serve Xcode completo):

```bash
xcode-select -p || xcode-select --install   # apre una finestra di sistema: lascia finire l'installazione
clang --version | head -1
```

3. Rust via rustup (per utente, senza admin). Minimo 1.85 (`rust-version` in `Cargo.toml`); la CI usa `stable`:

```bash
command -v rustup || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
rustup default stable
rustup target add "$TARGET"
rustc --version && cargo --version
```

4. Node >= 22.10 (la CI usa 22) e pnpm 10.28.1 (`packageManager` in `package.json`):

```bash
node --version    # >= v22.10
corepack enable; corepack prepare pnpm@10.28.1 --activate
pnpm --version    # 10.28.1
```

Se Node manca o è vecchio: nvm per utente (`nvm install 22`) o Homebrew se già presente. Se `corepack enable` dà errore di permessi: `npm install -g pnpm@10.28.1 --prefix "$HOME/.local"` e aggiungi `$HOME/.local/bin` al PATH.

5. VS Code con il comando `code` nel PATH (Palette comandi: «Shell Command: Install 'code' command in PATH»). Verifica: `code --version`.

6. `cswap` (claude-swap) 0.26.0 o successivo: **opzionale**, solo se il collega vuole usare SwapDeck davvero. Al primo avvio SwapDeck propone da solo di installare `uv` e `claude-swap` per utente, con conferma: non farlo tu senza chiedere. Verifica in sola lettura: `command -v cswap`.

Se fallisce: `rustup target add` senza rete, controlla proxy/VPN; `clang` assente, riesegui `xcode-select --install`.

## 2. Clone e dipendenze

```bash
git clone https://github.com/Gian20luca/SwapDeck.git && cd SwapDeck   # repo privato: serve l'accesso del collega (gh auth login o credenziali git)
grep -m1 '^version' Cargo.toml                                         # atteso: version = "0.2.2"
pnpm install --frozen-lockfile
```

Verifica: `pnpm install` termina senza errori. Se `--frozen-lockfile` fallisce, controlla `pnpm --version` (passo 1.4); non rigenerare il lockfile.

## 3. `swapdeck-cli` e sidecar Tauri

Tauri si aspetta il sidecar come `apps/desktop/src-tauri/binaries/swapdeck-cli-<TARGET>` (cartella in `.gitignore`). Stessi comandi della CI:

```bash
cargo build --release --locked -p swapdeck-cli --bin swapdeck-cli --target "$TARGET"
mkdir -p apps/desktop/src-tauri/binaries out/cli
cp "target/$TARGET/release/swapdeck-cli" "apps/desktop/src-tauri/binaries/swapdeck-cli-$TARGET"
cp "target/$TARGET/release/swapdeck-cli" "out/cli/swapdeck-cli-$TARGET"
chmod +x "out/cli/swapdeck-cli-$TARGET"
```

Verifica. Non c'è `--version`: il sottocomando è `version` e risponde in JSON.

```bash
"target/$TARGET/release/swapdeck-cli" version   # atteso: {"name":"swapdeck-cli","version":"0.2.2",...}
```

Se fallisce: errori di linker, CLT incompleti (passo 1.2). La prima compilazione richiede alcuni minuti. Non provare altri sottocomandi (`status`, `action`, ...): parlano col `cswap` reale.

## 4. App: `.app` e `.dmg`

Come la CI: si genera `tauri.ci.conf.json` da `tauri.release.conf.json` (con `externalBin`, perché il sidecar c'è) e poi `tauri build`:

```bash
node -e '
  const fs = require("fs");
  const c = JSON.parse(fs.readFileSync("apps/desktop/src-tauri/tauri.release.conf.json", "utf8"));
  fs.writeFileSync("apps/desktop/src-tauri/tauri.ci.conf.json", JSON.stringify(c));
'
pnpm --filter @swapdeck/desktop exec tauri build --ci --target "$TARGET" \
  --bundles app,dmg --config src-tauri/tauri.ci.conf.json
```

`createUpdaterArtifacts` è `false` in quella config: nessuna chiave di firma e nessuna variabile `TAURI_SIGNING_*`. `beforeBuildCommand` esegue `pnpm build`.

Verifica:

```bash
B="target/$TARGET/release/bundle"
ls "$B"/dmg/*.dmg "$B/macos/SwapDeck.app/Contents/MacOS/"   # attesi: SwapDeck_0.2.2_*.dmg; swapdeck e swapdeck-cli
```

Se fallisce: la prima build è lunga (10-30 minuti). Il `.dmg` può fallire se il Terminale non ha il permesso di automazione (Impostazioni di Sistema, Privacy e sicurezza, Automazione), poi rilancia. Se manca `swapdeck-cli` in `Contents/MacOS/`, il passo 3 non è stato fatto per lo stesso `TARGET`.

### Installazione e primo avvio (app non firmata)

```bash
cp -R "$B/macos/SwapDeck.app" /Applications/    # o ~/Applications se /Applications non è scrivibile
```

Gatekeeper: l'app ha solo una firma ad-hoc (`signingIdentity: "-"`), senza certificato Apple né notarizzazione, quindi è di «sviluppatore non identificato». Rischio: macOS non può verificare l'autore, quindi apri solo build fatte da te da questo repo. Costruita e aperta sullo stesso Mac non ha la quarantena e parte senza avvisi. Se arriva da un `.dmg` scaricato o da un altro Mac:

- apri l'app una volta (macOS la blocca), poi Impostazioni di Sistema → Privacy e sicurezza → «Apri comunque» e conferma. Da macOS 15 il clic destro → «Apri» non basta più;
- oppure togli la quarantena:

```bash
xattr -dr com.apple.quarantine /Applications/SwapDeck.app
```

Attenzione al profilo reale: l'app parte da sola al login (`autostart`) e cerca `cswap`; il motore delle regole resta spento finché il collega non lo accende (passo 0). Per una prima prova sicura usa una home isolata:

```bash
export SWAPDECK_HOME="$(mktemp -d)"; mkdir -p "$SWAPDECK_HOME/config"
echo '{ "autostart": false }' > "$SWAPDECK_HOME/config/config.json"
SWAPDECK_HOME="$SWAPDECK_HOME" /Applications/SwapDeck.app/Contents/MacOS/swapdeck &
```

## 5. Estensione VS Code (`.vsix` darwin)

Riusa il `swapdeck-cli` del passo 3:

```bash
node apps/vscode/scripts/package.mjs --target "$VSTARGET" --cli "out/cli/swapdeck-cli-$TARGET"
ls apps/vscode/dist/*.vsix    # swapdeck-vscode-<VSTARGET>-0.2.2.vsix
code --install-extension "apps/vscode/dist/swapdeck-vscode-$VSTARGET-0.2.2.vsix"
```

Poi in VS Code: «Developer: Reload Window». L'estensione contiene già `swapdeck-cli` e non richiede l'app.

Se fallisce: `code: command not found`, vedi passo 1.5, oppure «Estensioni», «...», «Installa da VSIX...». Se `package.mjs` non trova `vsce`, verifica che `pnpm install` (passo 2) sia finito.

## 6. Verifiche finali

```bash
grep -m1 '^version' Cargo.toml                                                                                # 0.2.2
"target/$TARGET/release/swapdeck-cli" version                                                                 # "version":"0.2.2"
/Applications/SwapDeck.app/Contents/MacOS/swapdeck-cli version                                                # sidecar nell'app, stesso JSON
/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' /Applications/SwapDeck.app/Contents/Info.plist  # 0.2.2
code --list-extensions --show-versions | grep -i swapdeck                                                     # ...@0.2.2
```

- App: si apre (finestra o icona nella barra dei menu) senza errori; con la home isolata del passo 4 non tocca il profilo reale.
- Estensione: icona SwapDeck nella barra delle attività di VS Code dopo il reload.
- Il motore resta spento. Riferisci al collega l'esito di ogni riga, senza incollare dati di account.

## 7. Aggiornamenti futuri

Updater spento (repo privato, build non firmate): si aggiorna a mano.

```bash
git pull --ff-only
pnpm install --frozen-lockfile
```

Poi rifai i passi 3, 4 e 5 (stessi comandi; chiudi prima l'app e sostituisci `/Applications/SwapDeck.app`). Se la versione cambia, aggiorna `0.2.2` nei nomi di `.dmg` e `.vsix`. L'aggiornamento non tocca il `cswap` né la config dell'utente.

## Da verificare

- Passi 1-5 provati il 22/10 su un Mac Apple Silicon (macOS 26.3) con la 0.2.2: `.dmg`, `.app` con `swapdeck-cli` e `.vsix` darwin-arm64 costruiti. Non provati: Mac Intel, installazione in `/Applications` e primo avvio da un `.dmg` scaricato (vedi `docs/PROVE-REALI.md` §6).
- Nome del `.dmg`: `SwapDeck_0.2.2_aarch64.dmg` su Apple Silicon; su Intel atteso `SwapDeck_0.2.2_x64.dmg`. La voce dell'estensione è `swapdeck.swapdeck-vscode`.

## Test sul Mac

Come su Windows (`CLAUDE.md`, «Comandi»), più il browser per gli E2E, una volta sola:

```bash
pnpm --filter @swapdeck/ui exec playwright install chromium
```

I test usano solo il finto cswap. Fino alla 0.2.2 compresa `crates/swapdeck-core/tests/setup.rs` trovava lo uv reale di Homebrew (`/opt/homebrew/bin/uv`) e lanciava `uv tool upgrade claude-swap`: con un checkout più vecchio non eseguire `cargo test` sul Mac senza escludere quel file (`cargo test --workspace -- --skip upgrade_busy_and_without_uv`).
