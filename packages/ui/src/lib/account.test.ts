import type { Account } from '@swapdeck/types';
import { describe, expect, it } from 'vitest';
import { translate } from '../i18n/index.js';
import { accountName, displayUsage, statusBadge, switchBlockedReason } from './account.js';

const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) =>
  translate('it', key, params);

function account(extra: Partial<Account> = {}): Account {
  return {
    number: 4,
    email: 'user4@example.com',
    organizationName: '',
    organizationUuid: '',
    isOrganization: false,
    active: false,
    usageStatus: 'ok',
    usage: null,
    ...extra,
  };
}

describe('accountName', () => {
  it('usa l’alias, altrimenti il numero di slot', () => {
    expect(accountName(account({ alias: 'lavoro' }), t)).toBe('lavoro');
    expect(accountName(account(), t)).toBe('Account 4');
    expect(accountName(account({ alias: '' }), t)).toBe('Account 4');
  });
});

describe('statusBadge', () => {
  it('nessun badge per ok, tono corretto per gli altri', () => {
    expect(statusBadge('ok')).toBeNull();
    expect(statusBadge('relogin_required')).toEqual({ key: 'account.relogin', tone: 'danger' });
    expect(statusBadge('token_expired')?.tone).toBe('warn');
    expect(statusBadge('api_key')?.tone).toBe('neutral');
    expect(statusBadge('no_credentials')?.tone).toBe('danger');
    expect(statusBadge('foreign_credential')?.tone).toBe('warn');
    expect(statusBadge('keychain_unavailable')?.tone).toBe('warn');
    expect(statusBadge('unavailable')?.tone).toBe('warn');
  });
});

describe('displayUsage', () => {
  it('preferisce i consumi attuali, poi l’ultimo dato noto, poi niente', () => {
    const now = displayUsage(account({ usage: { fiveHour: { pct: 1 } }, usageAgeSeconds: 12 }));
    expect(now).toMatchObject({ lastKnown: false, ageSeconds: 12 });
    const old = displayUsage(
      account({
        usageStatus: 'unavailable',
        lastGoodUsage: { sevenDay: { pct: 9 } },
        lastGoodAgeSeconds: 900,
      }),
    );
    expect(old).toMatchObject({ lastKnown: true, ageSeconds: 900 });
    expect(displayUsage(account())).toBeNull();
  });
});

describe('switchBlockedReason', () => {
  it('spiega perché non si può cambiare', () => {
    expect(switchBlockedReason(account())).toBeNull();
    expect(switchBlockedReason(account({ active: true }))).toBe('account.alreadyActive');
    expect(switchBlockedReason(account({ disabled: true }))).toBe('account.cannotSwitch.disabled');
    expect(switchBlockedReason(account({ usageStatus: 'relogin_required' }))).toBe(
      'account.cannotSwitch.relogin',
    );
  });
});
