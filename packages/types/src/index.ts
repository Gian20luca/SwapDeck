/**
 * Tipi condivisi, generati da `swapdeck-core` con ts-rs (`pnpm gen:types`).
 * Qui resta a mano solo la versione dello schema JSON di cswap supportata.
 */
export type * from './generated/index.js';

export const SCHEMA_VERSION = 1 as const;

export type SchemaVersion = typeof SCHEMA_VERSION;
