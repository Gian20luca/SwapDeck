import { describe, expect, it } from 'vitest';
import { SCHEMA_VERSION } from './index.js';

describe('types', () => {
  it('espone la versione del formato', () => {
    expect(SCHEMA_VERSION).toBe(1);
  });
});
