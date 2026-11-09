import type { HostMessage } from '@swapdeck/host';
import { MessageBackend } from '@swapdeck/host/backend';
import { HostApp, readState } from '@swapdeck/host/webview';
import '@swapdeck/host/webview.css';
import type { TabId } from '@swapdeck/ui';
import '@swapdeck/ui/styles.css';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

const vscode = acquireVsCodeApi();
const backend = new MessageBackend(
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
 * App completa a schede (`HostApp` di `@swapdeck/host/webview`), nella vista della barra laterale e
 * nella scheda dell’editor: lingua scelta in SwapDeck (o di VS Code), tema di VS Code.
 */
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <HostApp backend={backend} initialTab={saved.tab} onTabChange={rememberTab} />
  </StrictMode>,
);
