declare module '*.css';

/** API del webview fornita da VS Code (una sola chiamata per pagina). */
declare function acquireVsCodeApi(): {
  postMessage(message: unknown): void;
  /** Stato del webview: sopravvive quando la vista è nascosta e alla riapertura. */
  getState(): unknown;
  setState(state: unknown): void;
};
