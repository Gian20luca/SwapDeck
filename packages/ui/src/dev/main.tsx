import { StrictMode, useEffect, useId, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { BackendProvider } from '../backend/context.js';
import { Dashboard } from '../components/Dashboard.js';
import { MiniWidget } from '../components/MiniWidget.js';
import { LanguageSelect, ThemeSelect } from '../components/PreferenceControls.js';
import { I18nProvider, useI18n } from '../i18n/index.js';
import '../styles/index.css';
import { ThemeProvider } from '../theme/index.js';
import { FakeBackend } from './FakeBackend.js';
import { SCENARIO_NAMES, type ScenarioName } from './fixtures.js';

type View = 'full' | 'mini';

/** Anteprima di sviluppo: la dashboard con il FakeBackend, vista completa o mini-widget. */
function Preview() {
  const { t } = useI18n();
  const [view, setView] = useState<View>('full');
  const [scenario, setScenario] = useState<ScenarioName>('default');
  const viewId = useId();
  const scenarioId = useId();
  const backend = useMemo(() => new FakeBackend({ scenario, latencyMs: 250 }), [scenario]);

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
          {view === 'mini' && preferences}
        </div>
      </div>
      {view === 'full' ? (
        <Dashboard toolbarExtra={preferences} />
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
