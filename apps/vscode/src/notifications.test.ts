import type { Notification, NotifyPoll } from '@swapdeck/types';
import { describe, expect, it } from 'vitest';
import { fromPoll, langOf, messageOf, parseEngineEvent, shouldPoll } from './notifications.js';

const note: Notification = {
  kind: 'noReserve',
  severity: 'critical',
  key: 'noreserve',
  title: { it: 'Nessuna riserva disponibile', en: 'No reserve available' },
  body: { it: 'Primo reset alle 10:00.', en: 'Next reset at 10:00.' },
};

describe('notifiche', () => {
  it('riconosce gli eventi del motore e ignora le righe di log', () => {
    const line = JSON.stringify({ event: 'notification', notification: note });
    expect(parseEngineEvent(line)).toEqual(note);
    expect(parseEngineEvent('2026-10-16T08:00:00Z lock del motore acquisito')).toBeNull();
    expect(parseEngineEvent('{non json')).toBeNull();
    expect(parseEngineEvent(JSON.stringify({ event: 'altro' }))).toBeNull();
    expect(
      parseEngineEvent(JSON.stringify({ event: 'notification', notification: { kind: 'x' } })),
    ).toBeNull();
  });

  it('mittente unico: chiede solo senza motore vivo', () => {
    expect(shouldPoll(false, false)).toBe(true);
    expect(shouldPoll(false, undefined)).toBe(true);
    expect(shouldPoll(true, false)).toBe(false);
    expect(shouldPoll(false, true)).toBe(false);
    const poll = (sender: NotifyPoll['sender']): NotifyPoll => ({
      sender,
      notifications: [note],
      language: 'it',
    });
    expect(fromPoll(poll('caller'))).toEqual([note]);
    expect(fromPoll(poll('engine'))).toEqual([]);
    expect(fromPoll(poll('busy'))).toEqual([]);
  });

  it('lingua e livello', () => {
    expect(langOf('it')).toBe('it');
    expect(langOf('IT-ch')).toBe('it');
    expect(langOf('en-US')).toBe('en');
    expect(messageOf(note, 'it')).toEqual({
      level: 'error',
      text: 'SwapDeck: Nessuna riserva disponibile — Primo reset alle 10:00.',
    });
    expect(messageOf({ ...note, severity: 'warning' }, 'en').level).toBe('warning');
    expect(messageOf({ ...note, severity: 'info' }, 'en').level).toBe('info');
  });
});
