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

Il plugin serve i file dall'indirizzo `https://swapdeck.plugin/` (`toolwindow/PanelAssets`), senza rete: il `CefRequestHandler` del browser del pannello (`getResourceRequestHandler`) risponde per quell'origine con i file del jar.

| Indirizzo     | Risorsa nel jar      | Note                                                                                                   |
| ------------- | -------------------- | ------------------------------------------------------------------------------------------------------ |
| `/index.html` | `webview/index.html` | sostituire `__SWAPDECK_NONCE__` (nonce casuale a ogni caricamento) e `__SWAPDECK_LANG__` (`it` o `en`) |
| `/main.js`    | `webview/main.js`    | `text/javascript`                                                                                      |
| `/main.css`   | `webview/main.css`   | `text/css`                                                                                             |

Tutto il resto dell'origine è 404 (`text/plain`); risposte con `Cache-Control: no-store` e `X-Content-Type-Options: nosniff`.

CSP della pagina: `default-src 'none'`, script solo con il nonce, stile, font e immagini solo dalla stessa origine (immagini anche `data:`), `base-uri` e `form-action` a `'none'`. Script e foglio di stile si caricano con percorsi relativi, quindi dalla stessa origine della pagina.

## Tool window (`toolwindow/`)

| Classe                                       | Cosa                                                                                                                                                                                                                                                                                                                                                                                                    |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SwapDeckToolWindowFactory`                  | avvia `StateService` (su un thread in background: dopo l'installazione l'avvio dei progetti già aperti non gira); con `JBCefApp.isSupported()` un `SwapDeckPanel` per contenuto, smaltito col contenuto; senza JCEF un messaggio (barra di stato, azioni e notifiche restano)                                                                                                                           |
| `SwapDeckPanel`                              | `JBCefBrowser` (DevTools solo con `isInternal`), `JBCefJSQuery` creata prima del caricamento, `BridgeSession`. Navigazione solo nell'origine del pannello (`onBeforeBrowse`, `onOpenURLFromTab`): `http`/`https` con un gesto dell'utente si apre con `BrowserUtil`, il resto si blocca; popup bloccati. A ogni `onLoadEnd` della pagina inietta `window.__swapdeckHost` e lancia `swapdeck-host-ready` |
| `PanelAssets`                                | file serviti, nonce (128 bit, base64) e lingua nella pagina, origine permessa, link esterni                                                                                                                                                                                                                                                                                                             |
| `PanelMessages`                              | `view`, `state` (solo con `list`), `engine` (`suspended` da `engine.suspended`/`suspendDetail`, altrimenti `null`), `configChanged`; script di consegna e di iniezione; `jsStringLiteral` (escape di apici, `\`, controlli, `<`, `>`, `&`, U+2028/2029)                                                                                                                                                 |
| `PanelConnection`                            | instradamento senza IDE: testo del pannello → `BridgeSession.receive`; `ready` → `view`, `state`, `engine`. Prima del `ready` (anche dopo un nuovo caricamento) i messaggi si scartano: il `ready` arriva dopo l'iniezione e porta lo stato attuale. Dopo `close` niente `executeJavaScript`                                                                                                            |
| `ViewSettingsTracker`, `ViewSettingsService` | lingua dalle preferenze di SwapDeck (`preferences get` → `language`), altrimenti dall'IDE (`DynamicBundle.getLocale()`); tema da `JBColor.isBright()`. Si rilegge a `preferencesChanged` e `configChanged`, il tema a `LafManagerListener`; `view` ai pannelli solo se cambia                                                                                                                           |

Messaggi dal bus a ogni pannello pronto: `stateChanged` → `state` ed `engine` (come VS Code), `engineChanged` → `engine`, `configChanged` → `configChanged`.

Chiudere il contenuto smaltisce `SwapDeckPanel`: abbonamenti staccati, `PanelConnection.close`, `BridgeSession.dispose` (annulla «Rifai login»), poi `JBCefJSQuery` e browser.

Perché un gestore delle richieste del browser e non `CefApp.registerSchemeHandlerFactory`: vale solo per quel browser e se ne va con lui (niente registrazione globale che resta dopo lo scaricamento del plugin, mentre `clearSchemeHandlerFactories` toglierebbe anche quelle degli altri plugin); uno schema proprio andrebbe registrato prima dell'avvio di CEF (API non pubblica); con `https` la pagina è un contesto sicuro con un'origine stabile, quindi `localStorage` funziona.

## Plugin Gradle

Progetto Gradle a sé (Kotlin DSL) dentro il monorepo. Serve un JDK 21; il wrapper scarica Gradle e la piattaforma.

| File                                                     | Cosa                                                                                                                                                                                                                                                                                                                                          |
| -------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `settings.gradle.kts`, `build.gradle.kts`                | Kotlin JVM (target 21, `apiVersion` 2.0 come la libreria standard della 2024.3), kotlinx.serialization, IntelliJ Platform Gradle Plugin 2.x                                                                                                                                                                                                   |
| `gradle.properties`                                      | `pluginVersion` (uguale a `Cargo.toml`), piattaforma per compilare (IntelliJ IDEA Community 2024.3.x), `pluginSinceBuild` 243                                                                                                                                                                                                                 |
| `gradle/wrapper/`, `gradlew`, `gradlew.bat`              | wrapper Gradle                                                                                                                                                                                                                                                                                                                                |
| `src/main/resources/META-INF/plugin.xml`                 | id `com.swapdeck.jetbrains`, dipendenza da `com.intellij.modules.platform`, Terminal facoltativo (`swapdeck-terminal.xml`: registra `TerminalCommandRunner` sul punto di estensione `com.swapdeck.jetbrains.commandRunner`), tool window, notifiche                                                                                           |
| `src/main/resources/messages/SwapDeckBundle*.properties` | testi in inglese e italiano (`SwapDeckBundle`)                                                                                                                                                                                                                                                                                                |
| `src/main/kotlin/com/swapdeck/jetbrains/`                | `SwapDeckBundle`, `toolwindow/` (pannello JCEF, sotto), `protocol/HostProtocol` (legge `swapdeck/protocol.json` dal jar), `cli/` (`CliRunner`, `SwapDeckCli`), `settings/`, `engine/` (`EngineService`), `state/` (`StateService`), `sync/` (`SyncService`), `bridge/` (ponte col pannello, sotto), `terminal/` (solo con il plugin Terminal) |
| `src/test/kotlin/`                                       | JUnit 5: stesse chiavi nelle due lingue, `protocol.json` nel jar con i 33 metodi                                                                                                                                                                                                                                                              |

## Ponte col pannello (`bridge/`)

Porting di `bridge.ts` e della parte di `extension.ts` che risponde al pannello. Il pannello non è fidato: metodi, `kind` delle azioni, argomenti di `claude`, selettori, chiavi, versioni, codice di «Rifai login», testo da copiare, finestre e raggruppamenti di «chi consuma» e periodo dello storico vengono da `protocol.json` (`BridgeRules`). Un valore non valido risponde `invalidUsage` senza chiamare la CLI.

| Classe                        | Cosa                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ----------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `BridgeHandler`               | i 33 metodi (`handledMethods` = `requestMethods`, controllato da un test), stessi argomenti, stdin e timeout di VS Code; ogni richiesta su un thread del plugin. `runAction`: sempre `confirm - [--active N\|none]`, dialogo se serve (`purge`: anche la parola), `action -` (6 min), terminale e avvisi dell'esito, `afterAction`. Setup e «Rifai login» con pausa del motore (`EnginePause`: vale subito, `resume` sempre nel `finally`, anche se l'attesa dell'arresto fallisce) e `progress`; un solo «Rifai login» per processo IDE, codice e annulla solo dal pannello che l'ha avviato |
| `BridgeSession`               | un pannello: `receive(json, send)` gestisce `{"type":"request","id",…}` e manda `progress` e `response` (`HostMessage`); gli altri messaggi (`ready`) restano al chiamante (`false`). `dispose` (pannello chiuso) annulla «Rifai login» (riga `{"cancel":true}`), poi non manda più nulla; installazione e aggiornamento di cswap non si interrompono (fermare `uv tool upgrade` a metà può lasciare cswap a metà, come in VS Code): arrivano in fondo senza avanzamento e il motore riparte                                                                                                  |
| `BridgeHost`, `IdeBridgeHost` | stato, motore e sincronia: `StateService.afterAction` (con segnale e `configChanged`), `EngineService.pause`, `SyncService.signal`; `setPreferences` riuscito → `SwapDeckViewListener.preferencesChanged` (la tool window rilegge la lingua); `clientInfo` = `<IDE> <versione>, plugin <versione>` per `diagnostics --client`                                                                                                                                                                                                                                                                 |
| `BridgeUi`, `IdeBridgeUi`     | dialoghi del pannello (uno per progetto), chiamati dai thread del plugin e portati all'EDT con `invokeAndWait`: conferme modali (`Messages`), `FileChooser`/`FileSaverDialog`, `CopyPasteManager`, `RevealFileAction.openDirectory`, notifiche d'avviso, comando di `run`                                                                                                                                                                                                                                                                                                                     |
| `CommandRunner`               | punto di estensione per `run`: con il plugin Terminal `terminal/TerminalCommandRunner` apre una scheda «cswap run» col comando come processo (niente shell); senza, un dialogo mostra il comando con «Copia»                                                                                                                                                                                                                                                                                                                                                                                  |
| `BridgeService`               | servizio dell'applicazione: `BridgeService.getInstance().openSession(IdeBridgeUi(project))`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |

Nella build:

- `buildWebview` lancia `node scripts/build.mjs --production` (serve Node e `pnpm install` fatto); `processResources` mette `out/webview/{index.html,main.js,main.css}` in `webview/` e `../../packages/host/protocol.json` in `swapdeck/`, dentro il jar del plugin.
- `checkVersion` (in `check` e prima di `buildPlugin`) fallisce se `pluginVersion` è diversa dalla versione del `Cargo.toml`; lo stesso controllo è in `test/version.test.ts` (`pnpm test`).
- Servizi dell'applicazione (uno per processo IDE, anche con più progetti): `SwapDeckCli`, `EngineService`, `StateService`, `SyncService`, avviati da `SwapDeckStartup`. Ognuno chiede per primo `SwapDeckLifecycle`, che nasce prima e si smaltisce per ultimo: chiude i pool di `SwapDeckThreads` dopo che i servizi si sono fermati (`SwapDeckCli` annulla i comandi in corso), così il plugin si può scaricare. Dopo lo smaltimento nessun servizio programma altri lavori. Avvisi sul bus dell'applicazione (`SwapDeckStateListener`, `SwapDeckEngineListener`, `SwapDeckNotificationListener`, `SwapDeckSettingsListener`) dai thread del plugin, mai con un lock interno preso: chi aggiorna la UI passa all'EDT. Un cambio delle impostazioni avvia o ferma il motore su un thread del plugin, non su quello che salva.
- kotlinx.serialization e la libreria standard di Kotlin le fornisce l'IDE: non entrano nello zip.

Comandi:

| Comando                                               | Cosa                                                                             |
| ----------------------------------------------------- | -------------------------------------------------------------------------------- |
| `pnpm build`, `pnpm test` (qui o dalla radice)        | solo il pannello e i test vitest: niente Java                                    |
| `pnpm jetbrains:build` (radice) o `pnpm gradle:build` | `./gradlew buildPlugin`: `build/distributions/swapdeck-jetbrains-<versione>.zip` |
| `pnpm jetbrains:test` (radice) o `pnpm gradle:test`   | `./gradlew test`                                                                 |

Lo zip contiene `swapdeck-jetbrains/lib/swapdeck-jetbrains-<versione>.jar` con classi, `plugin.xml`, testi, `swapdeck/protocol.json` e `webview/`. `swapdeck-cli` per piattaforma arriva con il sottotask 11.
