import { readFileSync, readdirSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const root = new URL('../', import.meta.url);
const read = (rel: string) => readFileSync(new URL(rel, root), 'utf8');

/** Stringhe passate a `t(...)` nei sorgenti dell'host (anche su più righe). */
function sourceStrings(): Set<string> {
  const out = new Set<string>();
  for (const file of readdirSync(new URL('src/', root))) {
    if (!file.endsWith('.ts') || file.endsWith('.test.ts') || file === 'testing.ts') continue;
    for (const m of read(`src/${file}`).matchAll(/\bt\(\s*'((?:[^'\\]|\\.)*)'/g)) out.add(m[1]!);
  }
  return out;
}

describe('l10n', () => {
  it("ogni testo dell'host ha la traduzione italiana e viceversa", () => {
    const itBundle = JSON.parse(read('l10n/bundle.l10n.it.json')) as Record<string, string>;
    const used = sourceStrings();
    expect([...used].filter((s) => !(s in itBundle))).toEqual([]);
    expect(Object.keys(itBundle).filter((k) => !used.has(k))).toEqual([]);
  });

  it('package.nls in inglese e italiano con le stesse chiavi, tutte usate', () => {
    const en = JSON.parse(read('package.nls.json')) as Record<string, string>;
    const itNls = JSON.parse(read('package.nls.it.json')) as Record<string, string>;
    expect(Object.keys(itNls).sort()).toEqual(Object.keys(en).sort());
    const pkg = read('package.json');
    const used = [...pkg.matchAll(/"%([^%]+)%"/g)].map((m) => m[1]!);
    expect([...new Set(used)].sort()).toEqual(Object.keys(en).sort());
  });
});
