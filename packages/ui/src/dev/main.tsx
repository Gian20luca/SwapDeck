import { StrictMode, useEffect, useId, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import type { CswapAction } from '@swapdeck/types';
import { BackendProvider } from '../backend/context.js';
import { ConfirmDialog } from '../components/ConfirmDialog.js';
import { MiniWidget } from '../components/MiniWidget.js';
import { SwapDeckApp } from '../components/SwapDeckApp.js';
import { LanguageSelect, ThemeSelect } from '../components/PreferenceControls.js';
import { I18nProvider, useI18n } from '../i18n/index.js';
import '../styles/index.css';
import { ThemeProvider } from '../theme/index.js';
import { FakeBackend } from './FakeBackend.js';
import { SCENARIO_NAMES, type ScenarioName } from './fixtures.js';

type View = 'full' | 'mini';

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
  const backend = useMemo(
    () =>
      new FakeBackend({
        scenario,
        latencyMs: 250,
        unclaimed: ['legacy-1f3a', 'legacy-9c2b'],
        mappings: [
          { path: String.raw`C:\Progetti\cliente-a`, account: 1 },
          { path: String.raw`C:\Progetti\cliente-b`, account: 3 },
        ],
        conflicts: [
          {
            id: 'task:cswap-guard',
            kind: 'scheduledTask',
            label: 'Attività pianificata cswap-guard',
            certain: true,
          },
          { id: 'process:tui', kind: 'process', label: 'cswap tui in esecuzione', certain: false },
        ],
        rules: { enabled: true, primary: 6, reserves: [1, 3] },
      }),
    [scenario],
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
          <ConfirmDemo backend={backend} />
          {view === 'mini' && preferences}
        </div>
      </div>
      {view === 'full' ? (
        <SwapDeckApp toolbarExtra={preferences} />
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
