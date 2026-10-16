import type { Account } from '@swapdeck/types';
import { useCallback, useState } from 'react';
import { actions } from '../../backend/actions.js';
import { useBackend } from '../../backend/context.js';
import { useActionRunner } from '../../backend/useActionRunner.js';
import { useAccounts } from '../../backend/useAccounts.js';
import { useI18n } from '../../i18n/index.js';
import { accountName } from '../../lib/account.js';
import { useLoad } from '../../lib/useLoad.js';
import { LoadGate, NoticeView, PathField, SelectField, Section, Unavailable } from './common.js';

/** Cartelle mappate agli account: elenco, `map` con scelta della cartella e `unmap`. */
export function FoldersPage() {
  const { t } = useI18n();
  const backend = useBackend();
  const { state: accountsState } = useAccounts();
  const { run, busy, notice, dialog } = useActionRunner();
  const supported = typeof backend.listMappings === 'function';
  const load = useCallback(() => backend.listMappings?.() ?? Promise.resolve([]), [backend]);
  const { state, reload } = useLoad(load, [load]);
  const [path, setPath] = useState('');
  const [account, setAccount] = useState('');

  const accounts: Account[] = accountsState.status === 'ready' ? accountsState.data.accounts : [];
  const selected = account || (accounts[0] ? String(accounts[0].number) : '');
  const nameOf = (n: number) => {
    const found = accounts.find((a) => a.number === n);
    return found ? accountName(found, t) : t('account.slot', { number: n });
  };
  const onBrowse = backend.pickFolder
    ? () => backend.pickFolder!({ title: t('folders.pickTitle') })
    : undefined;

  if (!supported) return <Unavailable />;

  async function map() {
    const result = await run(actions.map(Number(selected), path.trim()), {
      success: t('folders.mapped', { path: path.trim(), name: nameOf(Number(selected)) }),
    });
    if (result.ok) {
      setPath('');
      await reload();
    }
  }

  async function unmap(target: string) {
    const result = await run(actions.unmap(target), {
      copy: {
        title: t('folders.unmap.title'),
        message: t('folders.unmap.message', { path: target }),
        confirmLabel: t('folders.unmap'),
      },
      success: t('folders.unmapped', { path: target }),
    });
    if (result.ok) await reload();
  }

  return (
    <div className="sd-page">
      <h2>{t('folders.title')}</h2>
      <p className="sd-hint">{t('folders.hint')}</p>
      <NoticeView notice={notice} />

      <Section title={t('folders.list')}>
        <LoadGate state={state} onRetry={() => void reload()}>
          {(mappings) =>
            mappings.length === 0 ? (
              <div className="sd-state">
                <p>{t('folders.empty')}</p>
              </div>
            ) : (
              <ul className="sd-list" aria-label={t('folders.list')}>
                {mappings.map((m) => (
                  <li key={m.path} className="sd-card sd-card--row">
                    <div className="sd-card__title">
                      <span className="sd-card__name sd-path" title={m.path}>
                        {m.path}
                      </span>
                      <span className="sd-card__email">
                        {m.account === null
                          ? t('folders.removed', { email: m.email })
                          : nameOf(m.account)}
                      </span>
                    </div>
                    <button
                      type="button"
                      className="sd-btn sd-btn--small"
                      disabled={busy}
                      aria-label={t('folders.unmapLabel', { path: m.path })}
                      onClick={() => void unmap(m.path)}
                    >
                      {t('folders.unmap')}
                    </button>
                  </li>
                ))}
              </ul>
            )
          }
        </LoadGate>
      </Section>

      <Section title={t('folders.add')}>
        <div className="sd-form">
          <SelectField
            label={t('folders.account')}
            value={selected}
            onChange={setAccount}
            disabled={busy || accounts.length === 0}
            options={accounts.map((a) => ({
              value: String(a.number),
              label: `${accountName(a, t)} (#${a.number})`,
            }))}
          />
          <PathField
            label={t('folders.path')}
            value={path}
            onChange={setPath}
            {...(onBrowse ? { onBrowse } : {})}
            disabled={busy}
          />
          <div className="sd-row">
            <button
              type="button"
              className="sd-btn sd-btn--primary"
              disabled={busy || path.trim() === '' || selected === ''}
              onClick={() => void map()}
            >
              {t('folders.map')}
            </button>
          </div>
        </div>
      </Section>
      {dialog}
    </div>
  );
}
