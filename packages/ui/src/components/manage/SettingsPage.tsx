import type { ConfigSetting, Theme } from '@swapdeck/types';
import { useCallback, useState } from 'react';
import { actions } from '../../backend/actions.js';
import { useBackend } from '../../backend/context.js';
import { useActionRunner, type Notice } from '../../backend/useActionRunner.js';
import { errorMessage } from '../../backend/useAccounts.js';
import type { AppPreferences, HostSettings } from '../../backend/types.js';
import { resolveLocale, useI18n, type LocalePreference } from '../../i18n/index.js';
import {
  CONFIG_KEYS,
  configValueText,
  validateConfigValue,
  type ConfigKind,
} from '../../lib/cswapConfig.js';
import { useLoad } from '../../lib/useLoad.js';
import type { ThemeMode } from '../../theme/index.js';
import { LanguageSelect, ThemeSelect } from '../PreferenceControls.js';
import {
  CheckField,
  LoadGate,
  NoticeView,
  Section,
  SelectField,
  TextField,
  Unavailable,
} from './common.js';
import { NotificationsPage } from './NotificationsPage.js';
import { Tooltip } from '../Tooltip.js';

/** «80, 95» in soglie crescenti da 1 a 100 (da 1 a 4 valori); `null` se il testo non è valido. */
export function parseThresholds(text: string): number[] | null {
  const parts = text
    .split(/[,;\s]+/)
    .map((p) => p.trim())
    .filter(Boolean);
  if (parts.length === 0 || parts.length > 4) return null;
  const values = parts.map(Number);
  if (!values.every((n) => Number.isFinite(n) && n >= 1 && n <= 100)) return null;
  for (let i = 1; i < values.length; i += 1) if (values[i]! <= values[i - 1]!) return null;
  return values;
}

const THEME_TO_CONFIG: Record<ThemeMode, Theme> = { auto: 'system', light: 'light', dark: 'dark' };

/** Impostazioni dell'app (intervallo, notifiche, lingua, tema, avvio al login) e configurazione di cswap. */
export function SettingsPage() {
  const { t } = useI18n();
  const backend = useBackend();
  const [page, setPage] = useState<'main' | 'notifications'>('main');
  const notifications =
    typeof backend.getNotificationSettings === 'function' &&
    typeof backend.setNotificationSettings === 'function';
  if (page === 'notifications' && notifications)
    return <NotificationsPage onBack={() => setPage('main')} />;
  return (
    <div className="sd-page">
      <h2>{t('settings.title')}</h2>
      {notifications && (
        <Section title={t('notif.title')} hint={t('notif.hint')}>
          <div className="sd-row">
            <Tooltip text={t('notif.open.tip')}>
              <button type="button" className="sd-btn" onClick={() => setPage('notifications')}>
                {t('notif.open')}
              </button>
            </Tooltip>
          </div>
        </Section>
      )}
      <PreferencesSection />
      <AutostartSection />
      <CswapConfigSection />
    </div>
  );
}

function PreferencesSection() {
  const { t } = useI18n();
  const backend = useBackend();
  const supported = typeof backend.getPreferences === 'function';
  const load = useCallback(
    () => (backend.getPreferences ? backend.getPreferences() : Promise.reject(new Error('n/d'))),
    [backend],
  );
  const { state, set, reload } = useLoad<AppPreferences>(load, [load]);
  const [notice, setNotice] = useState<Notice | null>(null);

  async function save(change: Partial<AppPreferences>, text: string): Promise<boolean> {
    if (!backend.setPreferences) return false;
    setNotice(null);
    try {
      set(await backend.setPreferences(change));
      setNotice({ tone: 'info', text });
      return true;
    } catch (error) {
      setNotice({
        tone: 'danger',
        text: t('common.actionFailed', { message: errorMessage(error) }),
      });
      return false;
    }
  }

  return (
    <Section title={t('settings.app')}>
      <div className="sd-row">
        <LanguageSelect
          onChange={(value: LocalePreference) =>
            void save({ language: resolveLocale(value) }, t('settings.saved'))
          }
        />
        <ThemeSelect
          onChange={(mode) => void save({ theme: THEME_TO_CONFIG[mode] }, t('settings.saved'))}
        />
      </div>
      {supported && (
        <LoadGate state={state} onRetry={() => void reload()}>
          {(prefs) => (
            <PollForm
              prefs={prefs}
              onSave={save}
              withThresholds={typeof backend.getNotificationSettings !== 'function'}
            />
          )}
        </LoadGate>
      )}
      <NoticeView notice={notice} />
    </Section>
  );
}

function PollForm({
  prefs,
  onSave,
  withThresholds,
}: {
  prefs: AppPreferences;
  onSave: (change: Partial<AppPreferences>, text: string) => Promise<boolean>;
  /** Soglie delle notifiche della versione 1: solo se l'host non ha la pagina Notifiche. */
  withThresholds: boolean;
}) {
  const { t } = useI18n();
  const [interval, setInterval] = useState(String(prefs.pollIntervalSeconds));
  const [thresholds, setThresholds] = useState(prefs.notificationThresholds.join(', '));
  const [saving, setSaving] = useState(false);

  const intervalNumber = Number(interval);
  const intervalError =
    interval.trim() === '' ||
    !Number.isInteger(intervalNumber) ||
    intervalNumber < 60 ||
    intervalNumber > 3600
      ? t('settings.interval.invalid')
      : null;
  const parsed = parseThresholds(thresholds);
  const thresholdsError =
    withThresholds && parsed === null ? t('settings.thresholds.invalid') : null;
  const dirty =
    intervalNumber !== prefs.pollIntervalSeconds ||
    (withThresholds && JSON.stringify(parsed) !== JSON.stringify(prefs.notificationThresholds));

  return (
    <form
      className="sd-form"
      onSubmit={(event) => {
        event.preventDefault();
        if (intervalError || thresholdsError) return;
        setSaving(true);
        void onSave(
          withThresholds && parsed
            ? { pollIntervalSeconds: intervalNumber, notificationThresholds: parsed }
            : { pollIntervalSeconds: intervalNumber },
          t('settings.saved'),
        ).finally(() => setSaving(false));
      }}
    >
      <TextField
        label={t('settings.interval')}
        tip={t('settings.interval.tip')}
        type="number"
        min={60}
        max={3600}
        hint={t('settings.interval.hint')}
        value={interval}
        onChange={setInterval}
        error={intervalError}
        disabled={saving}
      />
      {withThresholds && (
        <TextField
          label={t('settings.thresholds')}
          tip={t('settings.thresholds.tip')}
          hint={t('settings.thresholds.hint')}
          value={thresholds}
          onChange={setThresholds}
          error={thresholdsError}
          disabled={saving}
        />
      )}
      <div className="sd-row">
        <Tooltip text={t('settings.save.tip')}>
          <button
            type="submit"
            className="sd-btn sd-btn--primary"
            disabled={saving || !dirty || intervalError !== null || thresholdsError !== null}
          >
            {t('common.save')}
          </button>
        </Tooltip>
      </div>
    </form>
  );
}

/** Avvio al login: solo dove l'host lo offre (app desktop). */
function AutostartSection() {
  const { t } = useI18n();
  const backend = useBackend();
  const supported = typeof backend.getAppSettings === 'function';
  const load = useCallback(
    () => (backend.getAppSettings ? backend.getAppSettings() : Promise.reject(new Error('n/d'))),
    [backend],
  );
  const { state, set } = useLoad<HostSettings>(load, [load]);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  if (!supported || state.status === 'loading') return null;
  if (state.status === 'error') return null;
  const settings = state.data;
  return (
    <Section title={t('settings.autostart.title')}>
      <CheckField
        label={t('settings.autostart')}
        tip={t('settings.autostart.tip')}
        checked={settings.autostart}
        disabled={busy}
        {...(settings.autostartManaged ? {} : { hint: t('settings.autostart.dev') })}
        onChange={(enabled) => {
          setBusy(true);
          setNotice(null);
          backend
            .setAutostart?.(enabled)
            .then(set)
            .catch((error: unknown) =>
              setNotice({
                tone: 'danger',
                text: t('common.actionFailed', { message: errorMessage(error) }),
              }),
            )
            .finally(() => setBusy(false));
        }}
      />
      <span className="sd-hint">{t('settings.version', { version: settings.version })}</span>
      <NoticeView notice={notice} />
    </Section>
  );
}

function CswapConfigSection() {
  const { t } = useI18n();
  const backend = useBackend();
  const load = useCallback(() => backend.getConfig(), [backend]);
  const { state, reload } = useLoad(load, [load]);
  const { run, busy, notice, dialog } = useActionRunner();

  async function after(promise: Promise<{ ok: boolean }>) {
    if ((await promise).ok) await reload();
  }

  return (
    <Section title={t('config.title')} hint={t('config.hint')}>
      <LoadGate state={state} onRetry={() => void reload()}>
        {(config) => (
          <>
            <span className="sd-hint sd-path" title={config.path}>
              {t('config.path', { path: config.path })}
            </span>
            <ul className="sd-list" aria-label={t('config.title')}>
              {config.settings.map((setting) => (
                <ConfigRow
                  key={`${setting.key}:${JSON.stringify(setting.value)}:${setting.isSet}`}
                  setting={setting}
                  busy={busy}
                  onSet={(value) =>
                    after(
                      run(actions.configSet(setting.key, value), {
                        success: t('config.saved', { key: setting.key }),
                      }),
                    )
                  }
                  onUnset={() =>
                    after(
                      run(actions.configUnset(setting.key), {
                        success: t('config.reset', { key: setting.key }),
                      }),
                    )
                  }
                />
              ))}
            </ul>
          </>
        )}
      </LoadGate>
      <NoticeView notice={notice} />
      {dialog}
    </Section>
  );
}

function ConfigRow({
  setting,
  busy,
  onSet,
  onUnset,
}: {
  setting: ConfigSetting;
  busy: boolean;
  onSet: (value: string) => void;
  onUnset: () => void;
}) {
  const { t } = useI18n();
  const info = CONFIG_KEYS[setting.key];
  const initial = configValueText(setting.value);
  const [text, setText] = useState(initial);
  const error = validateConfigValue(setting.key, text);
  const label = info ? t(info.label) : setting.key;
  const kind: ConfigKind = info?.kind ?? { type: 'string' };
  const range =
    kind.type === 'number' ? t('config.range', { min: kind.min, max: kind.max }) : undefined;
  const choiceLabel = (value: string) => value;

  return (
    <li className="sd-card sd-card--form">
      <div className="sd-card__title">
        <span className="sd-card__name">{label}</span>
        <span className="sd-card__email">
          {setting.key} · {t(setting.isSet ? 'config.isSet' : 'config.isDefault')}
        </span>
      </div>
      {kind.type === 'choice' && (
        <SelectField
          label={label}
          tip={t('config.field.tip')}
          value={text}
          onChange={setText}
          disabled={busy}
          options={kind.choices.map((c) => ({ value: c, label: choiceLabel(c) }))}
        />
      )}
      {kind.type === 'bool' && (
        <CheckField
          label={label}
          tip={t('config.field.tip')}
          checked={text === 'true'}
          onChange={(checked) => setText(checked ? 'true' : 'false')}
          disabled={busy}
        />
      )}
      {(kind.type === 'number' || kind.type === 'string') && (
        <TextField
          label={label}
          tip={t('config.field.tip')}
          type={kind.type === 'number' ? 'number' : 'text'}
          {...(kind.type === 'number' ? { step: 'any' as const } : {})}
          {...(range ? { hint: range } : {})}
          value={text}
          onChange={setText}
          error={error ? t(error) : null}
          disabled={busy}
        />
      )}
      <div className="sd-row">
        <Tooltip text={t('config.save.tip')}>
          <button
            type="button"
            className="sd-btn sd-btn--small sd-btn--primary"
            disabled={busy || error !== null || text === initial}
            aria-label={t('config.saveLabel', { key: setting.key })}
            onClick={() => onSet(text)}
          >
            {t('common.save')}
          </button>
        </Tooltip>
        {setting.isSet && (
          <Tooltip text={t('config.reset.tip')}>
            <button
              type="button"
              className="sd-btn sd-btn--small"
              disabled={busy}
              aria-label={t('config.resetLabel', { key: setting.key })}
              onClick={onUnset}
            >
              {t('config.resetAction')}
            </button>
          </Tooltip>
        )}
      </div>
    </li>
  );
}
