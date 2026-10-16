import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import * as path from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import {
  CliError,
  cliExecutable,
  createCliRunner,
  ensureExecutable,
  parseCliResult,
  resolveCliPath,
  toCswapError,
} from './cli.js';
import { preloadEnv, PROCESS_TEST_TIMEOUT_MS } from './testing.js';

/** `true` se il processo esiste ancora (segnale 0: solo controllo). */
function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return (error as NodeJS.ErrnoException).code === 'EPERM';
  }
}

describe('cli', () => {
  it("nome dell'eseguibile e percorso effettivo", () => {
    expect(cliExecutable('win32')).toBe('swapdeck-cli.exe');
    expect(cliExecutable('linux')).toBe('swapdeck-cli');
    const ext = path.resolve('ext');
    expect(resolveCliPath('', ext, 'linux')).toBe(path.join(ext, 'bin', 'swapdeck-cli'));
    expect(resolveCliPath('relativo/cli', ext, 'win32')).toBe(
      path.join(ext, 'bin', 'swapdeck-cli.exe'),
    );
    const abs = path.resolve('altro', 'swapdeck-cli');
    expect(resolveCliPath(` ${abs} `, ext, 'linux')).toBe(abs);
  });

  it('parseCliResult: successo, errore JSON, uscita senza JSON', () => {
    expect(parseCliResult(0, '{"a":1}\n')).toEqual({ a: 1 });
    expect(() => parseCliResult(1, '{"error":{"kind":"busy","message":"occupato"}}')).toThrow(
      CliError,
    );
    let caught: unknown;
    try {
      parseCliResult(2, '{"error":{"kind":"invalidUsage","message":"uso"}}');
    } catch (e) {
      caught = e;
    }
    expect(toCswapError(caught)).toEqual({ kind: 'invalidUsage', message: 'uso' });
    expect(() => parseCliResult(1, '', 'panico')).toThrow(/non JSON.*panico/);
    expect(() => parseCliResult(1, '{"x":1}')).toThrow(/uscita 1/);
    expect(toCswapError(new Error('x'))).toEqual({ kind: 'failed', message: 'x' });
  });
});

describe('createCliRunner', { timeout: PROCESS_TEST_TIMEOUT_MS }, () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'swapdeck-cli-'));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  /** `node <args>` con uno script precaricato che fa da swapdeck-cli. */
  const fake = (code: string) => preloadEnv(dir, code);
  const run = createCliRunner(() => process.execPath);

  it('passa stdin e legge il JSON di risposta', async () => {
    const env = fake(
      "const fs = require('fs'); fs.writeSync(1, JSON.stringify({ got: fs.readFileSync(0, 'utf8') })); process.exit(0);",
    );
    await expect(run(['action', '-'], { stdin: '{"kind":"purge"}', env })).resolves.toEqual({
      got: '{"kind":"purge"}',
    });
  });

  it("errore della CLI: rifiuta con l'errore del JSON", async () => {
    const env = fake(
      "process.stdout.write(JSON.stringify({ error: { kind: 'busy', message: 'occupato' } })); process.exit(1);",
    );
    await expect(run(['state'], { env })).rejects.toMatchObject({
      info: { kind: 'busy', message: 'occupato' },
    });
  });

  it('timeout: termina il processo e rifiuta con timeout', async () => {
    // Il finto scrive il suo PID e resta bloccato nel preload: senza il blocco `node state`
    // uscirebbe da solo (modulo inesistente) e l'esito dipenderebbe dalla velocità di avvio.
    const pidFile = path.join(dir, 'timeout.pid');
    const env = fake(
      `require('fs').writeFileSync(${JSON.stringify(pidFile)}, String(process.pid)); ` +
        'Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 120000);',
    );
    await expect(run(['state'], { env, timeoutMs: 3000 })).rejects.toMatchObject({
      info: { kind: 'timeout' },
    });
    // Il rifiuto arriva dopo l'uscita del processo: non deve essere più vivo.
    const pid = Number(readFileSync(pidFile, 'utf8'));
    expect(pid).toBeGreaterThan(0);
    expect(isAlive(pid)).toBe(false);
  });

  it('eseguibile mancante: errore chiaro', async () => {
    const missing = createCliRunner(() => path.join(dir, 'manca', cliExecutable(process.platform)));
    await expect(missing(['state'])).rejects.toMatchObject({ info: { kind: 'failed' } });
  });

  it('ensureExecutable: nulla su Windows, file esistente altrove', async () => {
    const file = path.join(dir, 'bin');
    writeFileSync(file, '');
    await expect(ensureExecutable(file, 'win32')).resolves.toBeUndefined();
    await expect(ensureExecutable(file, 'linux')).resolves.toBeUndefined();
  });
});
