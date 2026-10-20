import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open, save } from '@tauri-apps/plugin-dialog';
import {
  BackendProvider,
  I18nProvider,
  MiniWidget,
  SwapDeckApp,
  WindowTheme,
  ThemeProvider,
  type LocalePreference,
  type ThemeMode,
} from '@swapdeck/ui';
import '@swapdeck/ui/styles.css';
import { StrictMode, useRef } from 'react';
import { createRoot } from 'react-dom/client';
import { EngineStatus } from './EngineStatus.js';
import { TauriBackend } from './TauriBackend.js';
import './desktop.css';

const appWindow = getCurrentWindow();
const backend = new TauriBackend(invoke, listen, { open, save }, appWindow);
const isMini = appWindow.label === 'mini';

/** Spostamento minimo (px) prima che un clic diventi un trascinamento. */
const DRAG_THRESHOLD = 4;

/**
 * Mini-widget: si trascina da qualunque punto non interattivo (il trascinamento parte quando il
 * mouse si muove, così un clic semplice sulla striscia la riespande). Nessun clic apre la finestra
 * principale: si apre dall'icona nell'area di notifica.
 */
function Mini() {
  const start = useRef<{ x: number; y: number } | null>(null);
  return (
    <div
      className="sd-desktop-mini"
      onMouseDown={(event) => {
        const target = event.target as HTMLElement;
        start.current = null;
        if (event.button !== 0 || target.closest('button, a, input, select')) return;
        start.current = { x: event.clientX, y: event.clientY };
      }}
      onMouseMove={(event) => {
        const from = start.current;
        if (!from || (event.buttons & 1) === 0) return;
        if (Math.hypot(event.clientX - from.x, event.clientY - from.y) < DRAG_THRESHOLD) return;
        start.current = null;
        void appWindow.startDragging();
      }}
      onMouseUp={() => {
        start.current = null;
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
          <BackendProvider backend={backend}>
            <WindowTheme />
            {isMini ? <Mini /> : <Main />}
          </BackendProvider>
        </I18nProvider>
      </ThemeProvider>
    </StrictMode>,
  );
});
