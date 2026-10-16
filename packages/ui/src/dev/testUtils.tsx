import { render, type RenderResult } from '@testing-library/react';
import type { ReactElement } from 'react';
import { BackendProvider } from '../backend/context.js';
import type { Backend } from '../backend/types.js';
import { I18nProvider, type LocalePreference } from '../i18n/index.js';

/** Disegna un componente dentro i provider di i18n e backend (solo per i test). */
export function renderWith(
  ui: ReactElement,
  backend: Backend,
  locale: LocalePreference = 'it',
): RenderResult {
  return render(
    <I18nProvider initial={locale}>
      <BackendProvider backend={backend}>{ui}</BackendProvider>
    </I18nProvider>,
  );
}

/** Un backend con i soli metodi obbligatori: simula un host senza le funzioni di gestione. */
export function bareBackend(source: Backend): Backend {
  return {
    listAccounts: (options) => source.listAccounts(options),
    getStatus: () => source.getStatus(),
    getConfig: () => source.getConfig(),
    getConfigValue: (key) => source.getConfigValue(key),
    getHistory: (query) => source.getHistory(query),
    confirmationFor: (action) => source.confirmationFor(action),
    runAction: (action) => source.runAction(action),
    onStateChanged: (listener) => source.onStateChanged(listener),
  };
}
