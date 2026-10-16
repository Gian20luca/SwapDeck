import { useState } from 'react';
import { actions } from '../../backend/actions.js';
import { useBackend } from '../../backend/context.js';
import { useActionRunner } from '../../backend/useActionRunner.js';
import { useAccounts } from '../../backend/useAccounts.js';
import { useI18n } from '../../i18n/index.js';
import { accountName } from '../../lib/account.js';
import { CheckField, NoticeView, PathField, Section, SelectField } from './common.js';
import { Tooltip } from '../Tooltip.js';

const JSON_FILTER = [{ name: 'JSON', extensions: ['json'] }];

/** Strumenti: export e import, credenziali parcheggiate (`unclaimed`) e `purge`. */
export function ToolsPage() {
  const { t } = useI18n();
  const backend = useBackend();
  const { state } = useAccounts();
  const { run, busy, notice, dialog } = useActionRunner();
  const accounts = state.status === 'ready' ? state.data.accounts : [];
  const unclaimed = state.status === 'ready' ? (state.data.unclaimedCredentials ?? []) : [];

  const [scope, setScope] = useState('all');
  const [exportPath, setExportPath] = useState('');
  const [importPath, setImportPath] = useState('');
  const [force, setForce] = useState(false);

  const pickSave = backend.pickSaveFile;
  const pickOpen = backend.pickOpenFile;

  return (
    <div className="sd-page">
      <h2>{t('tools.title')}</h2>
      <NoticeView notice={notice} />

      <Section title={t('tools.export')}>
        <div className="sd-notice" data-tone="danger" role="note">
          {t('tools.export.warning')}
        </div>
        <div className="sd-form">
          <SelectField
            label={t('tools.export.scope')}
            tip={t('tools.export.scope.tip')}
            value={scope}
            onChange={setScope}
            disabled={busy}
            options={[
              { value: 'all', label: t('tools.export.all') },
              ...accounts.map((a) => ({
                value: String(a.number),
                label: `${accountName(a, t)} (#${a.number})`,
              })),
            ]}
          />
          <PathField
            label={t('tools.file')}
            tip={t('tools.export.file.tip')}
            value={exportPath}
            onChange={setExportPath}
            {...(pickSave
              ? {
                  onBrowse: () =>
                    pickSave.call(backend, {
                      title: t('tools.export.pickTitle'),
                      defaultPath: 'swapdeck-export.json',
                      filters: JSON_FILTER,
                    }),
                }
              : {})}
            disabled={busy}
          />
          <div className="sd-row">
            <Tooltip text={t('tools.export.action.tip')}>
              <button
                type="button"
                className="sd-btn"
                disabled={busy || exportPath.trim() === ''}
                onClick={() =>
                  void run(
                    actions.exportTo(
                      exportPath.trim(),
                      scope === 'all' ? { full: true } : { account: Number(scope) },
                    ),
                    {
                      copy: {
                        title: t('tools.export.confirmTitle'),
                        message: t('tools.export.confirmMessage', { path: exportPath.trim() }),
                        confirmLabel: t('tools.export.action'),
                      },
                      success: t('tools.export.done', { path: exportPath.trim() }),
                    },
                  )
                }
              >
                {t('tools.export.action')}
              </button>
            </Tooltip>
          </div>
        </div>
      </Section>

      <Section title={t('tools.import')}>
        <div className="sd-form">
          <PathField
            label={t('tools.file')}
            tip={t('tools.import.file.tip')}
            value={importPath}
            onChange={setImportPath}
            {...(pickOpen
              ? {
                  onBrowse: () =>
                    pickOpen.call(backend, {
                      title: t('tools.import.pickTitle'),
                      filters: JSON_FILTER,
                    }),
                }
              : {})}
            disabled={busy}
          />
          <CheckField
            label={t('tools.import.force')}
            tip={t('tools.import.force.tip')}
            hint={t('tools.import.forceHint')}
            checked={force}
            onChange={setForce}
            disabled={busy}
          />
          <div className="sd-row">
            <Tooltip text={t('tools.import.action.tip')}>
              <button
                type="button"
                className="sd-btn"
                disabled={busy || importPath.trim() === ''}
                onClick={() =>
                  void run(actions.importFrom(importPath.trim(), force), {
                    copy: {
                      title: t('tools.import.confirmTitle'),
                      message: t(
                        force ? 'tools.import.confirmForce' : 'tools.import.confirmMessage',
                        {
                          path: importPath.trim(),
                        },
                      ),
                      confirmLabel: t('tools.import.action'),
                    },
                    success: t('tools.import.done'),
                  })
                }
              >
                {t('tools.import.action')}
              </button>
            </Tooltip>
          </div>
        </div>
      </Section>

      <Section title={t('tools.unclaimed')} hint={t('tools.unclaimed.hint')}>
        {unclaimed.length === 0 ? (
          <div className="sd-state">
            <p>{t('tools.unclaimed.empty')}</p>
          </div>
        ) : (
          <ul className="sd-list" aria-label={t('tools.unclaimed')}>
            {unclaimed.map((id) => (
              <li key={id} className="sd-card sd-card--row">
                <span className="sd-card__name sd-path" title={id}>
                  {id}
                </span>
                <Tooltip text={t('tools.unclaimed.purge.tip')}>
                  <button
                    type="button"
                    className="sd-btn sd-btn--small"
                    disabled={busy}
                    aria-label={t('tools.unclaimed.purgeLabel', { id })}
                    onClick={() =>
                      void run(actions.unclaimedPurge(id), {
                        copy: {
                          title: t('tools.unclaimed.confirmTitle'),
                          message: t('tools.unclaimed.confirmMessage', { id }),
                          confirmLabel: t('tools.unclaimed.purge'),
                        },
                        success: t('tools.unclaimed.done', { id }),
                      })
                    }
                  >
                    {t('tools.unclaimed.purge')}
                  </button>
                </Tooltip>
              </li>
            ))}
          </ul>
        )}
      </Section>

      <Section title={t('tools.purge')} hint={t('tools.purge.hint')}>
        <div className="sd-row">
          <Tooltip text={t('tools.purge.action.tip')}>
            <button
              type="button"
              className="sd-btn sd-btn--danger"
              disabled={busy}
              onClick={() =>
                void run(actions.purge(), {
                  copy: {
                    title: t('tools.purge.confirmTitle'),
                    message: t('tools.purge.confirmMessage'),
                    confirmLabel: t('tools.purge.action'),
                  },
                  success: t('tools.purge.done'),
                })
              }
            >
              {t('tools.purge.action')}
            </button>
          </Tooltip>
        </div>
      </Section>
      {dialog}
    </div>
  );
}
