import type { CswapAction } from '@swapdeck/types';
import { describe, expect, it } from 'vitest';
import protocol from '../protocol.json' with { type: 'json' };
import {
  ACTION_KINDS,
  CONFIG_KEY,
  DEFAULT_HISTORY_SECONDS,
  MAX_COPY_CHARS,
  MAX_RELOGIN_CODE_CHARS,
  PICK_LIMITS,
  SAFE_CLAUDE_ARGS,
  USAGE_GROUPS,
  USAGE_WINDOWS,
  VERSION,
} from './rules.js';

/**
 * Tutti i `kind` di `CswapAction` (generato dal Rust): se il Rust ne aggiunge uno, qui manca una
 * chiave ed è un errore di compilazione; il test sotto chiede poi di aggiungerlo a `protocol.json`.
 */
const RUST_KINDS: Record<CswapAction['kind'], true> = {
  switch: true,
  add: true,
  addToken: true,
  remove: true,
  disable: true,
  enable: true,
  setAlias: true,
  swap: true,
  move: true,
  map: true,
  unmap: true,
  configSet: true,
  configUnset: true,
  export: true,
  import: true,
  unclaimedPurge: true,
  purge: true,
  upgrade: true,
  run: true,
  autoStart: true,
  autoStop: true,
};

describe('protocol.json', () => {
  it('actionKinds coincide con i kind di CswapAction, senza doppioni', () => {
    expect([...ACTION_KINDS].sort()).toEqual(Object.keys(RUST_KINDS).sort());
    expect(new Set(ACTION_KINDS).size).toBe(ACTION_KINDS.length);
  });

  it('valori letti come nelle regole precedenti', () => {
    expect([...SAFE_CLAUDE_ARGS]).toEqual(['--continue', '--verbose']);
    expect(MAX_RELOGIN_CODE_CHARS).toBe(4096);
    expect(MAX_COPY_CHARS).toBe(1_000_000);
    expect(DEFAULT_HISTORY_SECONDS).toBe(7 * 24 * 3600);
    expect(PICK_LIMITS.textFields).toEqual(['title', 'defaultPath']);
    expect(USAGE_WINDOWS).toEqual(['5h', '7d', 'custom']);
    expect(USAGE_GROUPS).toEqual(['project', 'session', 'agent', 'model']);
    expect(protocol.version).toBe(1);
  });

  it('i pattern sono ancorati e validano come prima', () => {
    for (const re of [CONFIG_KEY, VERSION, PICK_LIMITS.extension]) {
      expect(re.source.startsWith('^') && re.source.endsWith('$')).toBe(true);
    }
    expect(CONFIG_KEY.test('auto.strategy')).toBe(true);
    expect(CONFIG_KEY.test('a b')).toBe(false);
    expect(CONFIG_KEY.test('x'.repeat(101))).toBe(false);
    expect(VERSION.test('1.2.3rc1')).toBe(true);
    expect(VERSION.test('1.0; rm -rf')).toBe(false);
    expect(PICK_LIMITS.extension.test('json')).toBe(true);
    expect(PICK_LIMITS.extension.test('*.json')).toBe(false);
  });
});
