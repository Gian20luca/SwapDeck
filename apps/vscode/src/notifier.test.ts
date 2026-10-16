import { describe, expect, it } from 'vitest';
import { UsageNotifier } from './notifier.js';
import { fixtureList, withActive, withActiveUsage } from './testing.js';

const th = [80, 95];

describe('UsageNotifier', () => {
  it('la prima lettura fa da base, poi avvisa al superamento delle soglie', () => {
    const n = new UsageNotifier();
    expect(n.update(withActiveUsage(fixtureList(), 85, 10), th)).toEqual([]);
    expect(n.update(withActiveUsage(fixtureList(), 90, 10), th)).toEqual([]);
    expect(n.update(withActiveUsage(fixtureList(), 96, 81), th)).toEqual([
      { kind: 'threshold', account: 6, name: 'alias6', window: '5h', threshold: 95, pct: 96 },
      { kind: 'threshold', account: 6, name: 'alias6', window: '7d', threshold: 80, pct: 81 },
    ]);
    expect(n.update(withActiveUsage(fixtureList(), 97, 82), th)).toEqual([]);
  });

  it('sotto le soglie si riarma (reset della finestra)', () => {
    const n = new UsageNotifier();
    n.update(withActiveUsage(fixtureList(), 85, 10), th);
    n.update(withActiveUsage(fixtureList(), 5, 10), th);
    expect(n.update(withActiveUsage(fixtureList(), 82, 10), th)).toHaveLength(1);
  });

  it('cambio di account attivo', () => {
    const n = new UsageNotifier();
    n.update(fixtureList(), th);
    expect(n.update(withActive(fixtureList(), 3), th)).toEqual([
      { kind: 'switch', from: 6, to: 3, name: 'alias3' },
    ]);
    expect(n.update(withActive(fixtureList(), 3), th)).toEqual([]);
  });
});
