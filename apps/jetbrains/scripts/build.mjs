// Pannello per JCEF (browser, IIFE) con esbuild, in `out/webview/`: `main.js`, `main.css` e la
// pagina `index.html` (modello con segnaposto, la serve il plugin Kotlin). Gradle lo chiama con
// `--production` (task `buildWebview`) e mette la cartella nelle risorse del plugin (`webview/`).
import { build } from 'esbuild';
import { copyFileSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const production = process.argv.includes('--production');
const outdir = new URL('../out/webview/', import.meta.url);
rmSync(outdir, { recursive: true, force: true });

await build({
  absWorkingDir: root,
  bundle: true,
  minify: production,
  sourcemap: production ? false : 'linked',
  logLevel: 'warning',
  legalComments: 'none',
  entryPoints: { main: 'webview/main.tsx' },
  outdir: 'out/webview',
  platform: 'browser',
  format: 'iife',
  target: 'es2022',
  jsx: 'automatic',
  define: { 'process.env.NODE_ENV': production ? '"production"' : '"development"' },
});
copyFileSync(new URL('../webview/index.html', import.meta.url), new URL('index.html', outdir));
