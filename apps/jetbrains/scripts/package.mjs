// Zip del plugin per piattaforma, con `swapdeck-cli` incluso in `swapdeck-jetbrains/bin/`
// (`CliPaths.bundled`). Serve un JDK 21 per Gradle e `pnpm install` fatto.
//
//   node scripts/package.mjs --target linux-x64 --cli <percorso del binario già costruito>
//   node scripts/package.mjs                       (sistema corrente, build locale del binario)
//   node scripts/package.mjs --target linux-x64 --rust-target x86_64-unknown-linux-gnu
//
// Target ↔ Rust: win32-x64 ↔ x86_64-pc-windows-msvc, linux-x64 ↔ x86_64-unknown-linux-gnu,
// darwin-arm64 ↔ aarch64-apple-darwin, darwin-x64 ↔ x86_64-apple-darwin. Senza `--cli` e senza
// `--rust-target` si compila per l'host. Con il toolchain gnullvm (sviluppo su Windows senza MSVC)
// si include anche `libunwind.dll`.
//
// Uscita: `dist/swapdeck-jetbrains-<target>-<versione>.zip` = lo zip di `buildPlugin` (pannello di
// produzione) più `bin/swapdeck-cli[.exe]` con permessi 0755.
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { delimiter, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { appendEntries, listEntries, readEntry } from './zip.mjs';

const TARGETS = ['linux-x64', 'win32-x64', 'darwin-arm64', 'darwin-x64'];
const PLUGIN = 'swapdeck-jetbrains';

const { values } = parseArgs({
  options: {
    target: { type: 'string' },
    'rust-target': { type: 'string' },
    cli: { type: 'string' },
  },
});
// Senza `--target`: il sistema su cui si lancia (`pnpm jetbrains:package`).
const target = values.target ?? `${process.platform}-${process.arch}`;
if (!TARGETS.includes(target)) {
  console.error(`--target non valido (${target}), uno di: ${TARGETS.join(', ')}`);
  process.exit(2);
}

const app = fileURLToPath(new URL('..', import.meta.url));
const repo = join(app, '..', '..');
const windows = target.startsWith('win32');
const exe = windows ? 'swapdeck-cli.exe' : 'swapdeck-cli';
const winHost = process.platform === 'win32';

function run(cmd, args, cwd, shell = false) {
  const r = spawnSync(cmd, args, { cwd, stdio: 'inherit', shell });
  if (r.error) throw r.error;
  if (r.status !== 0) process.exit(r.status ?? 1);
}

function rustHost() {
  try {
    return /host: (\S+)/.exec(execFileSync('rustc', ['-vV'], { encoding: 'utf8' }))?.[1] ?? '';
  } catch {
    return '';
  }
}

const version = /^pluginVersion\s*=\s*(\S+)\s*$/m.exec(
  readFileSync(join(app, 'gradle.properties'), 'utf8'),
)?.[1];
if (!version) {
  console.error('pluginVersion non trovata in gradle.properties');
  process.exit(1);
}

// 1. swapdeck-cli.
let cli = values.cli;
const rustTarget = values['rust-target'];
if (!cli) {
  if (!rustTarget && target !== `${process.platform}-${process.arch}`) {
    console.error(`per ${target} da questo sistema servono --cli o --rust-target`);
    process.exit(2);
  }
  const args = ['build', '--release', '--locked', '-p', 'swapdeck-cli', '--bin', 'swapdeck-cli'];
  if (rustTarget) args.push('--target', rustTarget);
  run('cargo', args, repo);
  const targetDir = process.env.CARGO_TARGET_DIR ?? join(repo, 'target');
  cli = join(targetDir, ...(rustTarget ? [rustTarget] : []), 'release', exe);
}
if (!existsSync(cli) || !statSync(cli).isFile()) {
  console.error(`binario non trovato: ${cli}`);
  process.exit(1);
}
// Il binario gnullvm dipende da libunwind.dll di llvm-mingw: la si prende dal PATH. Vale anche con
// `--cli` (dist:local passa il binario già costruito con il toolchain dell'host).
const gnullvm = windows && (rustTarget ?? (winHost ? rustHost() : '')).endsWith('-gnullvm');
let unwind;
if (gnullvm) {
  const dir = (process.env.PATH ?? '')
    .split(delimiter)
    .find((d) => d && existsSync(join(d, 'libunwind.dll')));
  if (!dir) {
    console.error('libunwind.dll non trovata nel PATH (llvm-mingw): vedi apps/desktop/README.md');
    process.exit(1);
  }
  unwind = join(dir, 'libunwind.dll');
}

// 2. Plugin (pannello di produzione dentro `buildWebview`).
run(winHost ? 'gradlew.bat' : './gradlew', ['--no-daemon', 'buildPlugin'], app, winHost);
const built = join(app, 'build', 'distributions', `${PLUGIN}-${version}.zip`);
if (!existsSync(built)) {
  console.error(`zip di buildPlugin non trovato: ${built}`);
  process.exit(1);
}

// 3. Zip per piattaforma.
const base = readFileSync(built);
const names = new Set(listEntries(base).map((e) => e.name));
const files = [];
if (!names.has(`${PLUGIN}/bin/`)) files.push({ name: `${PLUGIN}/bin/`, mode: 0o755 });
files.push({ name: `${PLUGIN}/bin/${exe}`, data: readFileSync(cli), mode: 0o755 });
if (unwind) {
  files.push({ name: `${PLUGIN}/bin/libunwind.dll`, data: readFileSync(unwind), mode: 0o644 });
}
// Stessa data delle voci di Gradle (archivi riproducibili): 1 febbraio 1980, ora locale.
const zip = appendEntries(base, files, new Date(1980, 1, 1));

// 4. Controllo: jar del plugin e binario integri, binario eseguibile.
const entries = new Map(listEntries(zip).map((e) => [e.name, e]));
const jar = `${PLUGIN}/lib/${PLUGIN}-${version}.jar`;
const bin = entries.get(`${PLUGIN}/bin/${exe}`);
if (!entries.has(jar)) throw new Error(`${jar} assente nello zip`);
if (!bin || (bin.mode & 0o777) !== 0o755) throw new Error(`${exe} assente o senza permessi 0755`);
if (!readEntry(zip, bin).equals(readFileSync(cli)))
  throw new Error(`${exe} diverso dall'originale`);
readEntry(zip, entries.get(jar));

const out = join(app, 'dist', `${PLUGIN}-${target}-${version}.zip`);
mkdirSync(dirname(out), { recursive: true });
writeFileSync(out, zip);
console.log(`pacchetto: ${out} (${Math.round(zip.length / 1024)} KiB)`);
