import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  BackendProvider,
  Dashboard,
  I18nProvider,
  LanguageSelect,
  MiniWidget,
  ThemeProvider,
  ThemeSelect,
} from '@swapdeck/ui';
import '@swapdeck/ui/styles.css';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { AutostartToggle } from './AutostartToggle.js';
import { TauriBackend } from './TauriBackend.js';
import './desktop.css';

const backend = new TauriBackend(invoke, listen);
const appWindow = getCurrentWindow();
const isMini = appWindow.label === 'mini';

/** Mini-widget: si trascina da qualunque punto non interattivo; doppio clic apre la finestra principale. */
function Mini() {
  return (
    <div
      className="sd-desktop-mini"
      onMouseDown={(event) => {
        const target = event.target as HTMLElement;
        if (event.button !== 0 || target.closest('button, a, input, select')) return;
        if (event.detail === 2) void backend.showMain();
        else void appWindow.startDragging();
      }}
    >
      <MiniWidget />
    </div>
  );
}

function Main() {
  return (
    <Dashboard
      toolbarExtra={
        <span className="sd-toolbar__group">
          <LanguageSelect />
          <ThemeSelect />
          <AutostartToggle backend={backend} />
        </span>
      }
    />
  );
}

document.documentElement.dataset['window'] = isMini ? 'mini' : 'main';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <ThemeProvider>
      <I18nProvider>
        <BackendProvider backend={backend}>{isMini ? <Mini /> : <Main />}</BackendProvider>
      </I18nProvider>
    </ThemeProvider>
  </StrictMode>,
);
