import type { Account, Usage, UsageStatus } from '@swapdeck/types';
import type { MessageKey, Translate } from '../i18n/index.js';

export type Tone = 'ok' | 'warn' | 'danger' | 'neutral';

export function accountName(account: Account, t: Translate): string {
  return account.alias && account.alias.length > 0
    ? account.alias
    : t('account.slot', { number: account.number });
}

/** Etichetta e tono del badge di stato; `null` quando lo stato è normale (`ok`). */
export function statusBadge(status: UsageStatus): { key: MessageKey; tone: Tone } | null {
  switch (status) {
    case 'ok':
      return null;
    case 'relogin_required':
      return { key: 'account.relogin', tone: 'danger' };
    case 'token_expired':
      return { key: 'account.tokenExpired', tone: 'warn' };
    case 'unavailable':
      return { key: 'account.unavailable', tone: 'warn' };
    case 'api_key':
      return { key: 'account.apiKey', tone: 'neutral' };
    case 'no_credentials':
      return { key: 'account.noCredentials', tone: 'danger' };
    case 'foreign_credential':
      return { key: 'account.foreign', tone: 'warn' };
    case 'keychain_unavailable':
      return { key: 'account.keychain', tone: 'warn' };
    default:
      return { key: 'account.unavailable', tone: 'neutral' };
  }
}

export interface DisplayUsage {
  usage: Usage;
  /** `true` se viene da `lastGoodUsage`: un dato vecchio, da mostrare ma non da usare per decidere. */
  lastKnown: boolean;
  ageSeconds: number | null;
}

/** Il dato da mostrare: i consumi attuali, altrimenti l'ultimo dato noto. */
export function displayUsage(account: Account): DisplayUsage | null {
  if (account.usage) {
    return { usage: account.usage, lastKnown: false, ageSeconds: account.usageAgeSeconds ?? null };
  }
  if (account.lastGoodUsage) {
    return {
      usage: account.lastGoodUsage,
      lastKnown: true,
      ageSeconds: account.lastGoodAgeSeconds ?? null,
    };
  }
  return null;
}

/** Perché non si può passare a questo account (`null` se si può). */
export function switchBlockedReason(account: Account): MessageKey | null {
  if (account.active) return 'account.alreadyActive';
  if (account.disabled === true) return 'account.cannotSwitch.disabled';
  if (account.usageStatus === 'relogin_required') return 'account.cannotSwitch.relogin';
  return null;
}
