/** Esecuzione di `swapdeck-cli` dall'extension host: JSON su stdout, errori `{"error": …}`. */
import { spawn } from 'node:child_process';
import { constants, promises as fs } from 'node:fs';
import * as path from 'node:path';
import { StringDecoder } from 'node:string_decoder';
import type { CswapError } from '@swapdeck/types';

/** Codici di uscita dei comandi JSON (`apps/cli/src/lib.rs`). */
export const EXIT_OK = 0;

/** Limite dell'output letto (lo storico di 7 giorni con molti account resta sotto). */
const MAX_STDOUT_BYTES = 64 * 1024 * 1024;
const STDERR_TAIL_BYTES = 4096;

export const DEFAULT_TIMEOUT_MS = 60_000;
/** Le azioni che scrivono hanno già un loro timeout in Rust (30 s, `upgrade` di più). */
export const ACTION_TIMEOUT_MS = 6 * 60_000;
/**
 * `usage breakdown` legge a budget (circa 8 s a chiamata, la UI richiama finché c'è `partial`):
 * margine per l'elenco dei file su dischi lenti.
 */
export const USAGE_TIMEOUT_MS = 2 * 60_000;
/** Dopo il timeout: attesa dell'uscita del processo prima di `SIGKILL` (e poi del rifiuto). */
export const KILL_GRACE_MS = 2_000;

export function cliExecutable(platform: NodeJS.Platform): string {
  return platform === 'win32' ? 'swapdeck-cli.exe' : 'swapdeck-cli';
}

/** `swapdeck-cli` incluso nel .vsix (`bin/`). */
export function bundledCliPath(extensionPath: string, platform: NodeJS.Platform): string {
  return path.join(extensionPath, 'bin', cliExecutable(platform));
}

/** Percorso effettivo: l'impostazione se valorizzata (solo assoluta), altrimenti quello incluso. */
export function resolveCliPath(
  setting: string | undefined,
  extensionPath: string,
  platform: NodeJS.Platform,
): string {
  const custom = setting?.trim();
  if (custom && path.isAbsolute(custom)) return custom;
  return bundledCliPath(extensionPath, platform);
}

/**
 * Su Linux e macOS il bit di esecuzione può perdersi nell'estrazione del .vsix: lo ripristina,
 * ma solo sul binario incluso nell'estensione (mai su un percorso scelto dall'utente).
 */
export async function ensureExecutable(file: string, platform: NodeJS.Platform): Promise<void> {
  if (platform === 'win32') return;
  try {
    await fs.access(file, constants.X_OK);
  } catch {
    const stat = await fs.lstat(file);
    if (stat.isFile()) await fs.chmod(file, 0o755);
  }
}

export class CliError extends Error {
  readonly info: CswapError;
  constructor(info: CswapError) {
    super(info.message);
    this.name = 'CliError';
    this.info = info;
  }
}

export function toCswapError(error: unknown): CswapError {
  if (error instanceof CliError) return error.info;
  const message = error instanceof Error ? error.message : String(error);
  return { kind: 'failed', message };
}

function isCswapError(value: unknown): value is CswapError {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { kind?: unknown }).kind === 'string' &&
    typeof (value as { message?: unknown }).message === 'string'
  );
}

/** Esito di un comando dal codice di uscita e da stdout. */
export function parseCliResult<T>(code: number | null, stdout: string, stderr = ''): T {
  let value: unknown;
  try {
    value = JSON.parse(stdout.trim());
  } catch {
    const tail = stderr.trim().slice(-500);
    throw new CliError({
      kind: 'failed',
      message: `swapdeck-cli: risposta non JSON (uscita ${code ?? 'segnale'})${tail ? `: ${tail}` : ''}`,
    });
  }
  if (code === EXIT_OK) return value as T;
  const error = (value as { error?: unknown } | null)?.error;
  if (isCswapError(error)) throw new CliError(error);
  throw new CliError({ kind: 'failed', message: `swapdeck-cli: uscita ${code ?? 'segnale'}` });
}

/** `setup install|upgrade`: script di uv e `uv tool` hanno 10 minuti ciascuno in Rust. */
export const SETUP_TIMEOUT_MS = 30 * 60_000;

export interface RunOptions {
  /** Testo su stdin (le azioni passano da qui: un token non deve finire negli argomenti). */
  stdin?: string;
  timeoutMs?: number;
  env?: NodeJS.ProcessEnv;
  /**
   * Comandi che scrivono un evento JSON per riga (`setup install|upgrade`): ogni riga con `event`
   * arriva qui appena completa; l'esito è l'ultima riga.
   */
  onEvent?: (event: unknown) => void;
}

/** Spezza l'output in righe JSON e passa quelle con `event`. */
export function eventLines(onEvent: (event: unknown) => void): (chunk: string) => void {
  let pending = '';
  return (chunk) => {
    pending += chunk;
    const lines = pending.split('\n');
    pending = lines.pop() ?? '';
    for (const line of lines) {
      try {
        const value: unknown = JSON.parse(line);
        if (typeof (value as { event?: unknown } | null)?.event === 'string') onEvent(value);
      } catch {
        // Riga non JSON: si ignora, l'esito si legge dall'ultima riga.
      }
    }
  };
}

/** Ultima riga non vuota (l'esito dei comandi con eventi). */
export function lastLine(text: string): string {
  const lines = text.split('\n').filter((l) => l.trim() !== '');
  return lines[lines.length - 1] ?? '';
}

export type CliRunner = <T>(args: string[], options?: RunOptions) => Promise<T>;

/** Lancia `swapdeck-cli` senza shell e senza finestra, con timeout e limite di output. */
/** Nome con cui la CLI lanciata dall'estensione firma le sue righe nel log di SwapDeck. */
export const LOG_SOURCE = 'vscode';

/** Ambiente dei processi `swapdeck-cli`: quello di VS Code più `SWAPDECK_LOG_SOURCE`. */
export function cliEnv(base: NodeJS.ProcessEnv = process.env): NodeJS.ProcessEnv {
  return { ...base, SWAPDECK_LOG_SOURCE: LOG_SOURCE };
}

export function createCliRunner(getPath: () => string): CliRunner {
  return <T>(args: string[], options: RunOptions = {}) =>
    new Promise<T>((resolve, reject) => {
      const file = getPath();
      const child = spawn(file, args, {
        shell: false,
        windowsHide: true,
        stdio: ['pipe', 'pipe', 'pipe'],
        env: cliEnv(options.env ?? process.env),
      });
      const chunks: Buffer[] = [];
      const onChunk = options.onEvent ? eventLines(options.onEvent) : undefined;
      const decoder = new StringDecoder('utf8');
      let size = 0;
      let stderr = '';
      let settled = false;
      let expired = false;
      let killTimer: NodeJS.Timeout | undefined;
      const finish = (fn: () => void) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        clearTimeout(killTimer);
        fn();
      };
      const rejectTimeout = () =>
        reject(
          new CliError({
            kind: 'timeout',
            message: `swapdeck-cli ${args[0] ?? ''}: nessuna risposta in tempo`,
          }),
        );
      // Timeout: si termina il processo e si rifiuta quando è uscito (se non esce: `SIGKILL`,
      // poi un'altra attesa), così chi chiama non lascia processi vivi alle spalle.
      const timer = setTimeout(() => {
        expired = true;
        child.kill();
        killTimer = setTimeout(() => {
          child.kill('SIGKILL');
          killTimer = setTimeout(() => finish(rejectTimeout), KILL_GRACE_MS);
        }, KILL_GRACE_MS);
      }, options.timeoutMs ?? DEFAULT_TIMEOUT_MS);
      child.on('exit', () => {
        if (expired) finish(rejectTimeout);
      });
      child.stdout.on('data', (chunk: Buffer) => {
        size += chunk.length;
        if (size > MAX_STDOUT_BYTES) {
          child.kill();
          finish(() =>
            reject(
              new CliError({ kind: 'failed', message: 'swapdeck-cli: risposta troppo grande' }),
            ),
          );
          return;
        }
        chunks.push(chunk);
        onChunk?.(decoder.write(chunk));
      });
      child.stderr.on('data', (chunk: Buffer) => {
        stderr = (stderr + chunk.toString('utf8')).slice(-STDERR_TAIL_BYTES);
      });
      child.on('error', (error: NodeJS.ErrnoException) => {
        const message =
          error.code === 'ENOENT'
            ? `swapdeck-cli non trovato: ${file}`
            : `swapdeck-cli non avviato: ${error.message}`;
        finish(() => reject(new CliError({ kind: 'failed', message })));
      });
      child.on('close', (code) => {
        finish(() => {
          try {
            const out = Buffer.concat(chunks).toString('utf8');
            resolve(parseCliResult<T>(code, options.onEvent ? lastLine(out) : out, stderr));
          } catch (error) {
            reject(error);
          }
        });
      });
      child.stdin.on('error', () => {
        // Il processo può chiudere stdin prima di leggerlo (errore d'uso): l'esito arriva da `close`.
      });
      child.stdin.end(options.stdin ?? '');
    });
}
