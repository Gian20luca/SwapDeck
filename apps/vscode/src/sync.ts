/**
 * Sincronia fra finestre di VS Code (anche istanze diverse) sulla stessa cartella di SwapDeck.
 *
 * Ogni finestra osserva la cartella della config: `config.json` (regole, mappe, preferenze,
 * notifiche, scritti dalla CLI) e il segnale `vscode-sync.json`, che una finestra riscrive dopo
 * una propria azione che non tocca la config (cambio di account, azioni di cswap, cambi del
 * motore). Gli eventi sono raggruppati (`debounceMs`) e fanno rileggere lo stato; il polling
 * resta come rete di sicurezza (watcher non disponibile, cartella che non esiste ancora).
 */
import { randomBytes } from 'node:crypto';
import * as fs from 'node:fs';
import { basename, join } from 'node:path';

/** Segnale scritto dalle finestre: `{ source, at }`, nessun dato dell'utente. */
export const SYNC_FILE = 'vscode-sync.json';
/** File della config di SwapDeck (`config::CONFIG_FILE` del core). */
export const CONFIG_FILE = 'config.json';
export const SYNC_DEBOUNCE_MS = 750;
/** Il segnale è piccolo: oltre questa dimensione non si legge (file estraneo). */
const MAX_SIGNAL_BYTES = 4096;

export interface SyncOptions {
  /** Cartella della config (da `swapdeck-cli config get`, campo `path`). */
  dir: string;
  /** Dopo un cambio fatto da un altro processo (raggruppato). */
  onChange: () => void;
  debounceMs?: number;
  log?: (message: string) => void;
  /** Sostituibili nei test. */
  watch?: typeof fs.watch;
  source?: string;
}

/** Il nome dell'evento riguarda la sincronia? `null` (piattaforme senza nome): sì, per prudenza. */
export function isSyncEvent(filename: string | null | undefined): boolean {
  if (filename === null || filename === undefined) return true;
  const name = basename(filename);
  return name === CONFIG_FILE || name === SYNC_FILE;
}

export class SyncWatcher {
  /** Identifica questa finestra nel segnale: il proprio segnale non fa rileggere. */
  readonly source: string;
  private watcher: fs.FSWatcher | undefined;
  private timer: NodeJS.Timeout | undefined;
  private disposed = false;

  constructor(private readonly opts: SyncOptions) {
    this.source = opts.source ?? `${process.pid}-${randomBytes(6).toString('hex')}`;
  }

  get watching(): boolean {
    return this.watcher !== undefined;
  }

  /** Avvia l'osservazione; senza cartella (o con errore) resta spento e si può ritentare. */
  start(): boolean {
    if (this.disposed || this.watcher) return this.watcher !== undefined;
    try {
      const watcher = (this.opts.watch ?? fs.watch)(this.opts.dir, { persistent: false });
      watcher.on('change', (_event, filename) => this.onEvent(filename));
      watcher.on('error', (error) => {
        this.opts.log?.(`sync: watcher fermato (${error.message}), resta il polling`);
        this.stopWatcher();
      });
      this.watcher = watcher;
      return true;
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code !== 'ENOENT') this.opts.log?.(`sync: watcher non disponibile (${code ?? error})`);
      return false;
    }
  }

  private onEvent(filename: string | Buffer | null): void {
    const name = typeof filename === 'string' ? filename : filename?.toString('utf8');
    if (!isSyncEvent(name)) return;
    if (name !== undefined && basename(name) === SYNC_FILE && this.isOwnSignal()) return;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => {
      if (!this.disposed) this.opts.onChange();
    }, this.opts.debounceMs ?? SYNC_DEBOUNCE_MS);
  }

  /** Il segnale attuale l'ha scritto questa finestra? Errori e file strani: no (si rilegge). */
  private isOwnSignal(): boolean {
    try {
      const path = join(this.opts.dir, SYNC_FILE);
      const st = fs.lstatSync(path);
      if (!st.isFile() || st.size > MAX_SIGNAL_BYTES) return false;
      const data = JSON.parse(fs.readFileSync(path, 'utf8')) as { source?: unknown };
      return data.source === this.source;
    } catch {
      return false;
    }
  }

  /**
   * Avvisa le altre finestre: segnale riscritto in modo atomico (file temporaneo esclusivo,
   * permessi 0600, poi rinomina sopra il precedente). Un errore non ferma nulla: resta il polling.
   */
  async signal(): Promise<void> {
    if (this.disposed) return;
    const target = join(this.opts.dir, SYNC_FILE);
    const tmp = join(this.opts.dir, `.${SYNC_FILE}.${randomBytes(6).toString('hex')}.tmp`);
    const body = JSON.stringify({ source: this.source, at: Date.now() });
    try {
      await fs.promises.writeFile(tmp, body, { flag: 'wx', mode: 0o600 });
      try {
        await renameWithRetry(tmp, target);
      } catch (error) {
        await fs.promises.rm(tmp, { force: true });
        throw error;
      }
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code !== 'ENOENT') this.opts.log?.(`sync: segnale non scritto (${code ?? error})`);
    }
  }

  private stopWatcher(): void {
    this.watcher?.close();
    this.watcher = undefined;
  }

  dispose(): void {
    this.disposed = true;
    clearTimeout(this.timer);
    this.stopWatcher();
  }
}

/** Su Windows la rinomina sopra un file appena letto da un'altra finestra può dare EPERM/EBUSY. */
async function renameWithRetry(from: string, to: string, attempts = 3): Promise<void> {
  for (let i = 1; ; i++) {
    try {
      await fs.promises.rename(from, to);
      return;
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (i >= attempts || (code !== 'EPERM' && code !== 'EBUSY' && code !== 'EACCES')) throw error;
      await new Promise((r) => setTimeout(r, 50 * i));
    }
  }
}
