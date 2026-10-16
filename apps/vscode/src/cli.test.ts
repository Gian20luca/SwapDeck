import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
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
    // Bloccato nel preload: Node non arriva mai a cercare lo script `state` (che non esiste).
    const env = fake('Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0);');
    await expect(run(['state'], { env, timeoutMs: 300 })).rejects.toMatchObject({
      info: { kind: 'timeout' },
    });
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
