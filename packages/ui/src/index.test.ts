import { describe, expect, it } from 'vitest';
import { clampPercent } from './index.js';

describe('clampPercent', () => {
  it('limita i valori a 0..100', () => {
    expect(clampPercent(-5)).toBe(0);
    expect(clampPercent(42.5)).toBe(42.5);
    expect(clampPercent(130)).toBe(100);
  });

  it('restituisce null per valori assenti o non finiti', () => {
    expect(clampPercent(null)).toBeNull();
    expect(clampPercent(undefined)).toBeNull();
    expect(clampPercent(Number.NaN)).toBeNull();
  });
});
