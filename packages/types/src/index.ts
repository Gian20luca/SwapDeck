/**
 * Tipi condivisi. Dallo step 1 saranno generati da `swapdeck-core` con ts-rs:
 * qui resta solo la versione del formato dei dati.
 */
export const SCHEMA_VERSION = 1 as const;

export type SchemaVersion = typeof SCHEMA_VERSION;
