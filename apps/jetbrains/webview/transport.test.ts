import type { HostMessage } from '@swapdeck/host';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  BRIDGE_POLL_MS,
  HOST_MESSAGE_EVENT,
  HOST_READY_EVENT,
  createJcefTransport,
  hostMessageOf,
  type HostBridge,
  type TransportWindow,
} from './transport.js';

class FakeWindow extends EventTarget implements TransportWindow {
  __swapdeckHost?: HostBridge | undefined;
  setInterval(handler: () => void, ms: number): number {
    return setInterval(handler, ms) as unknown as number;
  }
  clearInterval(id: number): void {
    clearInterval(id);
  }
}

function inject(win: FakeWindow): string[] {
  const sent: string[] = [];
  win.__swapdeckHost = { post: (json) => sent.push(json) };
  return sent;
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('trasporto JCEF', () => {
  it('con la funzione già iniettata manda subito il JSON', () => {
    const win = new FakeWindow();
    const sent = inject(win);
    createJcefTransport(win).post({ type: 'ready' });
    expect(sent).toEqual(['{"type":"ready"}']);
  });

  it('senza funzione tiene in coda e consegna in ordine all’evento di pronto', () => {
    const win = new FakeWindow();
    const t = createJcefTransport(win);
    t.post({ type: 'ready' });
    t.post({ type: 'request', id: 1, method: 'getStatus', params: {} } as never);
    expect(t.pending()).toBe(2);
    const sent = inject(win);
    win.dispatchEvent(new Event(HOST_READY_EVENT));
    expect(sent.map((s) => JSON.parse(s).type)).toEqual(['ready', 'request']);
    expect(t.pending()).toBe(0);
    t.post({ type: 'ready' });
    expect(sent).toHaveLength(3);
  });

  it('senza evento di pronto trova la funzione con il controllo periodico', () => {
    const win = new FakeWindow();
    const t = createJcefTransport(win);
    t.post({ type: 'ready' });
    vi.advanceTimersByTime(BRIDGE_POLL_MS * 3);
    expect(t.pending()).toBe(1);
    const sent = inject(win);
    vi.advanceTimersByTime(BRIDGE_POLL_MS);
    expect(sent).toEqual(['{"type":"ready"}']);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('consegna i messaggi dell’host, in JSON o come oggetto, e ignora il resto', () => {
    const win = new FakeWindow();
    const got: HostMessage[] = [];
    const off = createJcefTransport(win).subscribe((m) => got.push(m));
    const send = (detail: unknown) =>
      win.dispatchEvent(new CustomEvent(HOST_MESSAGE_EVENT, { detail }));
    send('{"type":"configChanged"}');
    send({ type: 'view', settings: { locale: 'it', theme: 'dark' } });
    send('non json');
    send(42);
    send({ nope: true });
    send(null);
    expect(got.map((m) => m.type)).toEqual(['configChanged', 'view']);
    off();
    send('{"type":"configChanged"}');
    expect(got).toHaveLength(2);
  });

  it('hostMessageOf vuole un oggetto con type stringa', () => {
    expect(hostMessageOf('{"type":"x"}')).toEqual({ type: 'x' });
    expect(hostMessageOf('[1]')).toBeUndefined();
    expect(hostMessageOf({ type: 1 })).toBeUndefined();
    expect(hostMessageOf(undefined)).toBeUndefined();
  });
});
