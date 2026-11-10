/**
 * Segnale di sincronia fra host (finestre di VS Code, IDE JetBrains, app desktop) sulla stessa
 * cartella della config di SwapDeck. Formato e regole di scrittura sono in `README.md`; i valori
 * stanno in `protocol.json` (sezione `sync`).
 */
import protocol from '../protocol.json' with { type: 'json' };

/** Segnale riscritto da un host dopo una propria azione: `{ "source": string, "at": number }`. */
export const SYNC_FILE: string = protocol.sync.file;
/**
 * Nomi precedenti del segnale, ancora osservati (mai scritti) per compatibilità con le finestre di
 * VS Code non aggiornate. `vscode-sync.json` (fino alla 0.2.4) si toglie nella seconda versione dopo
 * la 0.2.4: la prima scrive già `ide-sync.json` e osserva ancora il vecchio nome.
 */
export const LEGACY_SYNC_FILES: readonly string[] = protocol.sync.legacyFiles;
/** File della config di SwapDeck (`config::CONFIG_FILE` del core). */
export const CONFIG_FILE: string = protocol.sync.configFile;
/** Attesa che raggruppa gli eventi della cartella prima di rileggere. */
export const SYNC_DEBOUNCE_MS: number = protocol.sync.debounceMs;
/** Il segnale è piccolo: oltre questa dimensione non si legge (file estraneo). */
export const MAX_SIGNAL_BYTES: number = protocol.sync.maxSignalBytes;

/** Contenuto del segnale: chi l'ha scritto (identificativo opaco per processo) e quando (ms epoch). */
export interface SyncSignal {
  source: string;
  at: number;
}

/** Il nome è quello di un segnale, attuale o precedente? */
export function isSyncSignalName(name: string): boolean {
  return name === SYNC_FILE || LEGACY_SYNC_FILES.includes(name);
}
