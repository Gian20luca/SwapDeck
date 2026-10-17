import type { Account } from '@swapdeck/types';
import { useEffect, useRef, useState } from 'react';
import { actions } from '../../backend/actions.js';
import { useActionRunner } from '../../backend/useActionRunner.js';
import { useI18n } from '../../i18n/index.js';
import { accountName } from '../../lib/account.js';
import { Tooltip } from '../Tooltip.js';
import { isValidAlias } from './AccountActions.js';
import { NoticeView, Section, SelectField, TextField } from './common.js';

export interface AccountDetailPageProps {
  /** L'utenza; `undefined` se non c'è più (rimossa o spostata fuori). */
  account: Account | undefined;
  /** Tutti gli account, per lo scambio e gli slot occupati. */
  accounts: Account[];
  /** Torna all'elenco; `notice` è l'esito da mostrare lì (per esempio dopo la rimozione). */
  onBack: (notice?: string) => void;
  /** L'utenza ha cambiato slot (sposta o scambia): la pagina la segue. */
  onNumberChange: (number: number) => void;
}

/**
 * Pagina di modifica di una sola utenza (F19): alias, attiva/disattiva, terminale, slot e rimozione,
 * con «Indietro». Niente scheda né percentuali: basta il nome nel titolo.
 */
export function AccountDetailPage({
  account,
  accounts,
  onBack,
  onNumberChange,
}: AccountDetailPageProps) {
  const { t } = useI18n();
  const { run, busy, notice, dialog } = useActionRunner();
  const back = useRef<HTMLButtonElement>(null);
  const [alias, setAlias] = useState(account?.alias ?? '');
  const [slot, setSlot] = useState('');
  const [swapWith, setSwapWith] = useState('');

  // All'apertura il focus va su «Indietro»: chi usa la tastiera sa dove si trova.
  useEffect(() => back.current?.focus(), []);

  const backButton = (
    <Tooltip text={t('detail.back.tip')}>
      <button ref={back} type="button" className="sd-btn sd-btn--small" onClick={() => onBack()}>
        ← {t('detail.back')}
      </button>
    </Tooltip>
  );

  if (!account) {
    return (
      <div className="sd-page">
        <div className="sd-detail__head">{backButton}</div>
        <div className="sd-notice" role="status">
          {t('detail.gone')}
        </div>
        <NoticeView notice={notice} />
      </div>
    );
  }

  const name = accountName(account, t);
  const others = accounts.filter((a) => a.number !== account.number);
  const swapTarget = swapWith || (others[0] ? String(others[0].number) : '');
  const disabled = account.disabled === true;
  const aliasTrimmed = alias.trim();
  const aliasError =
    aliasTrimmed.length > 0 && !isValidAlias(aliasTrimmed) ? t('manage.alias.invalid') : null;
  const slotNumber = Number(slot);
  const slotValid = slot.trim() !== '' && Number.isInteger(slotNumber) && slotNumber >= 1;
  const slotTaken = slotValid && accounts.some((a) => a.number === slotNumber);

  return (
    <div className="sd-page" role="region" aria-label={t('manage.group', { name })}>
      <div className="sd-detail__head">
        {backButton}
        <h2>{t('detail.title', { name })}</h2>
      </div>

      <Section title={t('detail.name')} info={t('info.detail.name')}>
        <div className="sd-form">
          <TextField
            label={t('manage.alias.label')}
            tip={t('detail.alias.tip')}
            value={alias}
            onChange={setAlias}
            hint={t('manage.alias.hint')}
            error={aliasError}
            disabled={busy}
          />
          <div className="sd-row">
            <Tooltip text={t('detail.alias.set.tip')}>
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
            </Tooltip>
            {account.alias && (
              <Tooltip text={t('detail.alias.unset.tip')}>
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
              </Tooltip>
            )}
          </div>
        </div>
      </Section>

      <Section title={t('detail.use')} info={t('info.detail.use')}>
        <div className="sd-row">
          <Tooltip text={t(disabled ? 'detail.enable.tip' : 'detail.disable.tip')}>
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
          </Tooltip>
          <Tooltip text={t('detail.run.tip')}>
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
          </Tooltip>
        </div>
      </Section>

      <Section title={t('detail.slot')} info={t('info.detail.slot')}>
        <div className="sd-form">
          <TextField
            label={t('manage.move.label')}
            tip={t('detail.move.field.tip')}
            type="number"
            min={1}
            value={slot}
            onChange={setSlot}
            hint={slotTaken ? t('manage.move.taken') : t('manage.move.hint')}
            disabled={busy}
          />
          <div className="sd-row">
            <Tooltip text={t('detail.move.tip')}>
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
                  }).then((r) => {
                    if (!r.ok) return;
                    setSlot('');
                    onNumberChange(slotNumber);
                  })
                }
              >
                {t('manage.move.action')}
              </button>
            </Tooltip>
          </div>
        </div>
        {others.length > 0 && (
          <div className="sd-form">
            <SelectField
              label={t('manage.swap.label')}
              tip={t('detail.swap.field.tip')}
              value={swapTarget}
              onChange={setSwapWith}
              disabled={busy}
              options={others.map((a) => ({
                value: String(a.number),
                label: `${accountName(a, t)} (#${a.number})`,
              }))}
            />
            <div className="sd-row">
              <Tooltip text={t('detail.swap.tip')}>
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
                    }).then((r) => r.ok && onNumberChange(Number(swapTarget)))
                  }
                >
                  {t('manage.swap.action')}
                </button>
              </Tooltip>
            </div>
          </div>
        )}
      </Section>

      <Section title={t('detail.danger')} info={t('info.detail.danger')}>
        <div className="sd-row">
          <Tooltip text={t('detail.remove.tip')}>
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
                }).then((r) => r.ok && onBack(t('manage.remove.done', { name })))
              }
            >
              {t('manage.remove')}
            </button>
          </Tooltip>
        </div>
      </Section>
      <NoticeView notice={notice} />
      {dialog}
    </div>
  );
}
