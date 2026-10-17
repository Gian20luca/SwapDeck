import type { EngineRules } from '@swapdeck/types';
import { describe, expect, it, vi } from 'vitest';
import { rulesToStart } from './firstEnable.js';

const OFF = {
  enabled: false,
  dryRun: false,
  firstEnableConfirmed: false,
  order: [6, 1],
} as unknown as EngineRules;
const reads = (...list: EngineRules[]) => {
  const read = vi.fn(async () => list.shift() ?? OFF);
  return read;
};

describe('rulesToStart («Avvia motore»)', () => {
  it('prima accensione: chiede; «Accendi» accende e ricorda la conferma, su regole rilette', async () => {
    const ask = vi.fn(async () => 'on' as const);
    const read = reads(OFF, { ...OFF, order: [1, 6] } as EngineRules);
    expect(await rulesToStart(read, ask)).toMatchObject({
      enabled: true,
      dryRun: false,
      firstEnableConfirmed: true,
      order: [1, 6],
    });
    expect(ask).toHaveBeenCalledTimes(1);
    expect(read).toHaveBeenCalledTimes(2);
  });

  it('«Solo osservazione» accende in osservazione', async () => {
    expect(await rulesToStart(reads(), async () => 'observe')).toMatchObject({
      enabled: true,
      dryRun: true,
      firstEnableConfirmed: true,
    });
  });

  it('annullato: nessuna regola da scrivere', async () => {
    expect(await rulesToStart(reads(), async () => undefined)).toBeNull();
  });

  it('già confermata, accesa o in osservazione: parte subito senza chiedere', async () => {
    const ask = vi.fn(async () => undefined);
    for (const rules of [
      { ...OFF, firstEnableConfirmed: true },
      { ...OFF, enabled: true },
      { ...OFF, dryRun: true },
    ]) {
      expect((await rulesToStart(reads(rules), ask))?.enabled).toBe(true);
    }
    expect(ask).not.toHaveBeenCalled();
  });
});
