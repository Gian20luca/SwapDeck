/** Scheda aperta ricordata in `localStorage` (JCEF non ha `getState`/`setState` come VS Code). */
import { readState, type WebviewState } from '@swapdeck/host/webview';

export const STATE_KEY = 'swapdeck.webview.state';

/** Stato salvato; vuoto se manca, non è leggibile o `localStorage` non è disponibile. */
export function loadState(storage: Pick<Storage, 'getItem'> | undefined): WebviewState {
  try {
    const raw = storage?.getItem(STATE_KEY);
    return raw ? readState(JSON.parse(raw)) : {};
  } catch {
    return {};
  }
}

/** Salva lo stato; un errore (spazio, accesso negato) si ignora: la scheda non si ricorda e basta. */
export function saveState(
  storage: Pick<Storage, 'setItem'> | undefined,
  state: WebviewState,
): void {
  try {
    storage?.setItem(STATE_KEY, JSON.stringify(state));
  } catch {
    // niente da fare
  }
}

/** `window.localStorage`, che in alcune configurazioni lancia già alla lettura. */
export function browserStorage(): Storage | undefined {
  try {
    return window.localStorage;
  } catch {
    return undefined;
  }
}
