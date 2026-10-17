import type { NotificationConfig, NotifyScope } from '@swapdeck/types';
import { useCallback, useEffect, useId, useMemo, useRef, useState, type ReactNode } from 'react';
import { useBackend } from '../../backend/context.js';
import type { Notice } from '../../backend/useActionRunner.js';
import { errorMessage } from '../../backend/useAccounts.js';
import type { NotificationPatch } from '../../backend/types.js';
import { useI18n, type MessageKey } from '../../i18n/index.js';
import { useLoad } from '../../lib/useLoad.js';
import { Tooltip } from '../Tooltip.js';
import { CheckField, LoadGate, NoticeView, Section, SelectField, TextField } from './common.js';

/** Campi numerici: percorso «avviso.campo» → limiti (come `NotificationConfig::validate` del Rust). */
const NUMBERS = {
  'switchPreview.pointsBefore': [0, 50],
  'switchPreview.minutesBefore': [0, 600],
  'resetSoon.minutesBefore': [1, 1440],
  'login.daysBefore': [1, 60],
  'stale.minutes': [1, 1440],
} as const;
type NumberPath = keyof typeof NUMBERS;
const THRESHOLDS = ['percent.fiveHour', 'percent.sevenDay', 'modelLimit.thresholds'] as const;
type ThresholdPath = (typeof THRESHOLDS)[number];
type TextPath = NumberPath | ThresholdPath;
const MAX_THRESHOLDS = 5;
const HHMM = /^([01]\d|2[0-3]):[0-5]\d$/;

type Section = Exclude<keyof NotificationConfig, 'thresholds'>;

/** Avvisi con una «i»: perché servono, oltre a cosa fanno (il tooltip). */
const INFO: Partial<Record<Section, MessageKey>> = {
  switchPreview: 'info.notif.switchPreview',
  noReserve: 'info.notif.noReserve',
  percent: 'info.notif.percent',
  exhaustion: 'info.notif.exhaustion',
  modelLimit: 'info.notif.modelLimit',
  engineState: 'info.notif.engineState',
  login: 'info.notif.login',
  stale: 'info.notif.stale',
};
type Texts = Record<TextPath, string>;

function read(config: NotificationConfig, path: TextPath): unknown {
  const [section, field] = path.split('.') as [Section, string];
  return (config[section] as unknown as Record<string, unknown>)[field];
}

function textsOf(config: NotificationConfig): Texts {
  const out = {} as Texts;
  for (const p of Object.keys(NUMBERS) as NumberPath[]) out[p] = String(read(config, p));
  for (const p of THRESHOLDS) out[p] = (read(config, p) as number[]).join(', ');
  return out;
}

/** «80, 95» → soglie crescenti tra 0 (escluso) e 100, da 1 a 5; `null` se il testo non va. */
export function parseNotifyThresholds(text: string): number[] | null {
  const parts = text
    .split(/[,;\s]+/)
    .map((p) => p.trim())
    .filter(Boolean);
  if (parts.length === 0 || parts.length > MAX_THRESHOLDS) return null;
  const values = parts.map(Number);
  if (!values.every((n) => Number.isFinite(n) && n > 0 && n <= 100)) return null;
  for (let i = 1; i < values.length; i += 1) if (values[i]! <= values[i - 1]!) return null;
  return values;
}

type Errors = Partial<Record<TextPath | 'quiet' | 'summary', string>>;

/** Regole dalla bozza (toggle e orari in `config`, numeri e soglie nei testi) ed errori dei campi. */
function build(
  config: NotificationConfig,
  texts: Texts,
  t: (key: MessageKey, params?: Record<string, string | number>) => string,
): { next: NotificationConfig; errors: Errors } {
  const next = structuredClone(config);
  const errors: Errors = {};
  for (const p of Object.keys(NUMBERS) as NumberPath[]) {
    const [min, max] = NUMBERS[p];
    const v = Number(texts[p]);
    if (texts[p].trim() === '' || !Number.isInteger(v) || v < min || v > max)
      errors[p] = t('notif.err.number', { min, max });
    else {
      const [section, field] = p.split('.') as [Section, string];
      (next[section] as unknown as Record<string, unknown>)[field] = v;
    }
  }
  for (const p of THRESHOLDS) {
    const v = parseNotifyThresholds(texts[p]);
    if (!v) errors[p] = t('notif.err.thresholds');
    else {
      const [section, field] = p.split('.') as [Section, string];
      (next[section] as unknown as Record<string, unknown>)[field] = v;
    }
  }
  if (!HHMM.test(next.quietHours.start) || !HHMM.test(next.quietHours.end))
    errors.quiet = t('notif.err.time');
  if (!HHMM.test(next.dailySummary.time)) errors.summary = t('notif.err.time');
  return { next, errors };
}

/** Patch con le sole sezioni cambiate. */
function diff(saved: NotificationConfig, next: NotificationConfig): NotificationPatch {
  const patch: Record<string, unknown> = {};
  for (const key of Object.keys(next) as (keyof NotificationConfig)[]) {
    if (key === 'thresholds') continue;
    if (JSON.stringify(next[key]) !== JSON.stringify(saved[key])) patch[key] = next[key];
  }
  return patch as NotificationPatch;
}

/** Pagina Notifiche (F18): avvisi con interruttore e parametri, ambito, non disturbare, riepilogo. */
export function NotificationsPage({ onBack }: { onBack: () => void }) {
  const { t } = useI18n();
  const backend = useBackend();
  const load = useCallback(
    () =>
      backend.getNotificationSettings
        ? backend.getNotificationSettings()
        : Promise.reject(new Error('n/d')),
    [backend],
  );
  const { state, set, reload } = useLoad<NotificationConfig>(load, [load]);
  const back = useRef<HTMLButtonElement>(null);
  useEffect(() => back.current?.focus(), []);

  return (
    <div className="sd-page" role="region" aria-label={t('notif.title')}>
      <div className="sd-detail__head">
        <Tooltip text={t('notif.back.tip')}>
          <button ref={back} type="button" className="sd-btn sd-btn--small" onClick={onBack}>
            ← {t('notif.back')}
          </button>
        </Tooltip>
        <h2>{t('notif.title')}</h2>
      </div>
      <p className="sd-hint">{t('notif.hint')}</p>
      <LoadGate state={state} onRetry={() => void reload()}>
        {(config) => <NotificationsForm saved={config} onSaved={set} />}
      </LoadGate>
    </div>
  );
}

function NotificationsForm({
  saved,
  onSaved,
}: {
  saved: NotificationConfig;
  onSaved: (config: NotificationConfig) => void;
}) {
  const { t } = useI18n();
  const backend = useBackend();
  const [config, setConfig] = useState(() => structuredClone(saved));
  const [texts, setTexts] = useState(() => textsOf(saved));
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const { next, errors } = useMemo(() => build(config, texts, t), [config, texts, t]);
  const hasErrors = Object.keys(errors).length > 0;
  const patch = useMemo(() => diff(saved, next), [saved, next]);
  const dirty =
    Object.keys(patch).length > 0 || JSON.stringify(texts) !== JSON.stringify(textsOf(saved));

  function update<S extends Section>(section: S, changes: Partial<NotificationConfig[S]>) {
    setConfig((c) => ({ ...c, [section]: { ...c[section], ...changes } }));
  }
  const text = (path: TextPath) => ({
    value: texts[path],
    onChange: (value: string) => setTexts((x) => ({ ...x, [path]: value })),
    error: errors[path] ?? null,
    disabled: saving,
  });

  async function save() {
    if (!backend.setNotificationSettings || hasErrors) return;
    setSaving(true);
    setNotice(null);
    try {
      const result = await backend.setNotificationSettings(patch);
      onSaved(result);
      setConfig(structuredClone(result));
      setTexts(textsOf(result));
      setNotice({ tone: 'info', text: t('notif.saved') });
    } catch (error) {
      setNotice({
        tone: 'danger',
        text: t('common.actionFailed', { message: errorMessage(error) }),
      });
    } finally {
      setSaving(false);
    }
  }

  const toggle = (section: Section, label: MessageKey, tip: MessageKey, children?: ReactNode) => {
    const rule = config[section] as { enabled: boolean };
    const infoKey = INFO[section];
    return (
      <div className="sd-notif" data-testid={`notif-${section}`}>
        <CheckField
          label={t(label)}
          tip={t(tip)}
          {...(infoKey ? { info: t(infoKey) } : {})}
          checked={rule.enabled}
          disabled={saving}
          onChange={(enabled) => update(section, { enabled } as never)}
        />
        {rule.enabled && children && <div className="sd-notif__params">{children}</div>}
      </div>
    );
  };
  const scope = (section: Section) => (
    <SelectField
      label={t('notif.scope')}
      tip={t('notif.scope.tip')}
      value={(config[section] as { scope: NotifyScope }).scope}
      disabled={saving}
      onChange={(value) => update(section, { scope: value as NotifyScope } as never)}
      options={[
        { value: 'active', label: t('notif.scope.active') },
        { value: 'all', label: t('notif.scope.all') },
      ]}
    />
  );
  const thresholds = (path: ThresholdPath, label: MessageKey) => (
    <TextField label={t(label)} tip={t('notif.thresholds.tip')} {...text(path)} />
  );
  const number = (path: NumberPath, label: MessageKey, tip: MessageKey) => (
    <TextField
      label={t(label)}
      tip={t(tip)}
      type="number"
      min={NUMBERS[path][0]}
      max={NUMBERS[path][1]}
      {...text(path)}
    />
  );

  return (
    <form
      className="sd-form"
      onSubmit={(event) => {
        event.preventDefault();
        void save();
      }}
    >
      <Section title={t('notif.group.engine')} info={t('info.notif.engine')}>
        {toggle(
          'switchPreview',
          'notif.switchPreview',
          'notif.switchPreview.tip',
          <>
            {number(
              'switchPreview.pointsBefore',
              'notif.switchPreview.points',
              'notif.switchPreview.points.tip',
            )}
            {number(
              'switchPreview.minutesBefore',
              'notif.switchPreview.minutes',
              'notif.switchPreview.minutes.tip',
            )}
          </>,
        )}
        {toggle('switched', 'notif.switched', 'notif.switched.tip')}
        {toggle('returnedToPrimary', 'notif.returnedToPrimary', 'notif.returnedToPrimary.tip')}
        {toggle('noReserve', 'notif.noReserve', 'notif.noReserve.tip')}
      </Section>

      <Section title={t('notif.group.usage')} info={t('info.notif.usage')}>
        {toggle(
          'percent',
          'notif.percent',
          'notif.percent.tip',
          <>
            {scope('percent')}
            {thresholds('percent.fiveHour', 'notif.percent.fiveHour')}
            {thresholds('percent.sevenDay', 'notif.percent.sevenDay')}
          </>,
        )}
        {toggle('exhaustion', 'notif.exhaustion', 'notif.exhaustion.tip', scope('exhaustion'))}
        {toggle(
          'resetSoon',
          'notif.resetSoon',
          'notif.resetSoon.tip',
          <>
            {scope('resetSoon')}
            {number(
              'resetSoon.minutesBefore',
              'notif.resetSoon.minutes',
              'notif.resetSoon.minutes.tip',
            )}
          </>,
        )}
        {toggle('resetDone', 'notif.resetDone', 'notif.resetDone.tip', scope('resetDone'))}
        {toggle(
          'modelLimit',
          'notif.modelLimit',
          'notif.modelLimit.tip',
          <>
            {scope('modelLimit')}
            {thresholds('modelLimit.thresholds', 'notif.modelLimit.thresholds')}
          </>,
        )}
      </Section>

      <Section title={t('notif.group.state')} info={t('info.notif.state')}>
        {toggle('engineState', 'notif.engineState', 'notif.engineState.tip')}
        {toggle(
          'login',
          'notif.login',
          'notif.login.tip',
          <>
            {scope('login')}
            {number('login.daysBefore', 'notif.login.days', 'notif.login.days.tip')}
          </>,
        )}
        {toggle(
          'stale',
          'notif.stale',
          'notif.stale.tip',
          <>
            {scope('stale')}
            {number('stale.minutes', 'notif.stale.minutes', 'notif.stale.minutes.tip')}
          </>,
        )}
        {toggle('updates', 'notif.updates', 'notif.updates.tip')}
      </Section>

      <Section title={t('notif.quiet')} hint={t('notif.quiet.hint')} info={t('info.notif.quiet')}>
        {toggle(
          'quietHours',
          'notif.quiet.enabled',
          'notif.quiet.enabled.tip',
          <>
            <div className="sd-row">
              <TimeField
                label={t('notif.quiet.start')}
                tip={t('notif.quiet.start.tip')}
                value={config.quietHours.start}
                disabled={saving}
                onChange={(start) => update('quietHours', { start })}
              />
              <TimeField
                label={t('notif.quiet.end')}
                tip={t('notif.quiet.end.tip')}
                value={config.quietHours.end}
                disabled={saving}
                onChange={(end) => update('quietHours', { end })}
              />
            </div>
            {errors.quiet && <span className="sd-field-error">{errors.quiet}</span>}
            <CheckField
              label={t('notif.quiet.critical')}
              tip={t('notif.quiet.critical.tip')}
              info={t('info.notif.quietCritical')}
              checked={config.quietHours.allowCritical}
              disabled={saving}
              onChange={(allowCritical) => update('quietHours', { allowCritical })}
            />
          </>,
        )}
      </Section>

      <Section title={t('notif.summary')} info={t('info.notif.summary')}>
        {toggle(
          'dailySummary',
          'notif.summary.enabled',
          'notif.summary.enabled.tip',
          <>
            <TimeField
              label={t('notif.summary.time')}
              tip={t('notif.summary.time.tip')}
              value={config.dailySummary.time}
              disabled={saving}
              onChange={(time) => update('dailySummary', { time })}
            />
            {errors.summary && <span className="sd-field-error">{errors.summary}</span>}
          </>,
        )}
      </Section>

      <NoticeView notice={notice} />
      <div className="sd-row sd-row--end">
        <Tooltip text={t('notif.revert.tip')}>
          <button
            type="button"
            className="sd-btn"
            disabled={saving || !dirty}
            onClick={() => {
              setConfig(structuredClone(saved));
              setTexts(textsOf(saved));
              setNotice(null);
            }}
          >
            {t('notif.revert')}
          </button>
        </Tooltip>
        <Tooltip text={t('notif.save.tip')}>
          <button
            type="submit"
            className="sd-btn sd-btn--primary"
            disabled={saving || hasErrors || !dirty}
          >
            {saving ? t('common.working') : t('common.save')}
          </button>
        </Tooltip>
      </div>
    </form>
  );
}

/** Campo orario `HH:MM` (input `time` del browser). */
function TimeField(props: {
  label: string;
  tip: string;
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  const id = useId();
  return (
    <div className="sd-form__field">
      <label htmlFor={id}>{props.label}</label>
      <Tooltip text={props.tip} block>
        <input
          id={id}
          className="sd-input"
          type="time"
          value={props.value}
          disabled={props.disabled}
          onChange={(event) => props.onChange(event.target.value)}
        />
      </Tooltip>
    </div>
  );
}
