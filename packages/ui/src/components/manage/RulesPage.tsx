import type {
  Account,
  EngineProfiles,
  EngineRules,
  ProfileCommand,
  ReserveCriterion,
  ScoreWeights,
} from '@swapdeck/types';
import { useCallback, useEffect, useId, useMemo, useRef, useState } from 'react';
import { useBackend } from '../../backend/context.js';
import type { Notice } from '../../backend/useActionRunner.js';
import { errorMessage, useAccounts } from '../../backend/useAccounts.js';
import type { EngineState } from '../../backend/types.js';
import { useI18n, type MessageKey } from '../../i18n/index.js';
import {
  CRITERIA,
  configuredOrder,
  flowRows,
  fromDraft,
  materialize,
  missingAccounts,
  moveRow,
  setIncluded,
  toDraft,
  validateRules,
  type RulesDraft,
  type RulesErrors,
} from '../../lib/engineRules.js';
import { shortEmail } from '../../lib/format.js';
import { useLoad } from '../../lib/useLoad.js';
import { Tooltip } from '../Tooltip.js';
import { FlowList } from './FlowList.js';
import { MapNameForm, MapSelector, profileLabel } from './MapSelector.js';
import {
  CheckField,
  LoadGate,
  NoticeView,
  Section,
  SelectField,
  TextField,
  Unavailable,
} from './common.js';

const SUSPEND_KEYS = {
  disabled: 'rules.suspend.disabled',
  invalidRules: 'rules.suspend.invalidRules',
  conflict: 'rules.suspend.conflict',
  noActiveAccount: 'rules.suspend.noActiveAccount',
  noViableTarget: 'rules.suspend.noViableTarget',
} as const satisfies Record<string, MessageKey>;

const WEIGHTS: readonly (keyof ScoreWeights)[] = ['fiveHour', 'sevenDay', 'reset', 'pace'];

interface RulesData {
  engine: EngineState;
  /** `null` se l'host non gestisce le mappe. */
  profiles: EngineProfiles | null;
}

/**
 * Scheda Regole: stato del motore, mappa attiva con il riquadro delle sue regole (lista delle
 * utenze, criterio, soglie; in sola lettura per un preset, con «Clona come mappa personale»),
 * interruttori comuni a tutte le mappe e conflitti.
 */
export function RulesPage() {
  const backend = useBackend();
  const { t } = useI18n();
  const supported =
    typeof backend.getEngineState === 'function' && typeof backend.setEngineRules === 'function';
  const load = useCallback(async (): Promise<RulesData> => {
    if (!backend.getEngineState) throw new Error('n/d');
    const [engine, profiles] = await Promise.all([
      backend.getEngineState(),
      backend.getEngineProfiles ? backend.getEngineProfiles() : Promise.resolve(null),
    ]);
    return { engine, profiles };
  }, [backend]);
  const { state, reload, set } = useLoad<RulesData>(load, [load]);
  const { state: accountsState } = useAccounts();
  /** Esito da mostrare nell'editor della nuova mappa (che riparte da capo). */
  const [carried, setCarried] = useState<Notice | null>(null);

  // Un cambio di stato (giro del motore, mappa scelta dal menu dell'icona) rilegge regole e candidato.
  useEffect(() => backend.onStateChanged(() => void reload()), [backend, reload]);

  if (!supported) return <Unavailable />;
  if (accountsState.status === 'loading')
    return (
      <div className="sd-state" role="status">
        {t('common.loading')}
      </div>
    );
  const accounts = accountsState.status === 'ready' ? accountsState.data.accounts : [];
  return (
    <LoadGate state={state} onRetry={() => void reload()}>
      {(data) => (
        <RulesEditor
          key={data.profiles?.activeId ?? 'rules'}
          data={data}
          accounts={accounts}
          accountsKnown={accountsState.status === 'ready'}
          onSaved={(next, notice) => {
            if (notice) setCarried(notice);
            set(next);
          }}
          initialNotice={carried}
        />
      )}
    </LoadGate>
  );
}

interface EditorProps {
  data: RulesData;
  accounts: Account[];
  /** La lista degli account è stata letta (senza, niente avviso sulle utenze mancanti). */
  accountsKnown: boolean;
  /** L'esito facoltativo passa all'editor della nuova mappa. */
  onSaved: (data: RulesData, notice?: Notice) => void;
  initialNotice: Notice | null;
}

function initialDraft(rules: EngineRules, accounts: readonly Account[]): RulesDraft {
  const draft = toDraft(rules);
  return draft.order.length === 0 && accounts.length > 0 ? materialize(draft, accounts) : draft;
}

function RulesEditor({ data, accounts, accountsKnown, onSaved, initialNotice }: EditorProps) {
  const { t } = useI18n();
  const backend = useBackend();
  const rulesNow = data.engine.rules;
  const [base, setBase] = useState<RulesDraft>(() => initialDraft(rulesNow, accounts));
  const [draft, setDraft] = useState<RulesDraft>(base);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(initialNotice);
  const [cloning, setCloning] = useState(false);
  const boxId = useId();
  const activeProfile = data.profiles?.profiles.find((p) => p.id === data.profiles?.activeId);
  /** Preset attivo: lista, criterio e soglie in sola lettura. */
  const locked = Boolean(activeProfile?.preset);

  // Confronto sulla lista resa esplicita: uno spostamento annullato non lascia modifiche.
  const dirty =
    JSON.stringify(materialize(draft, accounts)) !== JSON.stringify(materialize(base, accounts));
  // Regole cambiate fuori (altra finestra, CLI): senza modifiche in corso la bozza si riallinea.
  const seen = useRef(JSON.stringify(rulesNow));
  useEffect(() => {
    const json = JSON.stringify(rulesNow);
    if (json === seen.current) return;
    seen.current = json;
    if (!dirty) {
      const next = initialDraft(rulesNow, accounts);
      setBase(next);
      setDraft(next);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rulesNow]);

  /** Preset attivo senza lista: il motore non parte finché non lo si clona e si sceglie la lista. */
  const noList = locked && configuredOrder(rulesNow).length === 0;
  // Con un preset attivo si salvano solo le parti comuni a tutte le mappe: il resto è del preset.
  const rules = useMemo(
    () =>
      locked
        ? {
            ...rulesNow,
            enabled: draft.enabled,
            dryRun: draft.dryRun,
            ignoredConflicts: [...draft.ignoredConflicts],
          }
        : fromDraft(draft, rulesNow),
    [locked, draft, rulesNow],
  );
  const errors = useMemo<RulesErrors>(
    () =>
      !locked
        ? validateRules(rules)
        : noList && (rules.enabled || rules.dryRun)
          ? { order: 'maps.noList' }
          : {},
    [locked, noList, rules],
  );
  const hasErrors = Object.keys(errors).length > 0;
  const rows = useMemo(() => flowRows(draft, accounts), [draft, accounts]);
  const missing = accountsKnown ? missingAccounts(draft.order, accounts) : [];
  const busy = saving;

  const nameOf = (n: number) => {
    const found = accounts.find((a) => a.number === n);
    if (!found) return `#${n}`;
    return found.alias ? found.alias : shortEmail(found.email, 22);
  };
  const patch = (changes: Partial<RulesDraft>) => setDraft((d) => ({ ...d, ...changes }));

  function adopt(engine: EngineState, profiles: EngineProfiles | null) {
    seen.current = JSON.stringify(engine.rules);
    onSaved({ engine, profiles });
    const next = initialDraft(engine.rules, accounts);
    setBase(next);
    setDraft(next);
  }

  /** Salva; con `keepDraft` la bozza resta com'è, salvo l'elenco dei conflitti ignorati. */
  async function persist(next: EngineRules, doneText: string, keepDraft = false) {
    if (!backend.setEngineRules) return;
    setSaving(true);
    setNotice(null);
    try {
      const saved = await backend.setEngineRules(next);
      const profiles = backend.getEngineProfiles ? await backend.getEngineProfiles() : null;
      if (keepDraft) {
        seen.current = JSON.stringify(saved.rules);
        onSaved({ engine: saved, profiles });
        const ignoredConflicts = [...saved.rules.ignoredConflicts];
        setBase((b) => ({ ...b, ignoredConflicts }));
        patch({ ignoredConflicts });
      } else adopt(saved, profiles);
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

  async function profileCommand(command: ProfileCommand): Promise<boolean> {
    if (!backend.applyEngineProfile || !backend.getEngineProfiles) return false;
    setSaving(true);
    setNotice(null);
    try {
      const engine = await backend.applyEngineProfile(command);
      const profiles = await backend.getEngineProfiles();
      const active = profiles.profiles.find((p) => p.id === profiles.activeId);
      const name = active?.name ?? '';
      const text =
        command.op === 'select'
          ? t('maps.changed', { name })
          : command.op === 'clone'
            ? t('maps.cloned', { name: command.name })
            : command.op === 'create'
              ? t('maps.created', { name: command.name })
              : command.op === 'rename'
                ? t('maps.renamed', { name: command.name })
                : command.op === 'duplicate'
                  ? t('maps.duplicated')
                  : t('maps.deleted');
      if (profiles.activeId !== data.profiles?.activeId)
        onSaved({ engine, profiles }, { tone: 'info', text }); // l'editor riparte (key)
      else if (command.op === 'select') {
        adopt(engine, profiles);
        setNotice({ tone: 'info', text });
      } else {
        // Le regole attive non cambiano: la bozza in corso resta.
        seen.current = JSON.stringify(engine.rules);
        onSaved({ engine, profiles });
        setNotice({ tone: 'info', text });
      }
      return true;
    } catch (error) {
      setNotice({
        tone: 'danger',
        text: t('common.actionFailed', { message: errorMessage(error) }),
      });
      return false;
    } finally {
      setSaving(false);
    }
  }

  /** Ignora (o smette di ignorare) un conflitto: salva subito solo quell'elenco, senza toccare il resto della bozza. */
  async function setIgnored(id: string, ignore: boolean) {
    const current = rulesNow.ignoredConflicts;
    const next = ignore
      ? current.some((c) => c.toLowerCase() === id.toLowerCase())
        ? current
        : [...current, id]
      : current.filter((c) => c.toLowerCase() !== id.toLowerCase());
    await persist(
      { ...rulesNow, ignoredConflicts: next },
      t(ignore ? 'rules.conflict.ignored' : 'rules.conflict.unignored'),
      true,
    );
  }

  /** Accende il motore con le regole salvate (stesso comando del salvataggio); della bozza cambia solo l'interruttore. */
  async function turnOn() {
    if (noList) {
      setNotice({ tone: 'danger', text: t('maps.noList') });
      return;
    }
    await persist({ ...rulesNow, enabled: true }, t('rules.turnedOn'), true);
    setBase((b) => ({ ...b, enabled: true }));
    patch({ enabled: true });
  }

  const view = data.engine.view;
  const suspended = view?.suspended ?? null;
  const isOff = view ? suspended === 'disabled' || !view.enabled : false;
  const ignored = rulesNow.ignoredConflicts.map((c) => c.toLowerCase());
  const conflictById = new Map(data.engine.conflicts.map((c) => [c.id.toLowerCase(), c]));
  const candidate = view?.candidate;

  return (
    <div className="sd-page">
      <h2>{t('rules.title')}</h2>
      <p className="sd-hint">{t('rules.hint')}</p>

      {view && (
        <>
          {suspended && suspended !== 'disabled' ? (
            <div
              className="sd-notice"
              data-tone="danger"
              role="alert"
              aria-label={t('rules.status')}
            >
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
            </div>
          ) : isOff ? (
            <div
              className="sd-notice sd-engine-off"
              data-testid="engine-off"
              role="status"
              aria-label={t('rules.status')}
            >
              <span>{t('rules.off')}</span>
              <Tooltip text={t('rules.turnOn.tip')}>
                <button
                  type="button"
                  className="sd-btn sd-btn--small"
                  disabled={busy}
                  onClick={() => void turnOn()}
                >
                  {t('rules.turnOn')}
                </button>
              </Tooltip>
            </div>
          ) : (
            <>
              <div className="sd-notice" role="status" aria-label={t('rules.status')}>
                {t('rules.active')}
                {!view.holder && ` · ${t('rules.observer')}`}
              </div>
              {view.dryRun && (
                <div className="sd-notice" data-testid="engine-observe" role="status">
                  {t('rules.dryRunOn')}
                </div>
              )}
            </>
          )}
        </>
      )}

      {candidate && (
        <p className="sd-flow__next" data-testid="flow-next">
          <strong>{t('flow.next', { name: nameOf(candidate.number) })}</strong>
          {' — '}
          {candidate.fallback ? `${t('flow.next.fallback')}: ` : ''}
          {candidate.why}
        </p>
      )}

      {data.profiles && (
        <MapSelector
          profiles={data.profiles}
          busy={busy}
          dirty={dirty}
          onCommand={profileCommand}
        />
      )}

      <form
        className="sd-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (!hasErrors && dirty) void persist(rules, t('rules.saved'));
        }}
      >
        <section
          className="sd-mapbox"
          data-locked={locked}
          {...(activeProfile ? { 'aria-labelledby': boxId } : {})}
        >
          {activeProfile && (
            <h3 id={boxId}>{t('maps.box.title', { name: profileLabel(activeProfile, t) })}</h3>
          )}
          {locked && (
            <div className="sd-notice sd-mapbox__locked" data-testid="map-readonly">
              <span>{t(noList ? 'maps.noList' : 'maps.readOnly')}</span>
              <Tooltip text={t('maps.clone.tip')}>
                <button
                  type="button"
                  className="sd-btn sd-btn--small"
                  disabled={busy || cloning}
                  onClick={() => setCloning(true)}
                >
                  {t('maps.clone')}
                </button>
              </Tooltip>
            </div>
          )}
          {locked && cloning && activeProfile && (
            <MapNameForm
              initial=""
              busy={busy}
              onCancel={() => setCloning(false)}
              onSubmit={(name) => profileCommand({ op: 'clone', id: activeProfile.id, name })}
            />
          )}
          <Section title={t('flow.title')} hint={t('flow.hint')} level={4}>
            {missing.length > 0 && (
              <div className="sd-notice" data-tone="warn" role="status">
                {t('flow.missingWarn', { list: missing.map((n) => `#${n}`).join(', ') })}{' '}
                <Tooltip text={t('flow.removeMissing.tip')}>
                  <button
                    type="button"
                    className="sd-btn sd-btn--small"
                    disabled={busy || locked}
                    onClick={() =>
                      patch({
                        order: draft.order.filter((n) => !missing.includes(n)),
                        excluded: draft.excluded.filter((n) => !missing.includes(n)),
                      })
                    }
                  >
                    {t('flow.removeMissing')}
                  </button>
                </Tooltip>
              </div>
            )}
            <FlowList
              rows={rows}
              nameOf={nameOf}
              disabled={busy || locked}
              onMove={(n, to) =>
                setDraft((d) =>
                  moveRow(
                    d,
                    accounts,
                    flowRows(d, accounts).findIndex((r) => r.number === n),
                    to,
                  ),
                )
              }
              onToggle={(n, included) => setDraft((d) => setIncluded(d, accounts, n, included))}
            />
            {errors.order && (
              <span className="sd-field-error" role="alert">
                {t(errors.order)}
              </span>
            )}
          </Section>

          <Section title={t('rules.thresholds')} level={4}>
            <SelectField
              label={t('rules.criterion')}
              tip={t('flow.criterion.tip')}
              hint={t(`flow.criterion.hint.${draft.reserveCriterion}`)}
              value={draft.reserveCriterion}
              onChange={(value) => patch({ reserveCriterion: value as ReserveCriterion })}
              disabled={busy || locked}
              options={CRITERIA.map((c) => ({ value: c, label: t(`rules.criterion.${c}`) }))}
            />
            {draft.reserveCriterion === 'balanced-score' && (
              <fieldset className="sd-fieldset">
                <legend>{t('flow.weights')}</legend>
                <span className="sd-hint">{t('flow.weights.hint')}</span>
                <div className="sd-weights">
                  {WEIGHTS.map((w) => (
                    <TextField
                      key={w}
                      label={t(`flow.weight.${w}`)}
                      tip={t('flow.weight.tip')}
                      type="number"
                      min={0}
                      max={100}
                      step="any"
                      value={draft.weights[w]}
                      onChange={(value) => patch({ weights: { ...draft.weights, [w]: value } })}
                      disabled={busy || locked}
                    />
                  ))}
                </div>
                {errors.weights && <span className="sd-field-error">{t(errors.weights)}</span>}
              </fieldset>
            )}
            {(data.engine.warnings ?? []).map((w) => (
              <div key={w} className="sd-notice" data-tone="warn" role="status">
                {w}
              </div>
            ))}
            {draft.reserveCriterion === 'model-headroom' && (
              <TextField
                label={t('flow.model')}
                hint={t('flow.model.hint')}
                tip={t('flow.model.tip')}
                value={draft.headroomModel}
                onChange={(headroomModel) => patch({ headroomModel })}
                error={errors.model ? t(errors.model) : null}
                disabled={busy || locked}
              />
            )}
            <TextField
              label={t('rules.fiveHour')}
              tip={t('rules.fiveHour.tip')}
              type="number"
              min={1}
              max={100}
              step="any"
              value={draft.fiveHourThreshold}
              onChange={(value) => patch({ fiveHourThreshold: value })}
              error={errors.fiveHour ? t(errors.fiveHour) : null}
              disabled={busy || locked}
            />
            <TextField
              label={t('rules.sevenDay')}
              tip={t('rules.sevenDay.tip')}
              type="number"
              min={1}
              max={100}
              hint={t('rules.sevenDay.hint')}
              value={draft.sevenDayThreshold}
              onChange={(value) => patch({ sevenDayThreshold: value })}
              error={errors.sevenDay ? t(errors.sevenDay) : null}
              disabled={busy || locked}
            />
            <CheckField
              label={t('rules.return')}
              tip={t('rules.return.tip')}
              checked={draft.returnToPrimary}
              onChange={(returnToPrimary) => patch({ returnToPrimary })}
              disabled={busy || locked}
            />
            <TextField
              label={t('rules.hysteresis')}
              tip={t('rules.hysteresis.tip')}
              type="number"
              min={0}
              max={50}
              hint={t('rules.hysteresis.hint')}
              value={draft.returnHysteresis}
              onChange={(value) => patch({ returnHysteresis: value })}
              error={errors.hysteresis ? t(errors.hysteresis) : null}
              disabled={busy || locked}
            />
            <TextField
              label={t('rules.gap')}
              tip={t('rules.gap.tip')}
              type="number"
              min={0}
              hint={t('rules.gap.hint')}
              value={draft.minSwitchGapSecs}
              onChange={(value) => patch({ minSwitchGapSecs: value })}
              error={errors.gap ? t(errors.gap) : null}
              disabled={busy || locked}
            />
          </Section>
        </section>

        <Section title={t('rules.engine')}>
          <CheckField
            label={t('rules.enabled')}
            tip={t('rules.enabled.tip')}
            checked={draft.enabled}
            onChange={(enabled) => patch({ enabled })}
            disabled={busy}
          />
          <CheckField
            label={t('rules.dryRun')}
            hint={t('rules.dryRun.hint')}
            tip={t('rules.dryRun.tip')}
            checked={draft.dryRun}
            onChange={(dryRun) => patch({ dryRun })}
            disabled={busy}
          />
        </Section>

        <NoticeView notice={notice} />
        <div className="sd-row sd-row--end">
          <Tooltip text={t('rules.revert.tip')}>
            <button
              type="button"
              className="sd-btn"
              disabled={busy || !dirty}
              onClick={() => {
                setDraft(base);
                setNotice(null);
              }}
            >
              {t('rules.revert')}
            </button>
          </Tooltip>
          <Tooltip text={t('rules.save.tip')}>
            <button
              type="submit"
              className="sd-btn sd-btn--primary"
              disabled={busy || hasErrors || !dirty}
            >
              {saving ? t('common.working') : t('common.save')}
            </button>
          </Tooltip>
        </div>
      </form>

      {data.engine.conflicts.length > 0 && (
        <Section title={t('rules.conflicts')} hint={t('rules.conflicts.hint')}>
          <ul className="sd-list" aria-label={t('rules.conflicts')}>
            {data.engine.conflicts.map((c) => {
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
                  <Tooltip
                    text={t(
                      isIgnored ? 'rules.conflict.unignore.tip' : 'rules.conflict.ignore.tip',
                    )}
                  >
                    <button
                      type="button"
                      className="sd-btn sd-btn--small"
                      disabled={busy}
                      aria-label={t(
                        isIgnored ? 'rules.conflict.unignoreLabel' : 'rules.conflict.ignoreLabel',
                        { label: c.label },
                      )}
                      onClick={() => void setIgnored(c.id, !isIgnored)}
                    >
                      {t(isIgnored ? 'rules.conflict.unignore' : 'rules.conflict.ignore')}
                    </button>
                  </Tooltip>
                </li>
              );
            })}
          </ul>
        </Section>
      )}
    </div>
  );
}
