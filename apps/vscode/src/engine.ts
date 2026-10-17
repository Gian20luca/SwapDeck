/**
 * Motore come processo figlio: `swapdeck-cli headless --stop-on-stdin-eof --notify=stdout`
 * (rispetta il lock). Le notifiche arrivano come righe JSON e le mostra l'estensione.
 */
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { StringDecoder } from 'node:string_decoder';
import type { Readable } from 'node:stream';
import { cliEnv } from './cli.js';

/** Attesa dell'uscita pulita (stdin chiuso): il giro in corso (conflitti, cswap) va finito. */
export const STOP_GRACE_MS = 15_000;
/** Alla disattivazione VS Code concede pochi secondi; un lock lasciato da un PID morto scade subito. */
export const DEACTIVATE_GRACE_MS = 4000;
/** `headless::exit_code::LOCK_BUSY` del core: il motore gira già altrove (altra finestra o app). */
export const LOCK_BUSY_EXIT = 3;
/** Argomenti del motore: `--notify=stdout` evita il doppione con le notifiche di sistema. */
export const ENGINE_ARGS: readonly string[] = [
  'headless',
  '--stop-on-stdin-eof',
  '--notify=stdout',
];
/** Riga più lunga inoltrata al log (il resto si scarta). */
const MAX_LINE = 4096;

export interface EngineEvents {
  onLine: (line: string) => void;
  /** Uscita non chiesta (errore o config non valida). */
  onUnexpectedExit: (code: number | null) => void;
  /** Il lock è di un'altra istanza: questa resta in attesa e riprova (nessun avviso). */
  onLockBusy?: () => void;
}

/** Spezza uno stream in righe intere (UTF-8 anche a cavallo dei blocchi). */
export class LineSplitter {
  private readonly decoder = new StringDecoder('utf8');
  private rest = '';

  constructor(private readonly onLine: (line: string) => void) {}

  push(chunk: Buffer | string): void {
    const text = this.rest + (typeof chunk === 'string' ? chunk : this.decoder.write(chunk));
    const parts = text.split(/\r?\n/);
    this.rest = (parts.pop() ?? '').slice(0, MAX_LINE + 1);
    for (const p of parts) this.emit(p);
  }

  end(): void {
    this.emit(this.rest + this.decoder.end());
    this.rest = '';
  }

  private emit(line: string): void {
    if (line.trim()) this.onLine(line.length > MAX_LINE ? `${line.slice(0, MAX_LINE)}…` : line);
  }
}

/** Ritentare l'avvio: motore voluto, non attivo qui, in attesa del lock e lock ora libero. */
export function shouldRetryEngine(o: {
  enabled: boolean;
  localRunning: boolean;
  waitingLock: boolean;
  lockHeldElsewhere: boolean | undefined;
}): boolean {
  return o.enabled && !o.localRunning && o.waitingLock && o.lockHeldElsewhere === false;
}

function pipeLines(stream: Readable, onLine: (line: string) => void): void {
  const splitter = new LineSplitter(onLine);
  stream.on('data', (chunk: Buffer) => splitter.push(chunk));
  stream.on('end', () => splitter.end());
}

export class EngineProcess {
  private child: ChildProcessWithoutNullStreams | undefined;
  private stopping = false;

  constructor(private readonly events: EngineEvents) {}

  get running(): boolean {
    return this.child !== undefined;
  }

  get pid(): number | undefined {
    return this.child?.pid;
  }

  start(cliPath: string, env: NodeJS.ProcessEnv = cliEnv()): void {
    if (this.child) return;
    this.stopping = false;
    const child = spawn(cliPath, [...ENGINE_ARGS], {
      shell: false,
      windowsHide: true,
      stdio: ['pipe', 'pipe', 'pipe'],
      env,
    });
    this.child = child;
    pipeLines(child.stdout, this.events.onLine);
    pipeLines(child.stderr, this.events.onLine);
    child.stdin.on('error', () => undefined);
    child.on('error', (error) => this.events.onLine(`swapdeck-cli headless: ${error.message}`));
    child.on('close', (code) => {
      if (this.child === child) this.child = undefined;
      if (this.stopping) return;
      if (code === LOCK_BUSY_EXIT && this.events.onLockBusy) this.events.onLockBusy();
      else this.events.onUnexpectedExit(code);
    });
  }

  /** Chiude stdin (il runner rilascia il lock ed esce), poi termina se non basta. */
  async stop(graceMs = STOP_GRACE_MS): Promise<void> {
    const child = this.child;
    if (!child) return;
    this.stopping = true;
    const exited = new Promise<void>((resolve) => child.once('close', () => resolve()));
    child.stdin.end();
    let timer: NodeJS.Timeout | undefined;
    const timeout = new Promise<'timeout'>((resolve) => {
      timer = setTimeout(() => resolve('timeout'), graceMs);
    });
    const result = await Promise.race([exited, timeout]);
    clearTimeout(timer);
    if (result === 'timeout') {
      child.kill();
      await Promise.race([exited, new Promise((r) => setTimeout(r, 1000))]);
    }
    if (this.child === child) this.child = undefined;
  }
}
