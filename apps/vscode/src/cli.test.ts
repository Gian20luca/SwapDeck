import * as path from 'node:path';
import { describe, expect, it } from 'vitest';
import { CliError, cliExecutable, parseCliResult, resolveCliPath, toCswapError } from './cli.js';

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
