declare module '*.css';

interface Window {
  /** Funzione del plugin Kotlin (`JBCefJSQuery`), iniettata dopo il caricamento: vedi `transport.ts`. */
  __swapdeckHost?: import('./transport.js').HostBridge | undefined;
}
