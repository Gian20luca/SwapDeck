import { EventEmitter } from 'node:events';
import type * as fs from 'node:fs';
import {
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { CONFIG_FILE, SYNC_FILE, SyncWatcher, isSyncEvent } from './sync.js';

let dir: string;
const open: SyncWatcher[] = [];

beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'sd-sync-'));
});
afterEach(() => {
  for (const w of open.splice(0)) w.dispose();
  rmSync(dir, { recursive: true, force: true });
});

/** Una «finestra» di VS Code: un watcher sulla stessa cartella con il suo contatore. */
function window(name: string) {
  const onChange = vi.fn();
  const w = new SyncWatcher({ dir, onChange, debounceMs: 60, source: name });
  open.push(w);
  expect(w.start()).toBe(true);
  return { w, onChange };
}

const settle = () => new Promise((r) => setTimeout(r, 400));

describe('isSyncEvent', () => {
  it('solo config e segnale; senza nome per prudenza sì', () => {
    expect(isSyncEvent(CONFIG_FILE)).toBe(true);
    expect(isSyncEvent(SYNC_FILE)).toBe(true);
    expect(isSyncEvent(null)).toBe(true);
    expect(isSyncEvent('engine.lock')).toBe(false);
    expect(isSyncEvent('engine-memory.json')).toBe(false);
    expect(isSyncEvent(`.${SYNC_FILE}.abc.tmp`)).toBe(false);
  });
});

describe('SyncWatcher fra due finestre', () => {
  it('il segnale di una finestra fa rileggere l’altra, non sé stessa', async () => {
    const a = window('A');
    const b = window('B');
    await a.w.signal();
    await settle();
    expect(b.onChange).toHaveBeenCalledTimes(1);
    expect(a.onChange).not.toHaveBeenCalled();
    const body = JSON.parse(readFileSync(join(dir, SYNC_FILE), 'utf8')) as Record<string, unknown>;
    expect(Object.keys(body).sort()).toEqual(['at', 'source']);
    expect(body['source']).toBe('A');
    // Nessun file temporaneo lasciato indietro.
    expect(readdirSync(dir)).toEqual([SYNC_FILE]);
  });

  it('una modifica della config (CLI di qualunque finestra) fa rileggere tutte, raggruppata', async () => {
    const a = window('A');
    const b = window('B');
    for (let i = 0; i < 5; i++) writeFileSync(join(dir, CONFIG_FILE), `{"n":${i}}`);
    await settle();
    expect(a.onChange).toHaveBeenCalledTimes(1);
    expect(b.onChange).toHaveBeenCalledTimes(1);
  });

  it('i file del motore (lock, memoria) non fanno rileggere', async () => {
    const a = window('A');
    writeFileSync(join(dir, 'engine.lock'), '{}');
    writeFileSync(join(dir, 'engine-memory.json'), '{}');
    await settle();
    expect(a.onChange).not.toHaveBeenCalled();
  });

  it('un segnale che è un link non viene seguito: si rilegge e la rinomina lo sostituisce', async () => {
    const outside = join(dir, 'outside.json');
    writeFileSync(outside, JSON.stringify({ source: 'B' }));
    try {
      symlinkSync(outside, join(dir, SYNC_FILE));
    } catch {
      return; // link simbolici non permessi (Windows senza modalità sviluppatore)
    }
    const b = window('B');
    await b.w.signal();
    expect(readFileSync(outside, 'utf8')).toBe(JSON.stringify({ source: 'B' }));
  });
});

describe('SyncWatcher senza osservazione', () => {
  it('cartella assente: non parte, il segnale non scrive e non lancia', async () => {
    const log = vi.fn();
    const w = new SyncWatcher({ dir: join(dir, 'manca'), onChange: vi.fn(), log });
    open.push(w);
    expect(w.start()).toBe(false);
    expect(w.watching).toBe(false);
    await expect(w.signal()).resolves.toBeUndefined();
    expect(log).not.toHaveBeenCalled();
  });

  it('errore del watcher: si ferma, resta il polling, si può ripartire', () => {
    const fake = new EventEmitter() as EventEmitter & { close: () => void };
    fake.close = vi.fn();
    const watch = vi.fn(() => fake as unknown as fs.FSWatcher) as unknown as typeof fs.watch;
    const log = vi.fn();
    const w = new SyncWatcher({ dir, onChange: vi.fn(), watch, log });
    expect(w.start()).toBe(true);
    fake.emit('error', new Error('EPERM'));
    expect(w.watching).toBe(false);
    expect(fake.close).toHaveBeenCalled();
    expect(log).toHaveBeenCalledWith(expect.stringContaining('resta il polling'));
    expect(w.start()).toBe(true);
    w.dispose();
    expect(w.start()).toBe(false);
  });
});
