import { translate, type Translate } from '@swapdeck/ui';
import { describe, expect, it } from 'vitest';
import { suspensionOf } from '../protocol.js';
import { suspensionLine } from './EngineBanner.js';

const engine = { enabled: true, dryRun: false, pollIntervalSeconds: 60, running: true, lock: null };

describe('riga «motore sospeso»', () => {
  it('dal campo della CLI, solo se c’è', () => {
    expect(suspensionOf(undefined)).toBeNull();
    expect(suspensionOf(engine)).toBeNull();
    expect(suspensionOf({ ...engine, suspended: 'noViableTarget' })).toEqual({
      reason: 'noViableTarget',
      detail: [],
    });
    expect(
      suspensionOf({ ...engine, suspended: 'conflict', suspendDetail: ['schtask:cswap'] }),
    ).toEqual({ reason: 'conflict', detail: ['schtask:cswap'] });
  });

  it('testo con motivo e dettagli, nelle due lingue', () => {
    const it_: Translate = (key, p) => translate('it', key, p);
    const en: Translate = (key, p) => translate('en', key, p);
    expect(suspensionLine({ reason: 'conflict', detail: ['a', 'b'] }, it_)).toBe(
      'Motore sospeso: altro motore attivo (a, b)',
    );
    expect(suspensionLine({ reason: 'invalidRules', detail: [] }, en)).toBe(
      'Engine suspended: invalid rules',
    );
  });
});
