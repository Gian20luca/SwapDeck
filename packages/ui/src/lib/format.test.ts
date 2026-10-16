import { describe, expect, it } from 'vitest';
import {
  countdownTo,
  displayPct,
  formatAge,
  formatClock,
  formatWhen,
  formatCountdown,
  isStale,
  levelFor,
  shortEmail,
} from './format.js';

describe('levelFor', () => {
  it('usa le soglie 80 e 95', () => {
    expect(levelFor(0)).toBe('ok');
    expect(levelFor(79.9)).toBe('ok');
    expect(levelFor(80)).toBe('warn');
    expect(levelFor(94.9)).toBe('warn');
    expect(levelFor(95)).toBe('danger');
    expect(levelFor(100)).toBe('danger');
  });
  it('restituisce unknown per valori assenti', () => {
    expect(levelFor(null)).toBe('unknown');
    expect(levelFor(undefined)).toBe('unknown');
    expect(levelFor(Number.NaN)).toBe('unknown');
  });
  it('accetta soglie personalizzate', () => {
    expect(levelFor(60, { warn: 50, danger: 70 })).toBe('warn');
  });
});

describe('displayPct', () => {
  it('arrotonda e limita a 0..100', () => {
    expect(displayPct(47.4)).toBe(47);
    expect(displayPct(-3)).toBe(0);
    expect(displayPct(140)).toBe(100);
  });
});

describe('shortEmail', () => {
  it('lascia invariate le email corte', () => {
    expect(shortEmail('user1@example.com')).toBe('user1@example.com');
  });
  it('accorcia la parte locale e mantiene il dominio', () => {
    const out = shortEmail('un.nome.utente.molto.lungo@example.com', 24);
    expect(out.endsWith('@example.com')).toBe(true);
    expect(out).toContain('…');
    expect(out.length).toBeLessThanOrEqual(24);
  });
  it('tronca se manca la chiocciola', () => {
    expect(shortEmail('abcdefghijklmnopqrstuvwxyz0123', 10)).toBe('abcdefghi…');
  });
  it('accorcia anche un dominio molto lungo', () => {
    const out = shortEmail('a@un-dominio-lunghissimo-davvero.example.com', 20);
    expect(out).toContain('…');
  });
});

describe('formatCountdown', () => {
  const min = 60_000;
  it('formatta giorni, ore e minuti', () => {
    expect(formatCountdown((3 * 1440 + 21 * 60 + 5) * min)).toBe('3d 21h');
    expect(formatCountdown((60 + 57) * min)).toBe('1h 57m');
    expect(formatCountdown(12 * min)).toBe('12m');
  });
  it('gestisce zero, negativi e non finiti', () => {
    expect(formatCountdown(0)).toBe('0m');
    expect(formatCountdown(-5)).toBe('0m');
    expect(formatCountdown(Number.NaN)).toBe('0m');
  });
});

describe('countdownTo', () => {
  it('calcola dal resetsAt ISO', () => {
    const now = Date.parse('2026-10-15T20:00:00Z');
    expect(countdownTo('2026-10-15T21:57:59.9+00:00', now)).toBe('1h 57m');
  });
  it('restituisce null se assente o non valido', () => {
    expect(countdownTo(undefined, 0)).toBeNull();
    expect(countdownTo('boh', 0)).toBeNull();
  });
});

describe('formatAge e isStale', () => {
  it('formatta le età', () => {
    expect(formatAge(5)).toBe('<1m');
    expect(formatAge(153.2)).toBe('2m');
    expect(formatAge(7200)).toBe('2h');
    expect(formatAge(789_343)).toBe('9d');
  });
  it('segnala il dato vecchio oltre 600 s', () => {
    expect(isStale(599)).toBe(false);
    expect(isStale(601)).toBe(true);
    expect(isStale(null)).toBe(false);
  });
});

describe('formatClock', () => {
  it('ore, minuti e secondi nel fuso locale', () => {
    const d = new Date(2026, 9, 16, 9, 42, 15);
    expect(formatClock(d.toISOString())).toBe('09:42:15');
    expect(formatClock(new Date(2026, 9, 16, 0, 5, 3).toISOString())).toBe('00:05:03');
  });
  it('dato assente o non valido', () => {
    expect(formatClock(undefined)).toBeNull();
    expect(formatClock('')).toBeNull();
    expect(formatClock('non è una data')).toBeNull();
  });
});

describe('formatWhen', () => {
  const words = { today: 'oggi', tomorrow: 'domani' };
  const rome = 'Europe/Rome'; // UTC+2 il 16 ottobre 2026
  const now = Date.parse('2026-10-16T10:00:00Z');

  it('oggi: solo «oggi» e l’ora', () => {
    expect(formatWhen('2026-10-16T13:12:00Z', now, 'it', words, rome)).toBe('oggi 15:12');
  });
  it('domani', () => {
    expect(formatWhen('2026-10-17T07:00:00Z', now, 'it', words, rome)).toBe('domani 09:00');
  });
  it('un altro giorno: giorno della settimana, data e ora', () => {
    expect(formatWhen('2026-10-19T16:59:00Z', now, 'it', words, rome)).toBe('lun 19/10 18:59');
    expect(
      formatWhen('2026-10-16T10:00:00Z', Date.parse('2026-10-20T10:00:00Z'), 'it', words, rome),
    ).toBe('ven 16/10 12:00');
  });
  it('in inglese data nell’ordine del mese e 24 ore', () => {
    const en = { today: 'today', tomorrow: 'tomorrow' };
    expect(formatWhen('2026-10-19T13:12:00Z', now, 'en', en, rome)).toBe('Mon 10/19 15:12');
    expect(formatWhen('2026-10-16T13:12:00Z', now, 'en', en, rome)).toBe('today 15:12');
  });
  it('il fuso decide che giorno è: stesso istante, giorni diversi', () => {
    const late = Date.parse('2026-10-16T22:30:00Z'); // a Roma è già il 17, a New York ancora il 16
    const target = '2026-10-17T10:00:00Z';
    expect(formatWhen(target, late, 'it', words, rome)).toBe('oggi 12:00');
    expect(formatWhen(target, late, 'it', words, 'America/New_York')).toBe('domani 06:00');
  });
  it('dato assente o non valido', () => {
    expect(formatWhen(null, now, 'it', words)).toBeNull();
    expect(formatWhen('x', now, 'it', words)).toBeNull();
  });
});
