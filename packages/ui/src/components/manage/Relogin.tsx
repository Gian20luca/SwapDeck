import type {
  Account,
  ListResponse,
  ReloginEvent,
  ReloginOutcome,
  ReloginStep,
} from '@swapdeck/types';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useBackend } from '../../backend/context.js';
import { hasRelogin } from '../../backend/types.js';
import { errorMessage } from '../../backend/useAccounts.js';
import type { Notice } from '../../backend/useActionRunner.js';
import { useI18n, type MessageKey, type Translate } from '../../i18n/index.js';
import { accountName } from '../../lib/account.js';
import { Tooltip } from '../Tooltip.js';
import { copyToClipboard } from './DiagnosticsSection.js';
import { NoticeView, TextField } from './common.js';

/** Passi mostrati, nell'ordine (quelli non avviati restano in attesa). */
const STEPS: readonly ReloginStep[] = [
  'saveActive',
  'startLogin',
  'waitCode',
  'verify',
  'saveAccount',
  'restoreActive',
];

const MARK = { pending: '○', running: '◌', done: '✓' } as const;

/** Stato di un «Rifai login» in corso. */
export interface ReloginState {
  account: number;
  name: string;
  /** Passi avviati, nell'ordine di arrivo. */
  started: ReloginStep[];
  url: string | null;
  timeoutSeconds: number | null;
  codeSent: boolean;
  codeInvalid: boolean;
}

/** Applica un evento del core allo stato mostrato. */
export function reduceRelogin(state: ReloginState, event: ReloginEvent): ReloginState {
  switch (event.event) {
    case 'stepStarted':
      return state.started.includes(event.step)
        ? state
        : { ...state, started: [...state.started, event.step] };
    case 'link':
      return { ...state, url: event.url, timeoutSeconds: event.timeoutSeconds };
    case 'codeSent':
      return { ...state, codeSent: true, codeInvalid: false };
    case 'codeInvalid':
      return { ...state, codeInvalid: true };
  }
}

/** Testo dell'esito per l'avviso della dashboard. */
export function reloginNotice(outcome: ReloginOutcome, name: string, t: Translate): Notice {
  const parts: string[] = [];
  const restoreText = () => {
    if (outcome.previous === undefined) return t('relogin.result.noPrevious');
    return outcome.restored ? t('relogin.result.restored') : t('relogin.result.notRestored');
  };
  switch (outcome.result) {
    case 'updated':
      parts.push(t('relogin.result.updated', { name }));
      if (outcome.previous === undefined) parts.push(t('relogin.result.noPrevious'));
      else if (outcome.previous !== outcome.account) parts.push(restoreText());
      break;
    case 'wrongAccount':
      parts.push(t('relogin.result.wrongAccount', { name }), restoreText());
      break;
    case 'claudeNotFound':
      parts.push(t('relogin.result.claudeNotFound'));
      break;
    default: {
      const key: MessageKey = `relogin.result.${outcome.result}`;
      parts.push(t(key));
      if (outcome.result === 'loginFailed' && outcome.detail) parts.push(outcome.detail);
      if (!outcome.restored && outcome.previous !== undefined) {
        parts.push(t('relogin.result.notRestored'));
      }
    }
  }
  if (outcome.warning) parts.push(t('relogin.result.warning', { warning: outcome.warning }));
  const ok = outcome.result === 'updated' || outcome.result === 'cancelled';
  return { tone: ok ? 'info' : 'danger', text: parts.join(' ') };
}

/** Utenze per cui mostrare «Rifai login» (criterio nel core); vuoto se l'host non lo offre. */
export function useReloginAccounts(list: ListResponse | undefined): ReadonlySet<number> {
  const backend = useBackend();
  const [numbers, setNumbers] = useState<ReadonlySet<number>>(new Set());
  useEffect(() => {
    if (!list || !hasRelogin(backend)) {
      setNumbers(new Set());
      return;
    }
    let live = true;
    backend
      .reloginAccounts(list)
      .then((n) => live && setNumbers(new Set(n)))
      .catch(() => live && setNumbers(new Set()));
    return () => {
      live = false;
    };
  }, [backend, list]);
  return numbers;
}

/** Controllo di un «Rifai login»: uno alla volta; annullato se la vista si chiude a metà. */
export function useRelogin(onDone: (notice: Notice) => void) {
  const backend = useBackend();
  const { t } = useI18n();
  const [state, setState] = useState<ReloginState | null>(null);
  const running = useRef(false);
  const done = useRef(onDone);
  done.current = onDone;

  useEffect(
    () => () => {
      if (running.current && hasRelogin(backend)) void backend.cancelRelogin().catch(() => {});
    },
    [backend],
  );

  const start = useCallback(
    async (account: Account) => {
      if (!hasRelogin(backend) || running.current) return;
      running.current = true;
      const name = accountName(account, t);
      setState({
        account: account.number,
        name,
        started: [],
        url: null,
        timeoutSeconds: null,
        codeSent: false,
        codeInvalid: false,
      });
      try {
        const outcome = await backend.startRelogin(account.number, (event) =>
          setState((s) => (s ? reduceRelogin(s, event) : s)),
        );
        done.current(reloginNotice(outcome, name, t));
      } catch (error) {
        done.current({
          tone: 'danger',
          text: t('relogin.failed', { message: errorMessage(error) }),
        });
      } finally {
        running.current = false;
        setState(null);
      }
    },
    [backend, t],
  );

  const submit = useCallback(
    async (code: string) => {
      if (!hasRelogin(backend)) return;
      setState((s) => (s ? { ...s, codeInvalid: false } : s));
      await backend.submitReloginCode(code);
    },
    [backend],
  );

  const cancel = useCallback(async () => {
    if (hasRelogin(backend)) await backend.cancelRelogin();
  }, [backend]);

  return { state, start, submit, cancel, available: hasRelogin(backend) };
}

export type ReloginControl = ReturnType<typeof useRelogin>;

/** Pulsante «Rifai login» sulla scheda o nel dettaglio dell'utenza. */
export function ReloginButton({ account, control }: { account: Account; control: ReloginControl }) {
  const { t } = useI18n();
  const name = accountName(account, t);
  return (
    <Tooltip text={t('relogin.open.tip')}>
      <button
        type="button"
        className="sd-btn sd-btn--small"
        aria-label={t('relogin.openLabel', { name })}
        disabled={control.state !== null}
        onClick={() => void control.start(account)}
      >
        {t('relogin.open')}
      </button>
    </Tooltip>
  );
}

/** Riquadro dell'operazione in corso: passi, link con «Copia link», codice, «Annulla». */
export function ReloginPanel({ control }: { control: ReloginControl }) {
  const { t } = useI18n();
  const backend = useBackend();
  const [code, setCode] = useState('');
  const [notice, setNotice] = useState<Notice | null>(null);
  const state = control.state;
  const heading = useRef<HTMLHeadingElement>(null);
  const account = state?.account;
  useEffect(() => {
    setCode('');
    setNotice(null);
    if (account !== undefined) heading.current?.focus();
  }, [account]);
  if (!state) return null;

  const current = state.started[state.started.length - 1];
  const stepState = (step: ReloginStep): keyof typeof MARK => {
    if (!state.started.includes(step)) return 'pending';
    return step === current ? 'running' : 'done';
  };
  const waiting = state.url !== null && !state.started.includes('verify');

  return (
    <section className="sd-notice sd-relogin" aria-labelledby="sd-relogin-title">
      <h3 id="sd-relogin-title" ref={heading} tabIndex={-1}>
        {t('relogin.title', { name: state.name })}
      </h3>
      <p>{t('relogin.help')}</p>
      <p className="sd-hint">{t('relogin.engine')}</p>
      <ol className="sd-steps">
        {STEPS.filter((s) => s !== 'saveActive' || state.started.includes(s)).map((step) => {
          const s = stepState(step);
          return (
            <li key={step} data-state={s}>
              <span className="sd-steps__mark" aria-hidden="true">
                {MARK[s]}
              </span>
              <span>{t(`relogin.step.${step}`)}</span>
            </li>
          );
        })}
      </ol>
      {state.url === null ? (
        <p role="status">{t('relogin.preparing')}</p>
      ) : (
        <div className="sd-form">
          <div className="sd-form__field">
            <label htmlFor="sd-relogin-url">{t('relogin.link')}</label>
            <textarea
              id="sd-relogin-url"
              className="sd-input sd-relogin__url"
              readOnly
              rows={3}
              value={state.url}
              onFocus={(e) => e.currentTarget.select()}
            />
          </div>
          <div className="sd-row">
            <Tooltip text={t('relogin.copy.tip')}>
              <button
                type="button"
                className="sd-btn sd-btn--small"
                onClick={() => {
                  const url = state.url ?? '';
                  copyToClipboard(backend, url).then(
                    () => setNotice({ tone: 'info', text: t('relogin.copied') }),
                    () => setNotice({ tone: 'danger', text: t('relogin.copyFailed') }),
                  );
                }}
              >
                {t('relogin.copy')}
              </button>
            </Tooltip>
          </div>
          {state.timeoutSeconds !== null && (
            <p className="sd-hint">
              {t('relogin.timeout', { minutes: Math.round(state.timeoutSeconds / 60) })}
            </p>
          )}
          {waiting && (
            <form
              className="sd-form"
              onSubmit={(event) => {
                event.preventDefault();
                const value = code.trim();
                if (!value) return;
                setCode('');
                control.submit(value).catch((error: unknown) =>
                  setNotice({
                    tone: 'danger',
                    text: t('relogin.failed', { message: errorMessage(error) }),
                  }),
                );
              }}
            >
              <TextField
                label={t('relogin.code')}
                tip={t('relogin.code.tip')}
                value={code}
                onChange={setCode}
                secret
                error={state.codeInvalid ? t('relogin.codeInvalid') : null}
              />
              <div className="sd-row">
                <Tooltip text={t('relogin.confirm.tip')}>
                  <button
                    type="submit"
                    className="sd-btn sd-btn--primary sd-btn--small"
                    disabled={code.trim() === ''}
                  >
                    {t('relogin.confirm')}
                  </button>
                </Tooltip>
              </div>
            </form>
          )}
          {state.codeSent && waiting && <p role="status">{t('relogin.codeSent')}</p>}
        </div>
      )}
      <NoticeView notice={notice} />
      {/* Dopo il login non si annulla più: restano verifica e salvataggio. */}
      {!state.started.includes('verify') && (
        <div className="sd-row">
          <Tooltip text={t('relogin.cancel.tip')}>
            <button
              type="button"
              className="sd-btn sd-btn--small"
              onClick={() => void control.cancel().catch(() => {})}
            >
              {t('common.cancel')}
            </button>
          </Tooltip>
        </div>
      )}
    </section>
  );
}
