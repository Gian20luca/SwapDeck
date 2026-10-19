# Rilascio

Workflow: `.github/workflows/release.yml`. Produce installer, `swapdeck-cli`, `.vsix` per piattaforma,
firme dell'updater e `latest.json`, e li mette in una release **in bozza**: la pubblicazione resta manuale.

## Novità 0.2.1 (19/10)

- **Barra laterale stretta**: le card non escono più dal pannello con nomi di organizzazione lunghi.
- **Lingua in VS Code**: l'estensione usa la lingua scelta in SwapDeck e non torna a quella di VS Code al cambio di scheda.
- **Impostazioni di cswap auto** (prima «Configurazione di cswap») spostate sotto «Avanzate»: SwapDeck non le usa.

## Novità 0.2.0 (17/10)

- **Chi consuma**: classifica in sola lettura per progetto, sessione e agente su 5h e 7d dalle sessioni
  di Claude Code del PC (solo conteggi, mai il contenuto), più la quota stimata di altri dispositivi.
- **Mappe**: i quattro preset sono sempre presenti e in sola lettura; «Clona come mappa personale»
  crea una copia modificabile. Le config della v0.1.0 con preset modificati si migrano in mappe personali.
- **VS Code**: vista completa nella barra laterale, sincronizzata tra le finestre aperte.
- **Icone «i»** con la spiegazione di ogni impostazione della pagina Regole.
- **Prima accensione del motore** con conferma (Accendi, Solo osservazione, Annulla).
- **Log e diagnostica**: log per utente con rotazione e redazione, «Copia diagnostica» in app, VS Code
  e `swapdeck-cli diagnostics`.
- **Lock della config**: app, estensione e CLI non si sovrascrivono le modifiche a `config.json`.
- **Schede aggiornate da sole** quando cambiano lo stato di `cswap` o la config.
- **Avvio automatico** corretto su Linux e macOS; la voce spenta dal desktop viene rispettata.

## Prova senza release

Actions → Release → Run workflow sul branch, con `dry_run` attivo e `platforms` scelto (macOS costa
10 volte i minuti). Gli artefatti finiscono in `release-dry-run`, conservato per 7 giorni.

## Release in bozza

1. Allinea la versione `X.Y.Z` in `Cargo.toml` (`[workspace.package]`, vale anche per l'app Tauri)
   e in `apps/vscode/package.json`. Se le versioni sono diverse, il job `publish` fallisce.
2. Fai il commit su `main` e aspetta che la CI sia verde.
3. Crea e pubblica il tag, che deve essere `v` seguito dalla versione:
   `git tag vX.Y.Z` e poi `git push origin vX.Y.Z`.
4. Il workflow costruisce tutte le piattaforme e crea la bozza `vX.Y.Z`. Se lo rilanci sullo stesso
   tag, ricarica gli asset solo se la release è ancora in bozza: una release pubblicata non si tocca.
5. Controlla la bozza, poi su GitHub scegli **Publish release**.

Una release vera parte solo da un tag: un dispatch con `dry_run` spento su un branch si ferma subito.

## Chiave dell'updater

- La chiave privata, cifrata, e la sua password stanno in `%USERPROFILE%\.swapdeck-keys\` sul PC
  di sviluppo, con ACL riservate all'utente. Non vanno mai nel repo né nei log.
- La CI le usa come segreti del repo `TAURI_SIGNING_PRIVATE_KEY` e
  `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, soltanto nello step «Firma per l'updater». La compilazione
  gira senza segreti.
- La chiave pubblica è in `apps/desktop/src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).
- Tieni una copia della cartella fuori linea. Se la chiave si perde, le app installate non accettano
  più aggiornamenti e vanno reinstallate a mano.
- Per cambiare chiave, pubblica prima una versione con la nuova `pubkey`, firmata con la chiave
  vecchia. Dalla versione successiva si firma con la nuova.

## Attivare l'updater (repo pubblico)

L'endpoint `https://github.com/Gian20luca/SwapDeck/releases/latest/download/latest.json` risponde
solo se il repo è pubblico e la release è pubblicata (non in bozza e non pre-release). Finché il
repo è privato risponde 404, per questo l'updater è spento di default.

1. Rendi pubblico il repo e pubblica una release fatta con questo workflow.
2. Per una prova, aggiungi `"updates": { "enabled": true }` al `config.json` di un'installazione.
3. Per accenderlo a tutti, porta a `true` il default di `updates_enabled` in
   `apps/desktop/src-tauri/src/settings.rs` (e il test relativo), poi rilascia una nuova versione.

L'app scarica e verifica l'aggiornamento in background e lo installa all'uscita, senza riaprirsi.
Su Linux si aggiorna solo l'AppImage, perché il `.deb` resta a carico del gestore di pacchetti.
