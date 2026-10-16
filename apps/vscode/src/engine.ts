/** Motore come processo figlio: `swapdeck-cli headless --stop-on-stdin-eof` (rispetta il lock). */
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';

/** Attesa dell'uscita pulita (stdin chiuso): il giro in corso (conflitti, cswap) va finito. */
export const STOP_GRACE_MS = 15_000;
/** Alla disattivazione VS Code concede pochi secondi; un lock lasciato da un PID morto scade subito. */
export const DEACTIVATE_GRACE_MS = 4000;

export interface EngineEvents {
  onLine: (line: string) => void;
  /** Uscita non chiesta (errore o config non valida). */
  onUnexpectedExit: (code: number | null) => void;
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

  start(cliPath: string, env: NodeJS.ProcessEnv = process.env): void {
    if (this.child) return;
    this.stopping = false;
    const child = spawn(cliPath, ['headless', '--stop-on-stdin-eof'], {
      shell: false,
      windowsHide: true,
      stdio: ['pipe', 'pipe', 'pipe'],
      env,
    });
    this.child = child;
    const lines = (chunk: Buffer) => {
      for (const line of chunk.toString('utf8').split(/\r?\n/)) {
        if (line.trim()) this.events.onLine(line);
      }
    };
    child.stdout.on('data', lines);
    child.stderr.on('data', lines);
    child.stdin.on('error', () => undefined);
    child.on('error', (error) => this.events.onLine(`swapdeck-cli headless: ${error.message}`));
    child.on('close', (code) => {
      if (this.child === child) this.child = undefined;
      if (!this.stopping) this.events.onUnexpectedExit(code);
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
