import { describe, expect, it } from 'vitest';
import { FakeBackend } from './FakeBackend.js';

describe('FakeBackend: gestione completa', () => {
  it('map e unmap aggiornano l’elenco delle cartelle', async () => {
    const backend = new FakeBackend();
    expect(await backend.listMappings()).toEqual([]);
    await backend.runAction({ kind: 'map', account: 1, path: 'C:/a' });
    await backend.runAction({ kind: 'map', account: 3, path: 'C:/a' });
    expect(await backend.listMappings()).toEqual([
      { path: 'C:/a', account: 3, email: expect.any(String) },
    ]);
    await backend.runAction({ kind: 'unmap', path: 'C:/a' });
    expect(await backend.listMappings()).toEqual([]);
    await expect(backend.runAction({ kind: 'map', account: 99, path: 'x' })).rejects.toThrow(
      'does not exist',
    );
  });

  it('le regole si salvano solo se valide e lo stato spiega la sospensione', async () => {
    const backend = new FakeBackend({
      conflicts: [{ id: 'task:g', kind: 'scheduledTask', label: 'g', certain: true }],
    });
    expect((await backend.getEngineState()).view?.suspended).toBe('disabled');
    const rules = (await backend.getEngineState()).rules;
    await expect(backend.setEngineRules({ ...rules, enabled: true })).rejects.toThrow('non valide');
    let state = await backend.setEngineRules({ ...rules, enabled: true, primary: 6 });
    expect(state.view).toMatchObject({ suspended: 'conflict', detail: ['task:g'] });
    state = await backend.setEngineRules({
      ...rules,
      enabled: true,
      primary: 6,
      ignoredConflicts: ['TASK:G'],
    });
    expect(state.view?.suspended).toBeNull();
  });

  it('unclaimed --purge toglie la voce e emette lo stato', async () => {
    const backend = new FakeBackend({ unclaimed: ['a', 'b'] });
    let seen: string[] | undefined;
    backend.onStateChanged((s) => (seen = s.unclaimedCredentials));
    await backend.runAction({ kind: 'unclaimedPurge', id: 'a' });
    expect(seen).toEqual(['b']);
    await backend.runAction({ kind: 'unclaimedPurge', id: 'b' });
    expect(seen).toBeUndefined();
    await expect(backend.runAction({ kind: 'unclaimedPurge', id: 'z' })).rejects.toThrow();
  });

  it('add-token crea l’account e non lascia il token nel registro delle azioni', async () => {
    const backend = new FakeBackend();
    await backend.runAction({ kind: 'addToken', token: 'sk-ant-SEGRETO', slot: 9 });
    expect((await backend.listAccounts()).accounts.some((a) => a.number === 9)).toBe(true);
    expect(JSON.stringify(backend.actionLog)).not.toContain('SEGRETO');
    await expect(backend.runAction({ kind: 'addToken', token: 't', slot: 9 })).rejects.toThrow(
      'already occupied',
    );
    await backend.runAction({ kind: 'addToken', token: 't', slot: 9, overwrite: true });
    await backend.runAction({ kind: 'addToken', token: 't' });
  });

  it('config unset torna al default; purge svuota tutto', async () => {
    const backend = new FakeBackend({ mappings: [{ path: 'x', account: 1 }] });
    await backend.runAction({ kind: 'configSet', key: 'autoswitch.threshold', value: '80' });
    await backend.runAction({ kind: 'configUnset', key: 'autoswitch.threshold' });
    expect(await backend.getConfigValue('autoswitch.threshold')).toMatchObject({
      value: 90,
      isSet: false,
    });
    await backend.runAction({ kind: 'configUnset', key: 'autoswitch.model' });
    await backend.runAction({ kind: 'configUnset', key: 'autoswitch.includeApiKeyAccounts' });
    await backend.runAction({ kind: 'configUnset', key: 'autoswitch.strategy' });
    await expect(backend.runAction({ kind: 'configUnset', key: 'nope' })).rejects.toThrow();
    await backend.runAction({ kind: 'purge' });
    expect((await backend.listAccounts()).accounts).toEqual([]);
    expect(await backend.listMappings()).toEqual([]);
  });

  it('preferenze, avvio, selettori ed eventi', async () => {
    const backend = new FakeBackend({ picks: { folder: null } });
    expect(await backend.setPreferences({ pollIntervalSeconds: 30 })).toMatchObject({
      pollIntervalSeconds: 30,
      notificationThresholds: [80, 95],
    });
    expect((await backend.setAutostart(false)).autostart).toBe(false);
    expect(await backend.pickFolder()).toBeNull();
    expect(await backend.pickOpenFile()).toContain('import');
    expect(await backend.pickSaveFile()).toContain('export');
    const events = await backend.getHistoryEvents();
    expect(events.switches.length).toBeGreaterThan(0);
    const narrow = await backend.getHistoryEvents({ sinceSeconds: Date.now() / 1000 - 3600 });
    expect(narrow.switches.length).toBeLessThan(events.switches.length);
    expect((await new FakeBackend({ scenario: 'noAccounts' }).getHistoryEvents()).switches).toEqual(
      [],
    );
  });
});
