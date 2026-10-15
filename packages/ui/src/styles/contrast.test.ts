import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const css = readFileSync(resolve(process.cwd(), 'src/styles/index.css'), 'utf8');

function block(selector: string): Record<string, string> {
  const start = css.indexOf(selector);
  expect(start, selector).toBeGreaterThanOrEqual(0);
  const body = css.slice(css.indexOf('{', start) + 1, css.indexOf('}', start));
  return Object.fromEntries(
    [...body.matchAll(/--(sd-[\w-]+):\s*(#[0-9a-fA-F]{6})/g)].map((m) => [m[1]!, m[2]!]),
  );
}

function luminance(hex: string): number {
  const channels = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const [r, g, b] = channels.map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi! + 0.05) / (lo! + 0.05);
}

const themes = {
  chiaro: block(":root[data-theme='light']"),
  scuro: block(":root[data-theme='dark']"),
};

describe.each(Object.entries(themes))('contrasto WCAG AA, tema %s', (_name, v) => {
  const text = (fg: string, bg: string) => contrast(v[fg]!, v[bg]!);

  it('le variabili esistono', () => {
    for (const key of [
      'bg',
      'surface',
      'surface-2',
      'text',
      'muted',
      'accent',
      'ok',
      'warn',
      'danger',
    ]) {
      expect(v[`sd-${key}`], key).toBeDefined();
    }
  });

  it('testo e testo attenuato (4.5:1) su sfondo, scheda e superfici secondarie', () => {
    for (const bg of ['sd-bg', 'sd-surface', 'sd-surface-2']) {
      expect(text('sd-text', bg), `text/${bg}`).toBeGreaterThanOrEqual(4.5);
      expect(text('sd-muted', bg), `muted/${bg}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('testi di stato (ok, avviso, errore) leggibili su scheda e sulle superfici secondarie', () => {
    for (const fg of ['sd-ok', 'sd-warn', 'sd-danger']) {
      for (const bg of ['sd-surface', 'sd-surface-2']) {
        expect(text(fg, bg), `${fg}/${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it('pulsanti primari e pericolosi: testo sul fondo del pulsante', () => {
    expect(text('sd-accent-contrast', 'sd-accent')).toBeGreaterThanOrEqual(4.5);
    expect(text('sd-accent-contrast', 'sd-danger')).toBeGreaterThanOrEqual(4.5);
  });

  it('elementi grafici (anello di focus, barre) distinguibili: almeno 3:1', () => {
    for (const bg of ['sd-bg', 'sd-surface']) {
      expect(text('sd-focus', bg), `focus/${bg}`).toBeGreaterThanOrEqual(3);
    }
    for (const fill of ['sd-ok-fill', 'sd-warn-fill', 'sd-danger-fill']) {
      expect(text(fill, 'sd-track'), `${fill}/track`).toBeGreaterThanOrEqual(1.5);
      expect(text(fill, 'sd-surface'), `${fill}/surface`).toBeGreaterThanOrEqual(3);
    }
  });
});
