# SwapDeck per le IDE JetBrains

Plugin per IntelliJ IDEA, WebStorm, PyCharm e le altre IDE JetBrains (2024.3 e successive). Porta nell'IDE lo stesso pannello dell'estensione VS Code (`@swapdeck/ui` in una tool window JCEF).

Usa solo `packages/*` e `swapdeck-cli`, mai `apps/vscode` (controllo: `scripts/check-app-isolation.mjs`, in `pnpm lint`).

## Pannello (`webview/`)

| File                   | Cosa                                                                                                                   |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `webview/main.tsx`     | `HostApp` di `@swapdeck/host/webview` su `MessageBackend`, con il trasporto JCEF e la scheda ricordata                 |
| `webview/transport.ts` | trasporto dei messaggi con il plugin Kotlin (contratto sotto)                                                          |
| `webview/tabStore.ts`  | scheda aperta in `localStorage` (chiave `swapdeck.webview.state`, JSON `{"tab": …}`); errori ignorati                  |
| `webview/index.html`   | pagina servita dal plugin, con i segnaposto `__SWAPDECK_NONCE__` e `__SWAPDECK_LANG__`                                 |
| `scripts/build.mjs`    | esbuild (browser, IIFE, es2022) in `out/webview/`: `main.js`, `main.css` (con `@swapdeck/ui/styles.css`), `index.html` |

`pnpm build` (anche dalla radice) costruisce solo il pannello e non chiede Java.

### Contratto dei messaggi JS ↔ Kotlin

I messaggi sono quelli di `@swapdeck/host` (`WebviewMessage` dal pannello, `HostMessage` dall'host), in JSON.

- **Pannello → plugin:** `window.__swapdeckHost.post(json)`, dove `json` è una stringa con un `WebviewMessage`. Il plugin crea l'oggetto con `JBCefJSQuery` (la funzione `post` chiama il codice di `query.inject("json")`) e, appena iniettato, lancia `window.dispatchEvent(new Event('swapdeck-host-ready'))`.
  - Il bundle può partire prima dell'iniezione: i messaggi restano in coda e partono in ordine all'evento `swapdeck-host-ready` o al controllo periodico (ogni 100 ms) che trova `window.__swapdeckHost`.
  - Il primo messaggio è sempre `{"type":"ready"}`: il plugin risponde con `view` (lingua e tema), poi `state` ed `engine`.
  - Il pannello non è fidato: il plugin valida ogni messaggio con le regole di `protocol.json`.
- **Plugin → pannello:** `executeJavaScript` con
  `window.dispatchEvent(new CustomEvent('swapdeck-host', { detail: <stringa JSON> }))`.
  La stringa JSON va scritta come letterale JavaScript (per esempio `Json.encodeToString(String.serializer(), json)`), così nessun dato diventa codice. È accettato anche un oggetto già pronto. Un `detail` che non è un oggetto con `type` stringa si ignora.

### Pagina e indirizzi

Il plugin serve i file da un proprio indirizzo (`CefSchemeHandlerFactory`, sottotask 9), per esempio `https://swapdeck.plugin/`:

| Indirizzo     | Risorsa nel jar      | Note                                                                                                   |
| ------------- | -------------------- | ------------------------------------------------------------------------------------------------------ |
| `/index.html` | `webview/index.html` | sostituire `__SWAPDECK_NONCE__` (nonce casuale a ogni caricamento) e `__SWAPDECK_LANG__` (`it` o `en`) |
| `/main.js`    | `webview/main.js`    | `text/javascript`                                                                                      |
| `/main.css`   | `webview/main.css`   | `text/css`                                                                                             |

CSP della pagina: `default-src 'none'`, script solo con il nonce, stile, font e immagini solo dalla stessa origine (immagini anche `data:`), `base-uri` e `form-action` a `'none'`. Script e foglio di stile si caricano con percorsi relativi, quindi dalla stessa origine della pagina.

## Plugin Gradle

Progetto Gradle a sé (Kotlin DSL) dentro il monorepo. Serve un JDK 21; il wrapper scarica Gradle e la piattaforma.

| File                                                     | Cosa                                                                                                                                                                                                                                                                                                                                           |
| -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `settings.gradle.kts`, `build.gradle.kts`                | Kotlin JVM (target 21, `apiVersion` 2.0 come la libreria standard della 2024.3), kotlinx.serialization, IntelliJ Platform Gradle Plugin 2.x                                                                                                                                                                                                    |
| `gradle.properties`                                      | `pluginVersion` (uguale a `Cargo.toml`), piattaforma per compilare (IntelliJ IDEA Community 2024.3.x), `pluginSinceBuild` 243                                                                                                                                                                                                                  |
| `gradle/wrapper/`, `gradlew`, `gradlew.bat`              | wrapper Gradle                                                                                                                                                                                                                                                                                                                                 |
| `src/main/resources/META-INF/plugin.xml`                 | id `com.swapdeck.jetbrains`, dipendenza da `com.intellij.modules.platform`, Terminal facoltativo (`swapdeck-terminal.xml`: registra `TerminalCommandRunner` sul punto di estensione `com.swapdeck.jetbrains.commandRunner`), tool window, notifiche                                                                                            |
| `src/main/resources/messages/SwapDeckBundle*.properties` | testi in inglese e italiano (`SwapDeckBundle`)                                                                                                                                                                                                                                                                                                 |
| `src/main/kotlin/com/swapdeck/jetbrains/`                | `SwapDeckBundle`, `toolwindow/` (per ora un segnaposto), `protocol/HostProtocol` (legge `swapdeck/protocol.json` dal jar), `cli/` (`CliRunner`, `SwapDeckCli`), `settings/`, `engine/` (`EngineService`), `state/` (`StateService`), `sync/` (`SyncService`), `bridge/` (ponte col pannello, sotto), `terminal/` (solo con il plugin Terminal) |
| `src/test/kotlin/`                                       | JUnit 5: stesse chiavi nelle due lingue, `protocol.json` nel jar con i 33 metodi                                                                                                                                                                                                                                                               |

## Ponte col pannello (`bridge/`)

Porting di `bridge.ts` e della parte di `extension.ts` che risponde al pannello. Il pannello non è fidato: metodi, `kind` delle azioni, argomenti di `claude`, selettori, chiavi, versioni, codice di «Rifai login», testo da copiare, finestre e raggruppamenti di «chi consuma» e periodo dello storico vengono da `protocol.json` (`BridgeRules`). Un valore non valido risponde `invalidUsage` senza chiamare la CLI.

| Classe                        | Cosa                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `BridgeHandler`               | i 33 metodi (`handledMethods` = `requestMethods`, controllato da un test), stessi argomenti, stdin e timeout di VS Code; ogni richiesta su un thread del plugin. `runAction`: sempre `confirm - [--active N\|none]`, dialogo se serve (`purge`: anche la parola), `action -` (6 min), terminale e avvisi dell'esito, `afterAction`. Setup e «Rifai login» con pausa del motore e `progress`; un solo «Rifai login» per processo IDE, codice e annulla solo dal pannello che l'ha avviato |
| `BridgeSession`               | un pannello: `receive(json, send)` gestisce `{"type":"request","id",…}` e manda `progress` e `response` (`HostMessage`); gli altri messaggi (`ready`) restano al chiamante (`false`). `dispose` (pannello chiuso) annulla «Rifai login» (riga `{"cancel":true}`) e installazione o aggiornamento (processo terminato), poi non manda più nulla                                                                                                                                           |
| `BridgeHost`, `IdeBridgeHost` | stato, motore e sincronia: `StateService.afterAction` (con segnale e `configChanged`), `EngineService.pause`, `SyncService.signal`; `setPreferences` riuscito → `SwapDeckViewListener.preferencesChanged` (la tool window rilegge la lingua); `clientInfo` = `<IDE> <versione>, plugin <versione>` per `diagnostics --client`                                                                                                                                                            |
| `BridgeUi`, `IdeBridgeUi`     | dialoghi del pannello (uno per progetto), chiamati dai thread del plugin e portati all'EDT con `invokeAndWait`: conferme modali (`Messages`), `FileChooser`/`FileSaverDialog`, `CopyPasteManager`, `RevealFileAction.openDirectory`, notifiche d'avviso, comando di `run`                                                                                                                                                                                                                |
| `CommandRunner`               | punto di estensione per `run`: con il plugin Terminal `terminal/TerminalCommandRunner` apre una scheda «cswap run» col comando come processo (niente shell); senza, un dialogo mostra il comando con «Copia»                                                                                                                                                                                                                                                                             |
| `BridgeService`               | servizio dell'applicazione: `BridgeService.getInstance().openSession(IdeBridgeUi(project))`                                                                                                                                                                                                                                                                                                                                                                                              |

Nella build:

- `buildWebview` lancia `node scripts/build.mjs --production` (serve Node e `pnpm install` fatto); `processResources` mette `out/webview/{index.html,main.js,main.css}` in `webview/` e `../../packages/host/protocol.json` in `swapdeck/`, dentro il jar del plugin.
- `checkVersion` (in `check` e prima di `buildPlugin`) fallisce se `pluginVersion` è diversa dalla versione del `Cargo.toml`; lo stesso controllo è in `test/version.test.ts` (`pnpm test`).
- Servizi dell'applicazione (uno per processo IDE, anche con più progetti): `SwapDeckCli`, `EngineService`, `StateService`, `SyncService`, avviati da `SwapDeckStartup`. Avvisi sul bus dell'applicazione (`SwapDeckStateListener`, `SwapDeckEngineListener`, `SwapDeckNotificationListener`, `SwapDeckSettingsListener`) dai thread del plugin: chi aggiorna la UI passa all'EDT.
- kotlinx.serialization e la libreria standard di Kotlin le fornisce l'IDE: non entrano nello zip.

Comandi:

| Comando                                               | Cosa                                                                             |
| ----------------------------------------------------- | -------------------------------------------------------------------------------- |
| `pnpm build`, `pnpm test` (qui o dalla radice)        | solo il pannello e i test vitest: niente Java                                    |
| `pnpm jetbrains:build` (radice) o `pnpm gradle:build` | `./gradlew buildPlugin`: `build/distributions/swapdeck-jetbrains-<versione>.zip` |
| `pnpm jetbrains:test` (radice) o `pnpm gradle:test`   | `./gradlew test`                                                                 |

Lo zip contiene `swapdeck-jetbrains/lib/swapdeck-jetbrains-<versione>.jar` con classi, `plugin.xml`, testi, `swapdeck/protocol.json` e `webview/`. `swapdeck-cli` per piattaforma arriva con il sottotask 11.
