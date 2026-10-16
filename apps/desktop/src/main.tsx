import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open, save } from '@tauri-apps/plugin-dialog';
import {
  BackendProvider,
  I18nProvider,
  MiniWidget,
  SwapDeckApp,
  ThemeProvider,
  type LocalePreference,
  type ThemeMode,
} from '@swapdeck/ui';
import '@swapdeck/ui/styles.css';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { EngineStatus } from './EngineStatus.js';
import { TauriBackend } from './TauriBackend.js';
import './desktop.css';

const backend = new TauriBackend(invoke, listen, { open, save });
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
  return <SwapDeckApp header={<EngineStatus backend={backend} />} />;
}

document.documentElement.dataset['window'] = isMini ? 'mini' : 'main';

/** Lingua e tema salvati nella config; se l'app non li legge (comando assente, finestra senza permesso) valgono i default. */
async function initialPreferences(): Promise<{ language: LocalePreference; theme: ThemeMode }> {
  try {
    const prefs = await Promise.race([
      backend.getPreferences(),
      new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), 1500)),
    ]);
    return { language: prefs.language, theme: prefs.theme === 'system' ? 'auto' : prefs.theme };
  } catch {
    return { language: 'auto', theme: 'auto' };
  }
}

void initialPreferences().then(({ language, theme }) => {
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <ThemeProvider initial={theme}>
        <I18nProvider initial={language}>
          <BackendProvider backend={backend}>{isMini ? <Mini /> : <Main />}</BackendProvider>
        </I18nProvider>
      </ThemeProvider>
    </StrictMode>,
  );
});
