import { BackendProvider, Dashboard, I18nProvider, ThemeProvider } from '@swapdeck/ui';
import '@swapdeck/ui/styles.css';
import { StrictMode, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import type { HostMessage, ViewSettings } from '../protocol.js';
import { VsCodeBackend } from './VsCodeBackend.js';
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

/** Lingua e tema seguono VS Code: a ogni cambio la UI si rimonta con i nuovi valori. */
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
          <Dashboard />
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
