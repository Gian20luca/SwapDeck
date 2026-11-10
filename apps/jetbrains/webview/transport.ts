/**
 * Trasporto dei messaggi tra il pannello in JCEF e il plugin Kotlin (contratto nel README).
 *
 * - Pannello → host: `window.__swapdeckHost.post(json)`, funzione iniettata da Kotlin con
 *   `JBCefJSQuery` dopo il caricamento della pagina. Finché non c'è, i messaggi restano in coda e
 *   partono, in ordine, all'evento `swapdeck-host-ready` o al controllo periodico.
 * - Host → pannello: `window.dispatchEvent(new CustomEvent('swapdeck-host', { detail }))` lanciato
 *   da Kotlin con `executeJavaScript`; `detail` è il messaggio in JSON (stringa) o già come oggetto.
 */
import type { HostMessage, WebviewMessage } from '@swapdeck/host';
import type { PostFn, SubscribeFn } from '@swapdeck/host/backend';

/** Evento con cui l'host consegna un messaggio al pannello. */
export const HOST_MESSAGE_EVENT = 'swapdeck-host';
/** Evento lanciato dall'host subito dopo aver iniettato `window.__swapdeckHost`. */
export const HOST_READY_EVENT = 'swapdeck-host-ready';
/** Ogni quanto si cerca la funzione iniettata, se l'evento di pronto non arriva. */
export const BRIDGE_POLL_MS = 100;

/** Oggetto iniettato da Kotlin: `post` riceve un messaggio del pannello in JSON. */
export interface HostBridge {
  post(json: string): void;
}

/** La parte di `window` che il trasporto usa (in prova basta un `EventTarget`). */
export interface TransportWindow extends EventTarget {
  __swapdeckHost?: HostBridge | undefined;
  setInterval(handler: () => void, ms: number): number;
  clearInterval(id: number): void;
}

export interface JcefTransport {
  post: PostFn;
  subscribe: SubscribeFn;
  /** Messaggi in attesa della funzione iniettata. */
  pending(): number;
}

function bridgeOf(win: TransportWindow): HostBridge | undefined {
  const bridge = win.__swapdeckHost;
  return bridge && typeof bridge.post === 'function' ? bridge : undefined;
}

/** Messaggio dell'host da `detail`: JSON o oggetto con `type` stringa; altrimenti `undefined`. */
export function hostMessageOf(detail: unknown): HostMessage | undefined {
  let value = detail;
  if (typeof value === 'string') {
    try {
      value = JSON.parse(value);
    } catch {
      return undefined;
    }
  }
  if (typeof value !== 'object' || value === null) return undefined;
  return typeof (value as { type?: unknown }).type === 'string'
    ? (value as HostMessage)
    : undefined;
}

export function createJcefTransport(win: TransportWindow): JcefTransport {
  const queue: string[] = [];
  let timer: number | undefined;

  const flush = () => {
    const bridge = bridgeOf(win);
    if (!bridge) return;
    if (timer !== undefined) {
      win.clearInterval(timer);
      timer = undefined;
    }
    win.removeEventListener(HOST_READY_EVENT, flush);
    while (queue.length > 0) bridge.post(queue.shift()!);
  };

  const post: PostFn = (message: WebviewMessage) => {
    const json = JSON.stringify(message);
    const bridge = bridgeOf(win);
    if (bridge && queue.length === 0) {
      bridge.post(json);
      return;
    }
    queue.push(json);
    if (bridge) {
      flush();
    } else if (timer === undefined) {
      win.addEventListener(HOST_READY_EVENT, flush);
      timer = win.setInterval(flush, BRIDGE_POLL_MS);
    }
  };

  const subscribe: SubscribeFn = (handler) => {
    const listener = (event: Event) => {
      const message = hostMessageOf((event as CustomEvent<unknown>).detail);
      if (message) handler(message);
    };
    win.addEventListener(HOST_MESSAGE_EVENT, listener);
    return () => win.removeEventListener(HOST_MESSAGE_EVENT, listener);
  };

  return { post, subscribe, pending: () => queue.length };
}
