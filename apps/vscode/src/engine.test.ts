import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll, describe, expect, it, vi } from 'vitest';
import {
  ENGINE_ARGS,
  EngineProcess,
  LineSplitter,
  LOCK_BUSY_EXIT,
  shouldRetryEngine,
} from './engine.js';
import { preloadEnv, PROCESS_TEST_TIMEOUT_MS } from './testing.js';

describe('LineSplitter', () => {
  it('ricompone righe e caratteri UTF-8 spezzati tra i blocchi', () => {
    const lines: string[] = [];
    const s = new LineSplitter((l) => lines.push(l));
    const bytes = Buffer.from('prima riga\r\nmotore già attivo\n\nultima');
    const cut = bytes.indexOf(Buffer.from('à')) + 1; // a metà del carattere
    s.push(bytes.subarray(0, 5));
    s.push(bytes.subarray(5, cut));
    expect(lines).toEqual(['prima riga']);
    s.push(bytes.subarray(cut));
    expect(lines).toEqual(['prima riga', 'motore già attivo']);
    s.end();
    expect(lines).toEqual(['prima riga', 'motore già attivo', 'ultima']);
  });

  it('tronca le righe troppo lunghe', () => {
    const lines: string[] = [];
    const s = new LineSplitter((l) => lines.push(l));
    for (let i = 0; i < 10; i++) s.push('x'.repeat(1000));
    s.push('\nok\n');
    expect(lines[0]).toHaveLength(4097);
    expect(lines[0]!.endsWith('…')).toBe(true);
    expect(lines[1]).toBe('ok');
  });
});

describe('ENGINE_ARGS', () => {
  it('chiede le notifiche su stdout: le mostra l’estensione, senza doppioni dal sistema', () => {
    expect(ENGINE_ARGS).toEqual(['headless', '--stop-on-stdin-eof', '--notify=stdout']);
  });
});

describe('shouldRetryEngine', () => {
  const base = { enabled: true, localRunning: false, waitingLock: true, lockHeldElsewhere: false };
  it('ritenta solo se il lock si è liberato', () => {
    expect(shouldRetryEngine(base)).toBe(true);
    expect(shouldRetryEngine({ ...base, lockHeldElsewhere: true })).toBe(false);
    expect(shouldRetryEngine({ ...base, lockHeldElsewhere: undefined })).toBe(false);
    expect(shouldRetryEngine({ ...base, enabled: false })).toBe(false);
    expect(shouldRetryEngine({ ...base, localRunning: true })).toBe(false);
    expect(shouldRetryEngine({ ...base, waitingLock: false })).toBe(false);
  });
});

describe('EngineProcess', { timeout: PROCESS_TEST_TIMEOUT_MS }, () => {
  const dir = mkdtempSync(join(tmpdir(), 'swapdeck-engine-'));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  /** `node headless ...` con uno script precaricato che decide l'uscita. */
  const fakeEnv = (code: string) => preloadEnv(dir, code);
  const waitExit = (engine: EngineProcess) =>
    vi.waitFor(() => expect(engine.running).toBe(false), { timeout: 10_000 });

  it('uscita LOCK_BUSY: onLockBusy, nessun avviso', async () => {
    const events = { onLine: vi.fn(), onUnexpectedExit: vi.fn(), onLockBusy: vi.fn() };
    const engine = new EngineProcess(events);
    engine.start(
      process.execPath,
      fakeEnv(`process.stderr.write('motore già attivo\\n'); process.exit(${LOCK_BUSY_EXIT});`),
    );
    expect(engine.running).toBe(true);
    await waitExit(engine);
    await vi.waitFor(() => expect(events.onLockBusy).toHaveBeenCalledOnce());
    expect(events.onUnexpectedExit).not.toHaveBeenCalled();
    expect(events.onLine).toHaveBeenCalledWith('motore già attivo');
  });

  it('altra uscita non chiesta: onUnexpectedExit con il codice', async () => {
    const events = { onLine: vi.fn(), onUnexpectedExit: vi.fn() };
    const engine = new EngineProcess(events);
    engine.start(process.execPath, fakeEnv('process.exit(2);'));
    await waitExit(engine);
    await vi.waitFor(() => expect(events.onUnexpectedExit).toHaveBeenCalledWith(2));
  });

  it('stop chiude stdin e non segnala uscite inattese', async () => {
    const events = { onLine: vi.fn(), onUnexpectedExit: vi.fn(), onLockBusy: vi.fn() };
    const engine = new EngineProcess(events);
    engine.start(
      process.execPath,
      fakeEnv("process.stdin.resume(); process.stdin.on('end', () => process.exit(0));"),
    );
    await engine.stop(5000);
    expect(engine.running).toBe(false);
    expect(events.onUnexpectedExit).not.toHaveBeenCalled();
    await engine.stop();
  });
});
