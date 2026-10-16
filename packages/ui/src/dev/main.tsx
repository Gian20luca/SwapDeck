import { StrictMode, useEffect, useId, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import type { CswapAction } from '@swapdeck/types';
import { BackendProvider } from '../backend/context.js';
import { ConfirmDialog } from '../components/ConfirmDialog.js';
import { CswapUpdateNotice } from '../components/CswapUpdateNotice.js';
import { Dashboard } from '../components/Dashboard.js';
import { MiniWidget } from '../components/MiniWidget.js';
import { LanguageSelect, ThemeSelect } from '../components/PreferenceControls.js';
import { SetupWizard } from '../components/SetupWizard.js';
import { I18nProvider, useI18n } from '../i18n/index.js';
import '../styles/index.css';
import { ThemeProvider } from '../theme/index.js';
import { FakeBackend } from './FakeBackend.js';
import type { FakeSetupOptions } from './fakeSetup.js';
import { SCENARIO_NAMES, type ScenarioName } from './fixtures.js';

type View = 'full' | 'mini';

/** Stati di cswap per provare il primo avvio e l'avviso di aggiornamento. */
const SETUP_DEMOS: Record<string, FakeSetupOptions> = {
  ok: {},
  update: { latest: '0.28.0' },
  updateBusy: { latest: '0.28.0', failure: { kind: 'busy', message: 'os error 32' } },
  missing: { state: 'missing' },
  missingUv: { state: 'missing', uv: false },
  old: { state: 'unsupportedVersion', installed: '0.25.0', latest: '0.27.2' },
  schema: { state: 'unsupportedSchema' },
};

const DEMO_ACTIONS: Record<'simple' | 'typed', CswapAction> = {
  simple: { kind: 'remove', account: 1 },
  typed: { kind: 'purge' },
};

/** Prova dei dialoghi di conferma: la regola viene da `confirmationFor` del backend; l'azione non viene eseguita. */
function ConfirmDemo({ backend }: { backend: FakeBackend }) {
  const [pending, setPending] = useState<{
    kind: 'simple' | 'typed';
    word: string | undefined;
  } | null>(null);
  const [result, setResult] = useState('');
  async function ask(kind: 'simple' | 'typed') {
    const confirmation = await backend.confirmationFor(DEMO_ACTIONS[kind]);
    setPending({ kind, word: confirmation.kind === 'typed' ? confirmation.word : undefined });
  }
  return (
    <span className="sd-toolbar__group">
      <button type="button" className="sd-btn" onClick={() => void ask('simple')}>
        Demo conferma semplice
      </button>
      <button type="button" className="sd-btn" onClick={() => void ask('typed')}>
        Demo doppia conferma
      </button>
      {result && <span role="status">{result}</span>}
      <ConfirmDialog
        open={pending !== null}
        title={pending?.kind === 'typed' ? 'Eliminare tutti i dati?' : 'Rimuovere l’account?'}
        message="Anteprima: l’azione non viene eseguita."
        tone="danger"
        {...(pending?.word ? { requireWord: pending.word } : {})}
        onConfirm={() => {
          setResult(`Confermato: ${pending?.kind}`);
          setPending(null);
        }}
        onCancel={() => {
          setResult('Annullato');
          setPending(null);
        }}
      />
    </span>
  );
}

/** Anteprima di sviluppo: la dashboard con il FakeBackend, vista completa o mini-widget. */
function Preview() {
  const { t } = useI18n();
  const [view, setView] = useState<View>('full');
  const [scenario, setScenario] = useState<ScenarioName>('default');
  const viewId = useId();
  const scenarioId = useId();
  const setupId = useId();
  const [setupDemo, setSetupDemo] = useState('ok');
  const [ready, setReady] = useState(false);
  const backend = useMemo(
    () => new FakeBackend({ scenario, latencyMs: 250, setup: SETUP_DEMOS[setupDemo] ?? {} }),
    [scenario, setupDemo],
  );

  // Simula gli aggiornamenti periodici del motore.
  useEffect(() => {
    const id = setInterval(() => backend.tick(), 15_000);
    return () => clearInterval(id);
  }, [backend]);

  const preferences = (
    <span className="sd-toolbar__group">
      <LanguageSelect />
      <ThemeSelect />
    </span>
  );

  return (
    <BackendProvider backend={backend}>
      <div className="sd-app" style={{ paddingBottom: 0 }}>
        <div className="sd-toolbar">
          <span className="sd-field">
            <label htmlFor={viewId}>{t('settings.view')}</label>
            <select
              id={viewId}
              className="sd-input sd-input--inline"
              value={view}
              onChange={(event) => setView(event.target.value as View)}
            >
              <option value="full">{t('settings.view.full')}</option>
              <option value="mini">{t('settings.view.mini')}</option>
            </select>
          </span>
          <span className="sd-field">
            <label htmlFor={scenarioId}>Fixture</label>
            <select
              id={scenarioId}
              className="sd-input sd-input--inline"
              value={scenario}
              onChange={(event) => setScenario(event.target.value as ScenarioName)}
            >
              {SCENARIO_NAMES.map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
          </span>
          <span className="sd-field">
            <label htmlFor={setupId}>cswap</label>
            <select
              id={setupId}
              className="sd-input sd-input--inline"
              value={setupDemo}
              onChange={(event) => {
                setReady(false);
                setSetupDemo(event.target.value);
              }}
            >
              {Object.keys(SETUP_DEMOS).map((name) => (
                <option key={name} value={name}>
                  {name}
                </option>
              ))}
            </select>
          </span>
          <ConfirmDemo backend={backend} />
          {view === 'mini' && preferences}
        </div>
      </div>
      {!ready ? (
        <div className="sd-app">
          <SetupWizard key={setupDemo + scenario} onReady={() => setReady(true)} />
        </div>
      ) : view === 'full' ? (
        <>
          <div className="sd-app" style={{ paddingBottom: 0 }}>
            <CswapUpdateNotice key={setupDemo} />
          </div>
          <Dashboard toolbarExtra={preferences} />
        </>
      ) : (
        <div style={{ width: 280, margin: '12px auto', border: '1px solid var(--sd-border)' }}>
          <MiniWidget />
        </div>
      )}
    </BackendProvider>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <I18nProvider>
        <Preview />
      </I18nProvider>
    </ThemeProvider>
  </StrictMode>,
);
