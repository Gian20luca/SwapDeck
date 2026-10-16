import { useId, useState } from 'react';
import { actions } from '../../backend/actions.js';
import { useActionRunner } from '../../backend/useActionRunner.js';
import { useI18n } from '../../i18n/index.js';
import { CheckField, NoticeView, TextField } from './common.js';

type Mode = 'current' | 'token';

/** `undefined` = campo vuoto (nessuno slot), `null` = non valido. */
function parseSlot(value: string): number | undefined | null {
  if (value.trim() === '') return undefined;
  const n = Number(value);
  return Number.isInteger(n) && n >= 1 ? n : null;
}

/** Aggiunta di un account: quello loggato in Claude Code (guida a `/login`) o un token (`add-token`). */
export function AddAccount() {
  const { t } = useI18n();
  const { run, busy, notice, dialog } = useActionRunner();
  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<Mode>('current');
  const [slot, setSlot] = useState('');
  const [alias, setAlias] = useState('');
  const [email, setEmail] = useState('');
  const [token, setToken] = useState('');
  const [overwrite, setOverwrite] = useState(false);
  const panelId = useId();
  const modeName = useId();

  const slotValue = parseSlot(slot);
  const slotError = slotValue === null ? t('manage.add.slotInvalid') : null;

  function reset() {
    setSlot('');
    setAlias('');
    setEmail('');
    setToken('');
    setOverwrite(false);
  }

  async function addCurrent() {
    if (slotValue === null) return;
    const result = await run(
      actions.addCurrent({
        ...(slotValue !== undefined ? { slot: slotValue } : {}),
        ...(alias.trim() ? { alias: alias.trim() } : {}),
        ...(overwrite ? { overwrite: true } : {}),
      }),
      { success: t('manage.add.done') },
    );
    if (result.ok) reset();
  }

  async function addToken() {
    if (slotValue === null || token.trim() === '') return;
    // Il token esce dallo stato prima della chiamata: non resta nella UI né compare nei messaggi.
    const secret = token.trim();
    setToken('');
    const result = await run(
      actions.addToken(secret, {
        ...(email.trim() ? { email: email.trim() } : {}),
        ...(slotValue !== undefined ? { slot: slotValue } : {}),
        ...(overwrite ? { overwrite: true } : {}),
      }),
      { success: t('manage.add.tokenDone') },
    );
    if (result.ok) reset();
  }

  return (
    <section className="sd-section" aria-label={t('manage.add.title')}>
      <div className="sd-toolbar">
        <button
          type="button"
          className="sd-btn"
          aria-expanded={open}
          aria-controls={panelId}
          onClick={() => setOpen((v) => !v)}
        >
          {t('manage.add.title')}
        </button>
      </div>
      {open && (
        <div id={panelId} className="sd-form">
          <div role="radiogroup" aria-label={t('manage.add.mode')} className="sd-row">
            {(['current', 'token'] as const).map((value) => (
              <label key={value} className="sd-radio">
                <input
                  type="radio"
                  name={modeName}
                  value={value}
                  checked={mode === value}
                  onChange={() => setMode(value)}
                />
                {t(value === 'current' ? 'manage.add.current' : 'manage.add.token')}
              </label>
            ))}
          </div>

          {mode === 'current' ? (
            <>
              <ol className="sd-steps">
                <li>{t('manage.add.step1')}</li>
                <li>{t('manage.add.step2')}</li>
              </ol>
              <TextField
                label={t('manage.add.alias')}
                value={alias}
                onChange={setAlias}
                disabled={busy}
              />
            </>
          ) : (
            <>
              <p className="sd-hint">{t('manage.add.tokenHint')}</p>
              <TextField
                label={t('manage.add.tokenLabel')}
                type="password"
                secret
                value={token}
                onChange={setToken}
                disabled={busy}
              />
              <TextField
                label={t('manage.add.email')}
                value={email}
                onChange={setEmail}
                disabled={busy}
              />
            </>
          )}
          <TextField
            label={t('manage.add.slot')}
            type="number"
            min={1}
            value={slot}
            onChange={setSlot}
            hint={t('manage.add.slotHint')}
            error={slotError}
            disabled={busy}
          />
          <CheckField
            label={t('manage.add.overwrite')}
            checked={overwrite}
            onChange={setOverwrite}
            disabled={busy}
          />
          <div className="sd-row">
            <button
              type="button"
              className="sd-btn sd-btn--primary"
              disabled={busy || slotError !== null || (mode === 'token' && token.trim() === '')}
              onClick={() => void (mode === 'current' ? addCurrent() : addToken())}
            >
              {busy ? t('common.working') : t('manage.add.action')}
            </button>
          </div>
        </div>
      )}
      <NoticeView notice={notice} />
      {dialog}
    </section>
  );
}
