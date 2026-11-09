/**
 * Punto d'ingresso di `@swapdeck/host`: protocollo pannello ↔ host, regole di validazione e segnale
 * di sincronia. Il client dei messaggi per il pannello (`MessageBackend`, che dipende da
 * `@swapdeck/ui`) è in `@swapdeck/host/backend`, così il lato host non carica la UI.
 */
export * from './protocol.js';
export * from './rules.js';
export * from './sync.js';
