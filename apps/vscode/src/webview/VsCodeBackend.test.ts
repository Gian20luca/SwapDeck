import { BackendError } from '@swapdeck/ui';
import { describe, expect, it } from 'vitest';
import type { HostMessage, WebviewMessage } from '../protocol.js';
import { VsCodeBackend } from './VsCodeBackend.js';

function setup() {
  const sent: WebviewMessage[] = [];
  let handler: ((m: HostMessage) => void) | undefined;
  let unsubscribed = false;
  const backend = new VsCodeBackend(
    (m) => sent.push(m),
    (h) => {
      handler = h;
      return () => {
        unsubscribed = true;
      };
    },
  );
  const reply = (m: HostMessage) => handler!(m);
  return { backend, sent, reply, isUnsubscribed: () => unsubscribed };
}

const list = { schemaVersion: 1, activeAccountNumber: 3, accounts: [] };

describe('VsCodeBackend', () => {
  it('inoltra le richieste e risolve con la risposta dello stesso id', async () => {
    const { backend, sent, reply } = setup();
    const a = backend.listAccounts({ refresh: true });
    const b = backend.getConfigValue('k');
    expect(sent).toEqual([
      { type: 'request', id: 1, method: 'listAccounts', params: { refresh: true } },
      { type: 'request', id: 2, method: 'getConfigValue', params: { key: 'k' } },
    ]);
    reply({ type: 'response', id: 2, ok: true, result: { v: 1 } });
    reply({ type: 'response', id: 1, ok: true, result: list });
    await expect(a).resolves.toEqual(list);
    await expect(b).resolves.toEqual({ v: 1 });
  });

  it('metodi di gestione inoltrati con i loro parametri', () => {
    const { backend, sent } = setup();
    void backend.listMappings();
    void backend.getEngineState();
    void backend.setEngineRules({ enabled: false } as never);
    void backend.getPreferences();
    void backend.setPreferences({ theme: 'dark' });
    void backend.getHistoryEvents({ account: 2 });
    void backend.getHistoryEvents();
    expect(sent.map((m) => (m.type === 'request' ? [m.method, m.params] : m.type))).toEqual([
      ['listMappings', {}],
      ['getEngineState', {}],
      ['setEngineRules', { rules: { enabled: false } }],
      ['getPreferences', {}],
      ['setPreferences', { prefs: { theme: 'dark' } }],
      ['getHistoryEvents', { account: 2 }],
      ['getHistoryEvents', {}],
    ]);
  });

  it("dichiara che le conferme le chiede l'host", () => {
    expect(setup().backend.hostConfirms).toBe(true);
  });

  it('rifiuta con BackendError e ignora id sconosciuti', async () => {
    const { backend, reply } = setup();
    const p = backend.getStatus();
    reply({ type: 'response', id: 99, ok: true, result: null });
    reply({ type: 'response', id: 1, ok: false, error: { kind: 'timeout', message: 'lento' } });
    const error = await p.catch((e: unknown) => e);
    expect(error).toBeInstanceOf(BackendError);
    expect((error as BackendError).info.kind).toBe('timeout');
  });

  it('runAction unisce messaggio e avviso e porta la risposta di switch', async () => {
    const { backend, sent, reply } = setup();
    const p = backend.runAction({ kind: 'switch', target: 3 });
    expect(sent[0]).toMatchObject({
      method: 'runAction',
      params: { action: { kind: 'switch', target: 3 } },
    });
    const sw = { switched: true };
    reply({
      type: 'response',
      id: 1,
      ok: true,
      result: { message: 'ok', warning: 'non verificato', switch: sw },
    });
    await expect(p).resolves.toEqual({ message: 'ok — non verificato', switch: sw });
  });

  it('getHistory passa solo i filtri presenti', () => {
    const { backend, sent } = setup();
    void backend.getHistory();
    void backend.getHistory({ account: 2, sinceSeconds: 10 });
    expect(sent.map((m) => (m.type === 'request' ? m.params : null))).toEqual([
      {},
      { account: 2, sinceSeconds: 10 },
    ]);
  });

  it('eventi di stato e di vista ai rispettivi ascoltatori', () => {
    const { backend, reply } = setup();
    const states: unknown[] = [];
    const views: unknown[] = [];
    const off = backend.onStateChanged((s) => states.push(s));
    backend.onViewSettings((v) => views.push(v));
    reply({ type: 'state', list });
    reply({ type: 'view', settings: { locale: 'en', theme: 'dark' } });
    off();
    reply({ type: 'state', list });
    expect(states).toEqual([list]);
    expect(views).toEqual([{ locale: 'en', theme: 'dark' }]);
  });

  it('ready avvisa l’host; dispose rifiuta le richieste in sospeso', async () => {
    const { backend, sent, isUnsubscribed } = setup();
    backend.ready();
    expect(sent[0]).toEqual({ type: 'ready' });
    const p = backend.getConfig();
    backend.dispose();
    await expect(p).rejects.toBeInstanceOf(BackendError);
    expect(isUnsubscribed()).toBe(true);
  });

  it('i selettori nativi diventano richieste all’host', async () => {
    const { backend, sent, reply } = setup();
    const folder = backend.pickFolder({ title: 'Cartella' });
    const open = backend.pickOpenFile({ filters: [{ name: 'JSON', extensions: ['json'] }] });
    const save = backend.pickSaveFile();
    expect(sent).toEqual([
      { type: 'request', id: 1, method: 'pickFolder', params: { title: 'Cartella' } },
      {
        type: 'request',
        id: 2,
        method: 'pickOpenFile',
        params: { filters: [{ name: 'JSON', extensions: ['json'] }] },
      },
      { type: 'request', id: 3, method: 'pickSaveFile', params: {} },
    ]);
    reply({ type: 'response', id: 1, ok: true, result: 'C:/x' });
    reply({ type: 'response', id: 2, ok: true, result: null });
    reply({ type: 'response', id: 3, ok: true, result: 'C:/y' });
    await expect(folder).resolves.toBe('C:/x');
    await expect(open).resolves.toBeNull();
    await expect(save).resolves.toBe('C:/y');
  });

  it('postMessage che lancia diventa BackendError', async () => {
    const backend = new VsCodeBackend(
      () => {
        throw new Error('chiuso');
      },
      () => () => undefined,
    );
    await expect(backend.getStatus()).rejects.toMatchObject({
      info: { kind: 'failed', message: 'chiuso' },
    });
  });
});

describe('VsCodeBackend: SetupBackend', () => {
  it('inoltra le richieste di setup e l’avanzamento della stessa richiesta', async () => {
    const { backend, sent, reply } = setup();
    const events: unknown[] = [];
    const run = backend.upgradeCswap((e) => events.push(e));
    void backend.checkCswapUpdate({ force: true });
    void backend.dismissCswapUpdate('0.28.0');
    expect(sent).toEqual([
      { type: 'request', id: 1, method: 'upgradeCswap', params: {} },
      { type: 'request', id: 2, method: 'checkCswapUpdate', params: { force: true } },
      { type: 'request', id: 3, method: 'dismissCswapUpdate', params: { version: '0.28.0' } },
    ]);
    reply({ type: 'progress', id: 1, event: { event: 'stepStarted', step: 'upgradeCswap' } });
    reply({ type: 'progress', id: 2, event: { event: 'stepDone', step: 'upgradeCswap' } });
    reply({ type: 'progress', id: 99, event: { event: 'stepDone', step: 'upgradeCswap' } });
    reply({ type: 'response', id: 1, ok: true, result: { uvInstalled: false } });
    await expect(run).resolves.toEqual({ uvInstalled: false });
    expect(events).toEqual([{ event: 'stepStarted', step: 'upgradeCswap' }]);
  });

  it('install e diagnosi; l’annullamento dell’host arriva come cancelled', async () => {
    const { backend, sent, reply } = setup();
    void backend.diagnoseSetup();
    const install = backend.installCswap(() => undefined);
    expect(sent.map((m) => (m.type === 'request' ? m.method : m.type))).toEqual([
      'diagnoseSetup',
      'installCswap',
    ]);
    reply({ type: 'response', id: 2, ok: false, error: { kind: 'cancelled', message: 'no' } });
    await expect(install).rejects.toBeInstanceOf(BackendError);
  });
});
