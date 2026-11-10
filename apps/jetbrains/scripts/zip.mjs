// Aggiunta di file a uno zip esistente, senza dipendenze (serve Node >= 22.2 per `zlib.crc32`).
// Le voci già presenti restano byte per byte; quelle nuove sono compresse (deflate) e portano i
// permessi Unix (`mode`), che l'IDE ripristina estraendo il plugin. Niente zip64: il plugin pesa
// pochi MiB.
import { crc32, deflateRawSync, inflateRawSync } from 'node:zlib';

const EOCD = 0x06054b50;
const CENTRAL = 0x02014b50;
const LOCAL = 0x04034b50;
const UTF8_FLAG = 0x0800;
const MADE_BY_UNIX = 3 << 8;
const VERSION = 20;

/** Fine della directory centrale (cercata dal fondo, dopo un eventuale commento). */
function findEnd(zip) {
  for (let i = zip.length - 22; i >= Math.max(0, zip.length - 22 - 0xffff); i--) {
    if (zip.readUInt32LE(i) === EOCD) {
      const count = zip.readUInt16LE(i + 10);
      const size = zip.readUInt32LE(i + 12);
      const offset = zip.readUInt32LE(i + 16);
      if (count === 0xffff || size === 0xffffffff || offset === 0xffffffff) {
        throw new Error('zip64 non supportato');
      }
      if (offset + size > i) throw new Error('directory centrale fuori dallo zip');
      return { at: i, count, size, offset };
    }
  }
  throw new Error('non è uno zip (fine della directory centrale assente)');
}

/**
 * Voci della directory centrale: nome, metodo, CRC, dimensioni, offset dell'intestazione locale e
 * permessi Unix (`mode`, 0 se lo zip non viene da Unix).
 * @param {Buffer} zip
 */
export function listEntries(zip) {
  const end = findEnd(zip);
  const entries = [];
  let p = end.offset;
  for (let n = 0; n < end.count; n++) {
    if (zip.readUInt32LE(p) !== CENTRAL) throw new Error(`voce ${n} della directory non valida`);
    const madeBy = zip.readUInt16LE(p + 4);
    const nameLen = zip.readUInt16LE(p + 28);
    const extraLen = zip.readUInt16LE(p + 30);
    const commentLen = zip.readUInt16LE(p + 32);
    const external = zip.readUInt32LE(p + 38);
    entries.push({
      name: zip.toString('utf8', p + 46, p + 46 + nameLen),
      method: zip.readUInt16LE(p + 10),
      crc: zip.readUInt32LE(p + 16),
      compressedSize: zip.readUInt32LE(p + 20),
      size: zip.readUInt32LE(p + 24),
      localOffset: zip.readUInt32LE(p + 42),
      mode: madeBy >> 8 === 3 ? (external >>> 16) & 0xffff : 0,
    });
    p += 46 + nameLen + extraLen + commentLen;
  }
  return entries;
}

/**
 * Contenuto di una voce (stored o deflate), con il controllo del CRC.
 * @param {Buffer} zip
 * @param {ReturnType<typeof listEntries>[number]} entry
 */
export function readEntry(zip, entry) {
  const p = entry.localOffset;
  if (zip.readUInt32LE(p) !== LOCAL)
    throw new Error(`${entry.name}: intestazione locale non valida`);
  const start = p + 30 + zip.readUInt16LE(p + 26) + zip.readUInt16LE(p + 28);
  const raw = zip.subarray(start, start + entry.compressedSize);
  let data;
  if (entry.method === 0) data = Buffer.from(raw);
  else if (entry.method === 8) data = inflateRawSync(raw);
  else throw new Error(`${entry.name}: metodo di compressione ${entry.method} non supportato`);
  if (data.length !== entry.size || crc32(data) !== entry.crc) {
    throw new Error(`${entry.name}: contenuto non valido (CRC o dimensione)`);
  }
  return data;
}

function dosDateTime(date) {
  const year = Math.max(1980, date.getFullYear());
  return {
    time: (date.getHours() << 11) | (date.getMinutes() << 5) | (date.getSeconds() >> 1),
    date: ((year - 1980) << 9) | ((date.getMonth() + 1) << 5) | date.getDate(),
  };
}

/**
 * Nuovo zip con `files` aggiunti in fondo a `zip`. Un nome che finisce con `/` è una cartella
 * (`data` ignorato). Un nome già presente è un errore.
 * @param {Buffer} zip
 * @param {{ name: string, data?: Buffer, mode: number }[]} files  `mode`: permessi Unix (es. 0o755)
 * @param {Date} [mtime]
 */
export function appendEntries(zip, files, mtime = new Date()) {
  const end = findEnd(zip);
  const existing = new Set(listEntries(zip).map((e) => e.name));
  const { time, date } = dosDateTime(mtime);
  const locals = [];
  const centrals = [];
  let offset = end.offset;
  for (const file of files) {
    if (existing.has(file.name)) throw new Error(`${file.name}: già nello zip`);
    if (!file.name || file.name.startsWith('/') || file.name.split('/').includes('..')) {
      throw new Error(`nome non valido: ${file.name}`);
    }
    existing.add(file.name);
    const dir = file.name.endsWith('/');
    const data = dir ? Buffer.alloc(0) : (file.data ?? Buffer.alloc(0));
    const compressed = dir ? data : deflateRawSync(data, { level: 9 });
    const method = dir ? 0 : 8;
    const crc = crc32(data);
    const name = Buffer.from(file.name, 'utf8');
    const flags = /[^\x20-\x7e]/.test(file.name) ? UTF8_FLAG : 0;
    const type = dir ? 0o040000 : 0o100000;
    // Attributi MS-DOS nei 16 bit bassi: 0x10 = cartella.
    const external = (((type | (file.mode & 0o7777)) << 16) | (dir ? 0x10 : 0)) >>> 0;

    const local = Buffer.alloc(30);
    local.writeUInt32LE(LOCAL, 0);
    local.writeUInt16LE(VERSION, 4);
    local.writeUInt16LE(flags, 6);
    local.writeUInt16LE(method, 8);
    local.writeUInt16LE(time, 10);
    local.writeUInt16LE(date, 12);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(compressed.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(name.length, 26);
    local.writeUInt16LE(0, 28);

    const central = Buffer.alloc(46);
    central.writeUInt32LE(CENTRAL, 0);
    central.writeUInt16LE(MADE_BY_UNIX | VERSION, 4);
    central.writeUInt16LE(VERSION, 6);
    central.writeUInt16LE(flags, 8);
    central.writeUInt16LE(method, 10);
    central.writeUInt16LE(time, 12);
    central.writeUInt16LE(date, 14);
    central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(compressed.length, 20);
    central.writeUInt32LE(data.length, 24);
    central.writeUInt16LE(name.length, 28);
    central.writeUInt32LE(external, 38);
    central.writeUInt32LE(offset, 42);

    locals.push(local, name, compressed);
    centrals.push(central, name);
    offset += local.length + name.length + compressed.length;
  }
  const oldCentral = zip.subarray(end.offset, end.offset + end.size);
  const newCentral = Buffer.concat([oldCentral, ...centrals]);
  const count = end.count + files.length;
  if (count >= 0xffff || offset >= 0xffffffff) throw new Error('zip troppo grande (zip64)');
  const tail = Buffer.from(zip.subarray(end.at));
  tail.writeUInt16LE(count, 8);
  tail.writeUInt16LE(count, 10);
  tail.writeUInt32LE(newCentral.length, 12);
  tail.writeUInt32LE(offset, 16);
  return Buffer.concat([zip.subarray(0, end.offset), ...locals, newCentral, tail]);
}
