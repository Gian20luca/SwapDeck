import {
  BackendProvider,
  I18nProvider,
  SwapDeckApp,
  ThemeProvider,
  type TabId,
} from '@swapdeck/ui';
import '@swapdeck/ui/styles.css';
import { StrictMode, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import type { HostMessage, ViewSettings } from '../protocol.js';
import { EngineBanner } from './EngineBanner.js';
import { VsCodeBackend } from './VsCodeBackend.js';
import { readState } from './viewState.js';
import './webview.css';

const vscode = acquireVsCodeApi();
const backend = new VsCodeBackend(
  (message) => vscode.postMessage(message),
  (handler) => {
    const listener = (event: MessageEvent<HostMessage>) => handler(event.data);
    window.addEventListener('message', listener);
    return () => window.removeEventListener('message', listener);
  },
);

/** Scheda aperta: ricordata da VS Code quando la vista si nasconde o la scheda passa in secondo piano. */
let saved = readState(vscode.getState());
const rememberTab = (tab: TabId) => {
  if (saved.tab === tab) return;
  saved = { ...saved, tab };
  vscode.setState(saved);
};

/**
 * App completa a schede, nella vista della barra laterale e nella scheda dell’editor. La lingua è
 * quella scelta in SwapDeck (o di VS Code), il tema quello di VS Code: a ogni cambio la UI si
 * rimonta con i nuovi valori, sulla scheda salvata. Le
 * conferme le chiede l'host (`hostConfirms`).
 */
function App() {
  const [view, setView] = useState<ViewSettings | null>(null);
  useEffect(() => {
    const off = backend.onViewSettings(setView);
    backend.ready();
    return off;
  }, []);
  if (!view) return null;
  return (
    <ThemeProvider key={`${view.locale}-${view.theme}`} initial={view.theme}>
      <I18nProvider initial={view.locale}>
        <BackendProvider backend={backend}>
          <SwapDeckApp
            header={<EngineBanner backend={backend} />}
            {...(saved.tab ? { initialTab: saved.tab } : {})}
            onTabChange={rememberTab}
          />
        </BackendProvider>
      </I18nProvider>
    </ThemeProvider>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
