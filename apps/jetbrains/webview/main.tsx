import { MessageBackend } from '@swapdeck/host/backend';
import { HostApp } from '@swapdeck/host/webview';
import '@swapdeck/host/webview.css';
import type { TabId } from '@swapdeck/ui';
import '@swapdeck/ui/styles.css';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { browserStorage, loadState, saveState } from './tabStore.js';
import { createJcefTransport } from './transport.js';

const transport = createJcefTransport(window);
const backend = new MessageBackend(transport.post, transport.subscribe);

/** Scheda aperta: ricordata in `localStorage` tra una riapertura e l'altra della tool window. */
const storage = browserStorage();
let saved = loadState(storage);
const rememberTab = (tab: TabId) => {
  if (saved.tab === tab) return;
  saved = { ...saved, tab };
  saveState(storage, saved);
};

/**
 * App completa a schede (`HostApp` di `@swapdeck/host/webview`) nella tool window «SwapDeck»: lingua
 * scelta in SwapDeck (o dell'IDE), tema dell'IDE, dal messaggio `view` del plugin.
 */
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <HostApp backend={backend} initialTab={saved.tab} onTabChange={rememberTab} />
  </StrictMode>,
);
