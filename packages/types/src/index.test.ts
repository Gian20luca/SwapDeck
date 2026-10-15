import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { SCHEMA_VERSION } from './index.js';
import type { CswapAction, ListResponse, StatusResponse, SwitchResponse } from './index.js';

const fixture = <T>(name: string): T =>
  JSON.parse(
    readFileSync(
      fileURLToPath(new URL(`../../../fixtures/cswap/${name}`, import.meta.url)),
      'utf8',
    ),
  ) as T;

describe('types', () => {
  it('espone la versione del formato', () => {
    expect(SCHEMA_VERSION).toBe(1);
  });

  it('descrive le fixture di cswap', () => {
    const list = fixture<ListResponse>('list.json');
    expect(list.schemaVersion).toBe(SCHEMA_VERSION);
    expect(list.accounts.length).toBeGreaterThan(0);
    expect(list.accounts.some((a) => a.loginExpiresAt !== undefined)).toBe(true);
    const status = fixture<StatusResponse>('status.json');
    expect(status.active?.managed).toBe(true);
    const sw = fixture<SwitchResponse>('switch-switched.json');
    expect(sw.reason).toBe('switched');
  });

  it('costruisce le azioni', () => {
    const action: CswapAction = { kind: 'switch', strategy: 'best', models: ['Fable'] };
    expect(action.kind).toBe('switch');
  });
});
