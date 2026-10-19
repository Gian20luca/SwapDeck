// Build Linux locale con Docker in `dist-locale/linux/`: .deb, AppImage, swapdeck-cli e .vsix
// linux-x64, come il job Linux della CI. Costruisce l'ultimo commit (non le modifiche non salvate).
//
//   pnpm dist:linux            (oppure: node scripts/dist-linux.mjs)
//
// Serve Docker (Docker Desktop su Windows e macOS). Cache di cargo, target e pnpm nel volume
// `swapdeck-linux-cache`: la prima build scarica tutto, le successive sono più rapide.
import { spawnSync } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(fileURLToPath(new URL('..', import.meta.url)));
const out = join(repo, 'dist-locale', 'linux');
const image = 'swapdeck-linux-build';

function docker(args) {
  const r = spawnSync('docker', args, { stdio: 'inherit' });
  if (r.error) throw new Error(`docker non disponibile: ${r.error.message}`);
  if (r.status !== 0) process.exit(r.status ?? 1);
}

mkdirSync(out, { recursive: true });
docker(['build', '-t', image, join(repo, 'scripts', 'linux')]);
docker([
  'run',
  '--rm',
  '-v',
  `${repo}:/src:ro`,
  '-v',
  'swapdeck-linux-cache:/cache',
  '-v',
  `${out}:/out`,
  image,
]);
console.log(`pacchetti Linux in ${out}`);
