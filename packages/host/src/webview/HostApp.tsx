import {
  BackendProvider,
  I18nProvider,
  SwapDeckApp,
  ThemeProvider,
  type TabId,
} from '@swapdeck/ui';
import { useEffect, useState } from 'react';
import type { MessageBackend } from '../MessageBackend.js';
import type { ViewSettings } from '../protocol.js';
import { EngineBanner } from './EngineBanner.js';

export interface HostAppProps {
  backend: MessageBackend;
  /** Scheda da aprire, ricordata dall'host. */
  initialTab?: TabId | undefined;
  /** A ogni cambio di scheda, per ricordarla. */
  onTabChange?: (tab: TabId) => void;
}

/**
 * App completa a schede dentro un IDE (VS Code, JetBrains). Aspetta il primo messaggio `view`
 * dell'host (lingua scelta in SwapDeck o dell'IDE, tema dell'IDE) e a ogni cambio rimonta la UI con
 * i nuovi valori, sulla scheda ricordata. Le conferme le chiede l'host (`hostConfirms`).
 */
export function HostApp({ backend, initialTab, onTabChange }: HostAppProps) {
  const [view, setView] = useState<ViewSettings | null>(null);
  useEffect(() => {
    const off = backend.onViewSettings(setView);
    backend.ready();
    return off;
  }, [backend]);
  if (!view) return null;
  return (
    <ThemeProvider key={`${view.locale}-${view.theme}`} initial={view.theme}>
      <I18nProvider initial={view.locale}>
        <BackendProvider backend={backend}>
          <SwapDeckApp
            header={<EngineBanner backend={backend} />}
            {...(initialTab ? { initialTab } : {})}
            {...(onTabChange ? { onTabChange } : {})}
          />
        </BackendProvider>
      </I18nProvider>
    </ThemeProvider>
  );
}
