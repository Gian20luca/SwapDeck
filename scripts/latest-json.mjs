// latest.json per tauri-plugin-updater, dai pacchetti firmati raccolti dalla CI di rilascio.
//
//   node scripts/latest-json.mjs --dir assets --version 0.1.0 \
//     --base-url https://github.com/<owner>/<repo>/releases/download/v0.1.0 --out assets/latest.json
//
// Ogni piattaforma vuole il pacchetto e la sua firma `.sig` (testo di minisign) accanto.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { parseArgs } from 'node:util';

const PLATFORMS = [
  ['windows-x86_64', /_x64-setup\.exe$/],
  ['linux-x86_64', /_amd64\.AppImage$/],
  ['darwin-aarch64', /^SwapDeck_aarch64\.app\.tar\.gz$/],
  ['darwin-x86_64', /^SwapDeck_x64\.app\.tar\.gz$/],
];

const { values } = parseArgs({
  options: {
    dir: { type: 'string' },
    version: { type: 'string' },
    'base-url': { type: 'string' },
    out: { type: 'string' },
    notes: { type: 'string', default: '' },
  },
});
for (const k of ['dir', 'version', 'base-url', 'out']) {
  if (!values[k]) {
    console.error(`--${k} obbligatorio`);
    process.exit(2);
  }
}

const files = readdirSync(values.dir);
const platforms = {};
for (const [key, re] of PLATFORMS) {
  const found = files.filter((f) => re.test(f));
  if (found.length === 0) {
    console.warn(`::warning::${key}: pacchetto dell'updater assente`);
    continue;
  }
  if (found.length > 1) {
    console.error(`${key}: più pacchetti (${found.join(', ')})`);
    process.exit(1);
  }
  const [file] = found;
  if (!files.includes(`${file}.sig`)) {
    console.error(`${key}: firma ${file}.sig assente`);
    process.exit(1);
  }
  platforms[key] = {
    signature: readFileSync(join(values.dir, `${file}.sig`), 'utf8').trim(),
    url: `${values['base-url']}/${encodeURIComponent(file)}`,
  };
}
if (Object.keys(platforms).length === 0) {
  console.error('nessun pacchetto dell’updater trovato');
  process.exit(1);
}

const manifest = {
  version: values.version,
  notes: values.notes,
  pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
  platforms,
};
writeFileSync(values.out, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`${values.out}: ${Object.keys(platforms).join(', ')}`);
