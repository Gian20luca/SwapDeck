declare module '*.css';

/** API del webview fornita da VS Code (una sola chiamata per pagina). */
declare function acquireVsCodeApi(): {
  postMessage(message: unknown): void;
};
