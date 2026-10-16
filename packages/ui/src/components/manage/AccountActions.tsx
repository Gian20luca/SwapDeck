import type { Account } from '@swapdeck/types';
import { useI18n } from '../../i18n/index.js';
import { accountName } from '../../lib/account.js';
import { Tooltip } from '../Tooltip.js';

/** Alias di cswap: lettere, cifre, `.`, `-`, `_`; non solo cifre. */
export const ALIAS_PATTERN = /^[A-Za-z0-9._-]+$/;

export function isValidAlias(alias: string): boolean {
  return ALIAS_PATTERN.test(alias) && !/^\d+$/.test(alias);
}

export interface AccountActionsProps {
  account: Account;
  /** Apre la pagina di dettaglio dell'utenza. */
  onManage: (number: number) => void;
}

/** «Gestisci utenza» sulla scheda dell'account: apre la pagina di dettaglio (la lista resta snella). */
export function AccountActions({ account, onManage }: AccountActionsProps) {
  const { t } = useI18n();
  const name = accountName(account, t);
  return (
    <div className="sd-card__manage">
      <Tooltip text={t('detail.open.tip')}>
        <button
          type="button"
          className="sd-btn sd-btn--small"
          aria-label={t('manage.showLabel', { name })}
          onClick={() => onManage(account.number)}
        >
          {t('detail.open')}
        </button>
      </Tooltip>
    </div>
  );
}
