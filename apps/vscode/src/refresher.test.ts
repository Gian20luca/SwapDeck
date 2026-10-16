import { describe, expect, it, vi } from 'vitest';
import { Refresher } from './refresher.js';

function deferredReads() {
  const resolvers: Array<(v: string) => void> = [];
  const read = vi.fn(
    () =>
      new Promise<string>((resolve) => {
        resolvers.push(resolve);
      }),
  );
  return { read, resolve: (i: number, v: string) => resolvers[i]!(v) };
}

describe('Refresher', () => {
  it('le chiamate concorrenti condividono una sola lettura', async () => {
    const { read, resolve } = deferredReads();
    const accept = vi.fn((v: string) => v);
    const r = new Refresher(read, accept);
    const a = r.refresh();
    const b = r.refresh();
    expect(a).toBe(b);
    resolve(0, 'uno');
    await expect(a).resolves.toBe('uno');
    expect(read).toHaveBeenCalledOnce();
    expect(accept).toHaveBeenCalledWith('uno', true);
    void r.refresh();
    expect(read).toHaveBeenCalledTimes(2);
  });

  it('una lettura vecchia dopo invalidate non è attuale e non blocca quella nuova', async () => {
    const { read, resolve } = deferredReads();
    let state = 'iniziale';
    const r = new Refresher(read, (v, current) => {
      if (current) state = v;
      return state;
    });
    const old = r.refresh();
    r.invalidate();
    state = 'dopo azione';
    const fresh = r.refresh();
    expect(fresh).not.toBe(old);
    resolve(0, 'vecchio');
    await expect(old).resolves.toBe('dopo azione');
    expect(state).toBe('dopo azione');
    resolve(1, 'nuovo');
    await expect(fresh).resolves.toBe('nuovo');
    // la vecchia, finendo, non ha azzerato la lettura in corso
    void r.refresh();
    expect(read).toHaveBeenCalledTimes(3);
  });
});
