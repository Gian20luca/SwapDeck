/**
 * `@swapdeck/host/webview`: pezzi del pannello comuni agli IDE, sopra `MessageBackend`. L'app
 * (`HostApp`), la riga «motore sospeso» (`EngineBanner`) e lo stato ricordato (`readState`). Il
 * trasporto dei messaggi e dove si salva lo stato restano all'host. Stile in
 * `@swapdeck/host/webview.css`.
 */
export { EngineBanner, suspensionLine } from './EngineBanner.js';
export { HostApp, type HostAppProps } from './HostApp.js';
export { readState, type WebviewState } from './viewState.js';
