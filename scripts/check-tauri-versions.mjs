// Controlla che i pacchetti npm di Tauri dell'app desktop abbiano lo stesso major.minor dei crate
// in Cargo.lock: `tauri build` si ferma se `@tauri-apps/plugin-X` e `tauri-plugin-X` (o
// `@tauri-apps/api` e `tauri`) divergono. Va lanciato dopo `pnpm install`.
//
//   node scripts/check-tauri-versions.mjs
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const desktop = join(root, 'apps', 'desktop');

// Versioni dei crate da Cargo.lock: nome -> elenco di versioni (più di una è già un problema).
const crates = new Map();
for (const block of readFileSync(join(root, 'Cargo.lock'), 'utf8').split('[[package]]')) {
  const name = /^name = "([^"]+)"$/m.exec(block)?.[1];
  const version = /^version = "([^"]+)"$/m.exec(block)?.[1];
  if (name && version) crates.set(name, [...(crates.get(name) ?? []), version]);
}

const pkg = JSON.parse(readFileSync(join(desktop, 'package.json'), 'utf8'));
const declared = { ...pkg.dependencies, ...pkg.devDependencies };
const crateFor = (npm) =>
  npm === '@tauri-apps/api'
    ? 'tauri'
    : npm.startsWith('@tauri-apps/plugin-')
      ? `tauri-plugin-${npm.slice('@tauri-apps/plugin-'.length)}`
      : null;
const majorMinor = (v) => v.split('.').slice(0, 2).join('.');

const errors = [];
for (const [npm, range] of Object.entries(declared)) {
  const crate = crateFor(npm);
  if (!crate) continue;
  // `^` lascia salire il minor al primo `pnpm update` e rompe di nuovo l'allineamento.
  if (range.startsWith('^')) errors.push(`${npm}: intervallo "${range}", usa la versione esatta o "~"`);
  const versions = crates.get(crate);
  if (!versions) {
    errors.push(`${npm}: manca il crate ${crate} in Cargo.lock`);
    continue;
  }
  if (versions.length > 1) errors.push(`${crate}: più versioni in Cargo.lock (${versions.join(', ')})`);
  let installed;
  try {
    installed = JSON.parse(readFileSync(join(desktop, 'node_modules', npm, 'package.json'), 'utf8')).version;
  } catch {
    errors.push(`${npm}: non installato (lancia prima pnpm install)`);
    continue;
  }
  const ok = versions.every((v) => majorMinor(v) === majorMinor(installed));
  console.log(`${ok ? 'ok ' : 'NO '} ${npm} ${installed} <-> ${crate} ${versions.join(', ')}`);
  if (!ok) errors.push(`${npm} ${installed} e ${crate} ${versions.join(', ')} hanno major.minor diversi`);
}

if (errors.length) {
  for (const e of errors) console.error(`errore: ${e}`);
  process.exit(1);
}
