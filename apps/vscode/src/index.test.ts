import { describe, expect, it } from 'vitest';
import { swapdeckExecutable } from './index.js';

describe('swapdeckExecutable', () => {
  it('aggiunge .exe solo su Windows', () => {
    expect(swapdeckExecutable('win32')).toBe('swapdeck.exe');
    expect(swapdeckExecutable('linux')).toBe('swapdeck');
    expect(swapdeckExecutable('darwin')).toBe('swapdeck');
  });
});
