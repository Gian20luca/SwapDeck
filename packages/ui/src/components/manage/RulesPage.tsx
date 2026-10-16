import type { Account, EngineRules, ReserveCriterion } from '@swapdeck/types';
import { useCallback, useMemo, useState } from 'react';
import { useBackend } from '../../backend/context.js';
import type { Notice } from '../../backend/useActionRunner.js';
import { errorMessage, useAccounts } from '../../backend/useAccounts.js';
import type { EngineState } from '../../backend/types.js';
import { useI18n, type MessageKey } from '../../i18n/index.js';
import { accountName } from '../../lib/account.js';
import {
  applyGuardPreset,
  fromDraft,
  toDraft,
  validateRules,
  type RulesDraft,
} from '../../lib/engineRules.js';
import { useLoad } from '../../lib/useLoad.js';
import {
  CheckField,
  LoadGate,
  NoticeView,
  Section,
  SelectField,
  TextField,
  Unavailable,
} from './common.js';

const CRITERIA: ReserveCriterion[] = ['lowest-weekly', 'best', 'fixed-order'];

const SUSPEND_KEYS = {
  disabled: 'rules.suspend.disabled',
  invalidRules: 'rules.suspend.invalidRules',
  conflict: 'rules.suspend.conflict',
  noActiveAccount: 'rules.suspend.noActiveAccount',
  noViableTarget: 'rules.suspend.noViableTarget',
} as const satisfies Record<string, MessageKey>;

/** Editor delle regole del motore (`EngineRules`): stato, conflitti, account, soglie e preset. */
export function RulesPage() {
  const backend = useBackend();
  const supported =
    typeof backend.getEngineState === 'function' && typeof backend.setEngineRules === 'function';
  const load = useCallback(
    () => (backend.getEngineState ? backend.getEngineState() : Promise.reject(new Error('n/d'))),
    [backend],
  );
  const { state, reload, set } = useLoad<EngineState>(load, [load]);
  const { state: accountsState } = useAccounts();
  const accounts = accountsState.status === 'ready' ? accountsState.data.accounts : [];
  if (!supported) return <Unavailable />;
  return (
    <LoadGate state={state} onRetry={() => void reload()}>
      {(data) => <RulesEditor data={data} accounts={accounts} onSaved={set} />}
    </LoadGate>
  );
}

interface EditorProps {
  data: EngineState;
  accounts: Account[];
  onSaved: (state: EngineState) => void;
}

function RulesEditor({ data, accounts, onSaved }: EditorProps) {
  const { t } = useI18n();
  const backend = useBackend();
  const [draft, setDraft] = useState<RulesDraft>(() => toDraft(data.rules));
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [reserveToAdd, setReserveToAdd] = useState('');

  const rules = useMemo(() => fromDraft(draft, data.rules), [draft, data.rules]);
  const errors = useMemo(() => validateRules(rules), [rules]);
  const hasErrors = Object.keys(errors).length > 0;
  const dirty = JSON.stringify(rules) !== JSON.stringify(data.rules);

  const nameOf = (n: number) => {
    const found = accounts.find((a) => a.number === n);
    return found ? `${accountName(found, t)} (#${n})` : `#${n}`;
  };
  const patch = (changes: Partial<RulesDraft>) => setDraft((d) => ({ ...d, ...changes }));

  /** Salva; con `keepDraft` la bozza resta com'è, salvo l'elenco dei conflitti ignorati. */
  async function persist(next: EngineRules, doneText: string, keepDraft = false) {
    if (!backend.setEngineRules) return;
    setSaving(true);
    setNotice(null);
    try {
      const saved = await backend.setEngineRules(next);
      onSaved(saved);
      if (keepDraft) patch({ ignoredConflicts: [...saved.rules.ignoredConflicts] });
      else setDraft(toDraft(saved.rules));
      setNotice({ tone: 'info', text: doneText });
    } catch (error) {
      setNotice({
        tone: 'danger',
        text: t('common.actionFailed', { message: errorMessage(error) }),
      });
    } finally {
      setSaving(false);
    }
  }

  /** Ignora (o smette di ignorare) un conflitto: salva subito solo quell'elenco, senza toccare il resto della bozza. */
  async function setIgnored(id: string, ignore: boolean) {
    const current = data.rules.ignoredConflicts;
    const next = ignore
      ? current.some((c) => c.toLowerCase() === id.toLowerCase())
        ? current
        : [...current, id]
      : current.filter((c) => c.toLowerCase() !== id.toLowerCase());
    await persist(
      { ...data.rules, ignoredConflicts: next },
      t(ignore ? 'rules.conflict.ignored' : 'rules.conflict.unignored'),
      true,
    );
  }

  const others = accounts.filter((a) => a.number !== draft.primary);
  const reserveChoices = others.filter(
    (a) => !draft.reserves.includes(a.number) && !draft.excluded.includes(a.number),
  );
  const addReserve = reserveToAdd || (reserveChoices[0] ? String(reserveChoices[0].number) : '');

  function move(index: number, by: -1 | 1) {
    const list = [...draft.reserves];
    const target = index + by;
    if (target < 0 || target >= list.length) return;
    [list[index], list[target]] = [list[target]!, list[index]!];
    patch({ reserves: list });
  }

  function toggleExcluded(n: number, excluded: boolean) {
    patch({
      excluded: excluded ? [...draft.excluded, n] : draft.excluded.filter((x) => x !== n),
      reserves: excluded ? draft.reserves.filter((x) => x !== n) : draft.reserves,
    });
  }

  const view = data.view;
  const suspended = view?.suspended ?? null;
  const ignored = data.rules.ignoredConflicts.map((c) => c.toLowerCase());
  const conflictById = new Map(data.conflicts.map((c) => [c.id.toLowerCase(), c]));

  return (
    <div className="sd-page">
      <h2>{t('rules.title')}</h2>
      <p className="sd-hint">{t('rules.hint')}</p>

      {view && (
        <div
          className="sd-notice"
          data-tone={suspended ? 'danger' : undefined}
          role={suspended ? 'alert' : 'status'}
          aria-label={t('rules.status')}
        >
          {suspended ? (
            <>
              <strong>{t('rules.suspended')}</strong> {t(SUSPEND_KEYS[suspended])}
              {suspended === 'invalidRules' && view.detail.length > 0 && (
                <ul>
                  {view.detail.map((d) => (
                    <li key={d}>{d}</li>
                  ))}
                </ul>
              )}
              {suspended === 'conflict' && view.detail.length > 0 && (
                <ul>
                  {view.detail.map((id) => (
                    <li key={id}>{conflictById.get(id.toLowerCase())?.label ?? id}</li>
                  ))}
                </ul>
              )}
            </>
          ) : (
            <>
              {t(view.enabled ? 'rules.active' : 'rules.off')}
              {view.enabled && view.dryRun && ` · ${t('rules.dryRunOn')}`}
              {view.enabled && !view.holder && ` · ${t('rules.observer')}`}
            </>
          )}
        </div>
      )}

      {data.conflicts.length > 0 && (
        <Section title={t('rules.conflicts')} hint={t('rules.conflicts.hint')}>
          <ul className="sd-list" aria-label={t('rules.conflicts')}>
            {data.conflicts.map((c) => {
              const isIgnored = ignored.includes(c.id.toLowerCase());
              return (
                <li key={c.id} className="sd-card sd-card--row" data-ignored={isIgnored}>
                  <div className="sd-card__title">
                    <span className="sd-card__name">{c.label}</span>
                    <span className="sd-card__email">
                      {t(c.certain ? 'rules.conflict.blocks' : 'rules.conflict.warns')}
                      {isIgnored ? ` · ${t('rules.conflict.ignoredTag')}` : ''}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="sd-btn sd-btn--small"
                    disabled={saving}
                    aria-label={t(
                      isIgnored ? 'rules.conflict.unignoreLabel' : 'rules.conflict.ignoreLabel',
                      {
                        label: c.label,
                      },
                    )}
                    onClick={() => void setIgnored(c.id, !isIgnored)}
                  >
                    {t(isIgnored ? 'rules.conflict.unignore' : 'rules.conflict.ignore')}
                  </button>
                </li>
              );
            })}
          </ul>
        </Section>
      )}

      <form
        className="sd-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (!hasErrors && dirty) void persist(rules, t('rules.saved'));
        }}
      >
        <Section title={t('rules.engine')}>
          <CheckField
            label={t('rules.enabled')}
            checked={draft.enabled}
            onChange={(enabled) => patch({ enabled })}
            disabled={saving}
          />
          <CheckField
            label={t('rules.dryRun')}
            hint={t('rules.dryRun.hint')}
            checked={draft.dryRun}
            onChange={(dryRun) => patch({ dryRun })}
            disabled={saving}
          />
        </Section>

        <Section title={t('rules.accounts')}>
          <div className="sd-form__field">
            <SelectField
              label={t('rules.primary')}
              value={draft.primary === null ? '' : String(draft.primary)}
              onChange={(value) =>
                patch({
                  primary: value === '' ? null : Number(value),
                  ...(value === ''
                    ? {}
                    : {
                        reserves: draft.reserves.filter((n) => n !== Number(value)),
                        excluded: draft.excluded.filter((n) => n !== Number(value)),
                      }),
                })
              }
              disabled={saving}
              options={[
                { value: '', label: t('rules.primary.none') },
                ...(draft.primary !== null && !accounts.some((a) => a.number === draft.primary)
                  ? [{ value: String(draft.primary), label: nameOf(draft.primary) }]
                  : []),
                ...accounts.map((a) => ({ value: String(a.number), label: nameOf(a.number) })),
              ]}
            />
            {errors.primary && <span className="sd-field-error">{t(errors.primary)}</span>}
          </div>

          <div className="sd-form__field">
            <span id="rules-reserves-label" className="sd-label">
              {t('rules.reserves')}
            </span>
            <span className="sd-hint">{t('rules.reserves.hint')}</span>
            {draft.reserves.length > 0 && (
              <ol className="sd-order" aria-labelledby="rules-reserves-label">
                {draft.reserves.map((n, index) => (
                  <li key={n} className="sd-order__item">
                    <span>{nameOf(n)}</span>
                    <span className="sd-row">
                      <button
                        type="button"
                        className="sd-btn sd-btn--small"
                        disabled={saving || index === 0}
                        aria-label={t('rules.reserve.up', { name: nameOf(n) })}
                        onClick={() => move(index, -1)}
                      >
                        ↑
                      </button>
                      <button
                        type="button"
                        className="sd-btn sd-btn--small"
                        disabled={saving || index === draft.reserves.length - 1}
                        aria-label={t('rules.reserve.down', { name: nameOf(n) })}
                        onClick={() => move(index, 1)}
                      >
                        ↓
                      </button>
                      <button
                        type="button"
                        className="sd-btn sd-btn--small"
                        disabled={saving}
                        aria-label={t('rules.reserve.remove', { name: nameOf(n) })}
                        onClick={() => patch({ reserves: draft.reserves.filter((x) => x !== n) })}
                      >
                        ×
                      </button>
                    </span>
                  </li>
                ))}
              </ol>
            )}
            {draft.reserves.length === 0 && (
              <span className="sd-hint">{t('rules.reserves.all')}</span>
            )}
            {reserveChoices.length > 0 && (
              <div className="sd-row">
                <select
                  className="sd-input"
                  aria-label={t('rules.reserve.addSelect')}
                  value={addReserve}
                  disabled={saving}
                  onChange={(event) => setReserveToAdd(event.target.value)}
                >
                  {reserveChoices.map((a) => (
                    <option key={a.number} value={a.number}>
                      {nameOf(a.number)}
                    </option>
                  ))}
                </select>
                <button
                  type="button"
                  className="sd-btn"
                  disabled={saving || addReserve === ''}
                  onClick={() => {
                    patch({ reserves: [...draft.reserves, Number(addReserve)] });
                    setReserveToAdd('');
                  }}
                >
                  {t('rules.reserve.add')}
                </button>
              </div>
            )}
          </div>

          <fieldset className="sd-fieldset">
            <legend>{t('rules.excluded')}</legend>
            <span className="sd-hint">{t('rules.excluded.hint')}</span>
            {others.length === 0 && <span className="sd-hint">{t('rules.excluded.none')}</span>}
            {others.map((a) => (
              <CheckField
                key={a.number}
                label={nameOf(a.number)}
                checked={draft.excluded.includes(a.number)}
                onChange={(checked) => toggleExcluded(a.number, checked)}
                disabled={saving}
              />
            ))}
          </fieldset>
        </Section>

        <Section title={t('rules.thresholds')}>
          <TextField
            label={t('rules.fiveHour')}
            type="number"
            min={1}
            max={100}
            step="any"
            value={draft.fiveHourThreshold}
            onChange={(value) => patch({ fiveHourThreshold: value })}
            error={errors.fiveHour ? t(errors.fiveHour) : null}
            disabled={saving}
          />
          <TextField
            label={t('rules.sevenDay')}
            type="number"
            min={1}
            max={100}
            hint={t('rules.sevenDay.hint')}
            value={draft.sevenDayThreshold}
            onChange={(value) => patch({ sevenDayThreshold: value })}
            error={errors.sevenDay ? t(errors.sevenDay) : null}
            disabled={saving}
          />
          <SelectField
            label={t('rules.criterion')}
            value={draft.reserveCriterion}
            onChange={(value) => patch({ reserveCriterion: value as ReserveCriterion })}
            disabled={saving}
            options={CRITERIA.map((c) => ({ value: c, label: t(`rules.criterion.${c}`) }))}
          />
          <CheckField
            label={t('rules.return')}
            checked={draft.returnToPrimary}
            onChange={(returnToPrimary) => patch({ returnToPrimary })}
            disabled={saving}
          />
          <TextField
            label={t('rules.hysteresis')}
            type="number"
            min={0}
            max={50}
            hint={t('rules.hysteresis.hint')}
            value={draft.returnHysteresis}
            onChange={(value) => patch({ returnHysteresis: value })}
            error={errors.hysteresis ? t(errors.hysteresis) : null}
            disabled={saving}
          />
          <TextField
            label={t('rules.gap')}
            type="number"
            min={0}
            hint={t('rules.gap.hint')}
            value={draft.minSwitchGapSecs}
            onChange={(value) => patch({ minSwitchGapSecs: value })}
            error={errors.gap ? t(errors.gap) : null}
            disabled={saving}
          />
        </Section>

        <NoticeView notice={notice} />
        <div className="sd-row sd-row--end">
          <button
            type="button"
            className="sd-btn"
            disabled={saving}
            title={t('rules.preset.hint')}
            onClick={() => {
              setDraft(toDraft(applyGuardPreset(rules)));
              setNotice({ tone: 'info', text: t('rules.preset.applied') });
            }}
          >
            {t('rules.preset')}
          </button>
          <button
            type="button"
            className="sd-btn"
            disabled={saving || !dirty}
            onClick={() => {
              setDraft(toDraft(data.rules));
              setNotice(null);
            }}
          >
            {t('rules.revert')}
          </button>
          <button
            type="submit"
            className="sd-btn sd-btn--primary"
            disabled={saving || hasErrors || !dirty}
          >
            {saving ? t('common.working') : t('common.save')}
          </button>
        </div>
      </form>
    </div>
  );
}
