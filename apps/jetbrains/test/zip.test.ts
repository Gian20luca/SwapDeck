// Aggiunta di `bin/swapdeck-cli` allo zip del plugin (scripts/zip.mjs, usato da scripts/package.mjs).
import { describe, expect, it } from 'vitest';
import { appendEntries, listEntries, readEntry } from '../scripts/zip.mjs';

/** Zip vuoto: solo la fine della directory centrale, con un commento. */
function emptyZip(comment = ''): Buffer {
  const text = Buffer.from(comment);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(text.length, 20);
  return Buffer.concat([end, text]);
}

describe('zip del plugin', () => {
  const jar = Buffer.from('jar finto '.repeat(100));
  const cli = Buffer.from([
    0x7f,
    0x45,
    0x4c,
    0x46,
    ...Array.from({ length: 300 }, (_, i) => i % 7),
  ]);
  const base = appendEntries(emptyZip('commento'), [
    { name: 'swapdeck-jetbrains/lib/swapdeck-jetbrains-1.0.0.jar', data: jar, mode: 0o644 },
  ]);

  it('aggiunge il binario con 0755 e lascia intatte le voci esistenti', () => {
    const zip = appendEntries(base, [
      { name: 'swapdeck-jetbrains/bin/', mode: 0o755 },
      { name: 'swapdeck-jetbrains/bin/swapdeck-cli', data: cli, mode: 0o755 },
    ]);
    const entries = listEntries(zip);
    expect(entries.map((e) => e.name)).toEqual([
      'swapdeck-jetbrains/lib/swapdeck-jetbrains-1.0.0.jar',
      'swapdeck-jetbrains/bin/',
      'swapdeck-jetbrains/bin/swapdeck-cli',
    ]);
    const [lib, dir, bin] = entries;
    expect(readEntry(zip, lib!).equals(jar)).toBe(true);
    expect(dir!.mode).toBe(0o040755);
    expect(bin!.mode).toBe(0o100755);
    expect(readEntry(zip, bin!).equals(cli)).toBe(true);
    // Il commento finale resta.
    expect(zip.subarray(-8).toString()).toBe('commento');
    // Le voci di prima restano byte per byte all'inizio.
    const start = dir!.localOffset;
    expect(zip.subarray(0, start).equals(base.subarray(0, start))).toBe(true);
  });

  it('rifiuta nomi già presenti o pericolosi', () => {
    const name = 'swapdeck-jetbrains/lib/swapdeck-jetbrains-1.0.0.jar';
    expect(() => appendEntries(base, [{ name, data: cli, mode: 0o755 }])).toThrow(/già/);
    expect(() => appendEntries(base, [{ name: '../x', data: cli, mode: 0o755 }])).toThrow();
    expect(() => appendEntries(base, [{ name: '/x', data: cli, mode: 0o755 }])).toThrow();
  });

  it('rifiuta ciò che non è uno zip e i dati corrotti', () => {
    expect(() => listEntries(Buffer.from('non è uno zip, davvero no'))).toThrow(/non è uno zip/);
    const zip = appendEntries(base, [{ name: 'a', data: cli, mode: 0o755 }]);
    const entry = listEntries(zip).find((e) => e.name === 'a')!;
    const broken = Buffer.from(zip);
    broken.writeUInt8(broken.readUInt8(entry.localOffset + 31) ^ 0xff, entry.localOffset + 31);
    expect(() => readEntry(broken, entry)).toThrow();
  });
});
