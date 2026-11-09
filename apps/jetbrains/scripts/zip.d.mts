// Tipi di `zip.mjs` per i test (TypeScript non legge i .mjs senza `allowJs`).
export interface ZipEntry {
  name: string;
  method: number;
  crc: number;
  compressedSize: number;
  size: number;
  localOffset: number;
  mode: number;
}

export interface NewEntry {
  name: string;
  data?: Buffer;
  mode: number;
}

export function listEntries(zip: Buffer): ZipEntry[];
export function readEntry(zip: Buffer, entry: ZipEntry): Buffer;
export function appendEntries(zip: Buffer, files: NewEntry[], mtime?: Date): Buffer;
