// Pacchetto .vsix per piattaforma, con `swapdeck-cli` incluso in `bin/`.
//
//   node scripts/package.mjs --target win32-x64                    (build locale del binario)
//   node scripts/package.mjs --target linux-x64 --rust-target x86_64-unknown-linux-gnu
//   node scripts/package.mjs --target darwin-arm64 --cli <percorso del binario già costruito>
//
// Target VS Code ↔ Rust (per la CI dello step 8): win32-x64 ↔ x86_64-pc-windows-msvc,
// linux-x64 ↔ x86_64-unknown-linux-gnu, darwin-arm64 ↔ aarch64-apple-darwin,
// darwin-x64 ↔ x86_64-apple-darwin. Senza `--rust-target` si usa l'host.
// Con il toolchain gnullvm (sviluppo su Windows senza MSVC) si include anche `libunwind.dll`.
import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { createRequire } from 'node:module';
import { delimiter, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const TARGETS = [
  'win32-x64',
  'win32-arm64',
  'linux-x64',
  'linux-arm64',
  'darwin-x64',
  'darwin-arm64',
];

const { values } = parseArgs({
  options: {
    target: { type: 'string' },
    'rust-target': { type: 'string' },
    cli: { type: 'string' },
  },
});
const target = values.target;
if (!target || !TARGETS.includes(target)) {
  console.error(`--target obbligatorio, uno di: ${TARGETS.join(', ')}`);
  process.exit(2);
}

const ext = fileURLToPath(new URL('..', import.meta.url));
const repo = join(ext, '..', '..');
const windows = target.startsWith('win32');
const exe = windows ? 'swapdeck-cli.exe' : 'swapdeck-cli';

function run(cmd, args, cwd) {
  const r = spawnSync(cmd, args, { cwd, stdio: 'inherit' });
  if (r.error) throw r.error;
  if (r.status !== 0) process.exit(r.status ?? 1);
}

run(process.execPath, [join(ext, 'scripts', 'build.mjs'), '--production'], ext);

let cli = values.cli;
let gnullvm = false;
if (!cli) {
  const rustTarget = values['rust-target'];
  const args = ['build', '--release', '-p', 'swapdeck-cli', '--bin', 'swapdeck-cli'];
  if (rustTarget) args.push('--target', rustTarget);
  run('cargo', args, repo);
  const targetDir = process.env.CARGO_TARGET_DIR ?? join(repo, 'target');
  cli = join(targetDir, ...(rustTarget ? [rustTarget] : []), 'release', exe);
  const host = /host: (\S+)/.exec(execFileSync('rustc', ['-vV'], { encoding: 'utf8' }))?.[1] ?? '';
  gnullvm = (rustTarget ?? host).endsWith('-gnullvm');
}
if (!existsSync(cli) || !statSync(cli).isFile()) {
  console.error(`binario non trovato: ${cli}`);
  process.exit(1);
}

const bin = join(ext, 'bin');
rmSync(bin, { recursive: true, force: true });
mkdirSync(bin);
copyFileSync(cli, join(bin, exe));
if (gnullvm && windows) {
  // Il binario gnullvm dipende da libunwind.dll di llvm-mingw: la si prende dal PATH.
  const dir = (process.env.PATH ?? '')
    .split(delimiter)
    .find((d) => d && existsSync(join(d, 'libunwind.dll')));
  if (!dir) {
    console.error('libunwind.dll non trovata nel PATH (llvm-mingw): vedi apps/desktop/README.md');
    process.exit(1);
  }
  copyFileSync(join(dir, 'libunwind.dll'), join(bin, 'libunwind.dll'));
}

const require = createRequire(import.meta.url);
const vscePkg = require.resolve('@vscode/vsce/package.json');
const vsceBin = join(dirname(vscePkg), JSON.parse(readFileSync(vscePkg, 'utf8')).bin.vsce);
const { version } = JSON.parse(readFileSync(join(ext, 'package.json'), 'utf8'));
const out = join(ext, 'dist', `swapdeck-vscode-${target}-${version}.vsix`);
mkdirSync(dirname(out), { recursive: true });
run(
  process.execPath,
  [
    vsceBin,
    'package',
    '--target',
    target,
    '--no-dependencies',
    '--skip-license',
    '--allow-missing-repository',
    '--out',
    out,
  ],
  ext,
);
console.log(`pacchetto: ${out} (${Math.round(statSync(out).size / 1024)} KiB)`);
