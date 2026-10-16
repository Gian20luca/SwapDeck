// Bundle dell'estensione (Node, CommonJS) e del webview (browser, IIFE) con esbuild, in `out/`.
import { build } from 'esbuild';
import { rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const production = process.argv.includes('--production');
rmSync(new URL('../out', import.meta.url), { recursive: true, force: true });

const common = {
  absWorkingDir: root,
  bundle: true,
  minify: production,
  sourcemap: production ? false : 'linked',
  logLevel: 'warning',
  legalComments: 'none',
};

await Promise.all([
  build({
    ...common,
    entryPoints: { extension: 'src/extension.ts' },
    outdir: 'out',
    platform: 'node',
    format: 'cjs',
    target: 'node20',
    external: ['vscode'],
  }),
  build({
    ...common,
    entryPoints: { main: 'src/webview/main.tsx' },
    outdir: 'out/webview',
    platform: 'browser',
    format: 'iife',
    target: 'es2022',
    jsx: 'automatic',
    define: { 'process.env.NODE_ENV': production ? '"production"' : '"development"' },
  }),
]);
