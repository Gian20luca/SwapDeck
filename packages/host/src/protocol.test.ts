import { describe, expect, it } from 'vitest';
import protocol from '../protocol.json' with { type: 'json' };
import {
  REQUEST_METHODS,
  isRequestMethod,
  preferredLocaleOf,
  suspensionOf,
  viewLocale,
  type RequestMethod,
} from './protocol.js';

/**
 * Tutti i metodi di `RequestParams`: un metodo mancante o in più è un errore di compilazione, e il
 * test sotto controlla che `protocol.json` elenchi esattamente gli stessi.
 */
const TYPED_METHODS: Record<RequestMethod, true> = {
  listAccounts: true,
  getStatus: true,
  getConfig: true,
  getConfigValue: true,
  getHistory: true,
  confirmationFor: true,
  runAction: true,
  listMappings: true,
  getEngineState: true,
  setEngineRules: true,
  getEngineProfiles: true,
  applyEngineProfile: true,
  getNotificationSettings: true,
  setNotificationSettings: true,
  getPreferences: true,
  setPreferences: true,
  getHistoryEvents: true,
  getUsageBreakdown: true,
  pickOpenFile: true,
  pickSaveFile: true,
  pickFolder: true,
  diagnoseSetup: true,
  installCswap: true,
  upgradeCswap: true,
  checkCswapUpdate: true,
  dismissCswapUpdate: true,
  getDiagnostics: true,
  openLogsFolder: true,
  copyText: true,
  reloginAccounts: true,
  startRelogin: true,
  submitReloginCode: true,
  cancelRelogin: true,
};

describe('metodi delle richieste', () => {
  it('protocol.json elenca esattamente i metodi di RequestParams, senza doppioni', () => {
    expect([...REQUEST_METHODS].sort()).toEqual(Object.keys(TYPED_METHODS).sort());
    expect(new Set(protocol.requestMethods).size).toBe(protocol.requestMethods.length);
  });

  it('isRequestMethod accetta solo i metodi elencati', () => {
    expect(isRequestMethod('listAccounts')).toBe(true);
    expect(isRequestMethod('cancelRelogin')).toBe(true);
    expect(isRequestMethod('toString')).toBe(false);
    expect(isRequestMethod(1)).toBe(false);
  });
});

describe('suspensionOf', () => {
  it('solo con un motivo; dettagli vuoti se mancano', () => {
    expect(suspensionOf(undefined)).toBeNull();
    expect(suspensionOf({})).toBeNull();
    expect(suspensionOf({ suspended: 'conflict' })).toEqual({ reason: 'conflict', detail: [] });
  });
});

describe('lingua delle viste', () => {
  it('la scelta di SwapDeck vince su quella dell’IDE', () => {
    expect(viewLocale('it', 'en')).toBe('it');
    expect(viewLocale('en', 'it')).toBe('en');
  });

  it('senza scelta segue l’IDE', () => {
    expect(viewLocale(undefined, 'it')).toBe('it');
    expect(viewLocale(undefined, 'IT-ch')).toBe('it');
    expect(viewLocale(undefined, 'en-us')).toBe('en');
    expect(viewLocale(undefined, 'de')).toBe('en');
  });

  it('accetta solo le lingue note dalle preferenze', () => {
    expect(preferredLocaleOf('it')).toBe('it');
    expect(preferredLocaleOf('en')).toBe('en');
    expect(preferredLocaleOf('fr')).toBeUndefined();
    expect(preferredLocaleOf(undefined)).toBeUndefined();
  });
});
