import { describe, expect, it } from 'vitest';
import { CONFIG_FILE, LEGACY_SYNC_FILES, SYNC_FILE, isSyncSignalName } from './sync.js';

describe('segnale di sincronia', () => {
  it('nome neutro, vecchio nome di VS Code ancora riconosciuto', () => {
    expect(SYNC_FILE).toBe('ide-sync.json');
    expect(LEGACY_SYNC_FILES).toEqual(['vscode-sync.json']);
    expect(CONFIG_FILE).toBe('config.json');
    expect(isSyncSignalName('ide-sync.json')).toBe(true);
    expect(isSyncSignalName('vscode-sync.json')).toBe(true);
    expect(isSyncSignalName('config.json')).toBe(false);
    expect(isSyncSignalName('.ide-sync.json.abc.tmp')).toBe(false);
  });
});
