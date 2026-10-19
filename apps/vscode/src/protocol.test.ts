import { describe, expect, it } from 'vitest';
import { preferredLocaleOf, viewLocale } from './protocol.js';

describe('lingua delle viste', () => {
  it('la scelta di SwapDeck vince su quella di VS Code', () => {
    expect(viewLocale('it', 'en')).toBe('it');
    expect(viewLocale('en', 'it')).toBe('en');
  });

  it('senza scelta segue VS Code', () => {
    expect(viewLocale(undefined, 'it')).toBe('it');
    expect(viewLocale(undefined, 'IT-ch')).toBe('it');
    expect(viewLocale(undefined, 'en-us')).toBe('en');
    expect(viewLocale(undefined, 'de')).toBe('en');
  });

  it('accetta solo le lingue note dalle preferenze', () => {
    expect(preferredLocaleOf('it')).toBe('it');
    expect(preferredLocaleOf('en')).toBe('en');
    expect(preferredLocaleOf('fr')).toBeUndefined();
    expect(preferredLocaleOf(undefined)).toBeUndefined();
  });
});
