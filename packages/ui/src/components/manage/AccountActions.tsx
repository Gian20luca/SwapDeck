import type { Account } from '@swapdeck/types';
import { useId, useState } from 'react';
import { actions } from '../../backend/actions.js';
import { useActionRunner } from '../../backend/useActionRunner.js';
import { useI18n } from '../../i18n/index.js';
import { accountName } from '../../lib/account.js';
import { NoticeView, SelectField, TextField } from './common.js';

/** Alias di cswap: lettere, cifre, `.`, `-`, `_`; non solo cifre. */
export const ALIAS_PATTERN = /^[A-Za-z0-9._-]+$/;

export function isValidAlias(alias: string): boolean {
  return ALIAS_PATTERN.test(alias) && !/^\d+$/.test(alias);
}

export interface AccountActionsProps {
  account: Account;
  /** Tutti gli account, per lo scambio. */
  accounts: Account[];
}

/** Azioni di gestione di un account: alias, disattivazione, slot, terminale, rimozione. */
export function AccountActions({ account, accounts }: AccountActionsProps) {
  const { t } = useI18n();
  const { run, busy, notice, dialog } = useActionRunner();
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const name = accountName(account, t);
  const [alias, setAlias] = useState(account.alias ?? '');
  const [slot, setSlot] = useState('');
  const others = accounts.filter((a) => a.number !== account.number);
  const [swapWith, setSwapWith] = useState('');
  const swapTarget = swapWith || (others[0] ? String(others[0].number) : '');
  const disabled = account.disabled === true;

  const aliasTrimmed = alias.trim();
  const aliasError =
    aliasTrimmed.length > 0 && !isValidAlias(aliasTrimmed) ? t('manage.alias.invalid') : null;
  const slotNumber = Number(slot);
  const slotValid = slot.trim() !== '' && Number.isInteger(slotNumber) && slotNumber >= 1;
  const slotTaken = slotValid && accounts.some((a) => a.number === slotNumber);

  return (
    <div className="sd-card__manage">
      <button
        type="button"
        className="sd-btn sd-btn--small"
        aria-expanded={open}
        aria-controls={panelId}
        aria-label={t(open ? 'manage.hideLabel' : 'manage.showLabel', { name })}
        onClick={() => setOpen((v) => !v)}
      >
        {t('manage.title')}
      </button>
      {open && (
        <div
          id={panelId}
          className="sd-manage"
          role="group"
          aria-label={t('manage.group', { name })}
        >
          <div className="sd-form">
            <TextField
              label={t('manage.alias.label')}
              value={alias}
              onChange={setAlias}
              hint={t('manage.alias.hint')}
              error={aliasError}
              disabled={busy}
            />
            <div className="sd-row">
              <button
                type="button"
                className="sd-btn sd-btn--small"
                disabled={busy || aliasTrimmed.length === 0 || aliasError !== null}
                onClick={() =>
                  void run(actions.setAlias(account.number, aliasTrimmed), {
                    success: t('manage.alias.done', { name: aliasTrimmed }),
                  })
                }
              >
                {t('manage.alias.set')}
              </button>
              {account.alias && (
                <button
                  type="button"
                  className="sd-btn sd-btn--small"
                  disabled={busy}
                  onClick={() =>
                    void run(actions.setAlias(account.number, null), {
                      success: t('manage.alias.removed'),
                    }).then((r) => r.ok && setAlias(''))
                  }
                >
                  {t('manage.alias.unset')}
                </button>
              )}
            </div>
          </div>

          <div className="sd-row">
            <button
              type="button"
              className="sd-btn sd-btn--small"
              disabled={busy}
              onClick={() =>
                void run(
                  disabled ? actions.enable(account.number) : actions.disable(account.number),
                  {
                    copy: {
                      title: t('manage.disable.title', { name }),
                      message: t('manage.disable.message'),
                      confirmLabel: t('manage.disable'),
                    },
                    success: t(disabled ? 'manage.enable.done' : 'manage.disable.done', { name }),
                  },
                )
              }
            >
              {disabled ? t('manage.enable') : t('manage.disable')}
            </button>
            <button
              type="button"
              className="sd-btn sd-btn--small"
              disabled={busy}
              onClick={() =>
                void run(actions.run(account.number), {
                  success: t('manage.run.done', { name }),
                })
              }
            >
              {t('manage.run')}
            </button>
          </div>

          <div className="sd-form">
            <TextField
              label={t('manage.move.label')}
              type="number"
              min={1}
              value={slot}
              onChange={setSlot}
              hint={slotTaken ? t('manage.move.taken') : t('manage.move.hint')}
              disabled={busy}
            />
            <div className="sd-row">
              <button
                type="button"
                className="sd-btn sd-btn--small"
                disabled={busy || !slotValid || slotNumber === account.number}
                onClick={() =>
                  void run(actions.move(account.number, slotNumber), {
                    copy: {
                      title: t('manage.move.title', { name }),
                      message: slotTaken
                        ? t('manage.move.confirmSwap', { slot: slotNumber })
                        : t('manage.move.confirm', { slot: slotNumber }),
                      confirmLabel: t('manage.move.action'),
                    },
                    success: t('manage.move.done', { slot: slotNumber }),
                  }).then((r) => r.ok && setSlot(''))
                }
              >
                {t('manage.move.action')}
              </button>
            </div>
          </div>

          {others.length > 0 && (
            <div className="sd-form">
              <SelectField
                label={t('manage.swap.label')}
                value={swapTarget}
                onChange={setSwapWith}
                disabled={busy}
                options={others.map((a) => ({
                  value: String(a.number),
                  label: `${accountName(a, t)} (#${a.number})`,
                }))}
              />
              <div className="sd-row">
                <button
                  type="button"
                  className="sd-btn sd-btn--small"
                  disabled={busy || swapTarget === ''}
                  onClick={() =>
                    void run(actions.swap(account.number, Number(swapTarget)), {
                      copy: {
                        title: t('manage.swap.title'),
                        message: t('manage.swap.message', {
                          a: account.number,
                          b: swapTarget,
                        }),
                        confirmLabel: t('manage.swap.action'),
                      },
                      success: t('manage.swap.done'),
                    })
                  }
                >
                  {t('manage.swap.action')}
                </button>
              </div>
            </div>
          )}

          <div className="sd-row">
            <button
              type="button"
              className="sd-btn sd-btn--small sd-btn--danger"
              disabled={busy}
              onClick={() =>
                void run(actions.remove(account.number), {
                  copy: {
                    title: t('manage.remove.title', { name }),
                    message: t('manage.remove.message', { email: account.email }),
                    confirmLabel: t('manage.remove'),
                  },
                  success: t('manage.remove.done', { name }),
                })
              }
            >
              {t('manage.remove')}
            </button>
          </div>
        </div>
      )}
      <NoticeView notice={notice} />
      {dialog}
    </div>
  );
}
