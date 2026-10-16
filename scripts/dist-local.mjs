// Build Windows locale in `dist-locale/` (toolchain gnullvm, senza Build Tools MSVC):
// installer NSIS per utente (se riesce), cartella portatile zippata, .vsix win32-x64, LEGGIMI.txt.
//
//   pnpm dist:local            (oppure: node scripts/dist-local.mjs)
//   pnpm dist:local --clean    (alla fine libera la target di rilascio: disco piccolo)
//
// Gli installer ufficiali escono dalla CI con MSVC (.github/workflows/release.yml).
import { execFileSync, spawnSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { delimiter, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: { clean: { type: 'boolean', default: false } },
});

if (process.platform !== 'win32') {
  console.error('dist:local è solo per Windows: sugli altri OS i pacchetti escono dalla CI');
  process.exit(2);
}

const repo = resolve(fileURLToPath(new URL('..', import.meta.url)));
const desktop = join(repo, 'apps', 'desktop');
const tauriDir = join(desktop, 'src-tauri');
const binaries = join(tauriDir, 'binaries');
const out = join(repo, 'dist-locale');
const targetDir = process.env.CARGO_TARGET_DIR
  ? resolve(process.env.CARGO_TARGET_DIR)
  : join(repo, 'target');
const release = join(targetDir, 'release');
const version = /^version = "([^"]+)"/m.exec(readFileSync(join(repo, 'Cargo.toml'), 'utf8'))?.[1];
if (!version) throw new Error('versione non trovata in Cargo.toml');

function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { cwd: repo, stdio: 'inherit', ...opts });
  if (r.error) throw r.error;
  return r.status === 0;
}

function must(ok, what) {
  if (!ok) {
    console.error(`non riuscito: ${what}`);
    process.exit(1);
  }
}

// pnpm lanciato da `pnpm dist:local` espone il proprio script in npm_execpath.
function pnpm(args, opts) {
  const cli = process.env.npm_execpath;
  if (cli && /pnpm\.c?js$/.test(cli)) return run(process.execPath, [cli, ...args], opts);
  return run('pnpm.cmd', args, { ...opts, shell: true });
}

function findInPath(file) {
  const dir = (process.env.PATH ?? '').split(delimiter).find((d) => d && existsSync(join(d, file)));
  return dir ? join(dir, file) : undefined;
}

// WebView2Loader.dll prodotta da webview2-com-sys (con i target gnu non è collegata staticamente).
function findWebView2Loader() {
  const build = join(release, 'build');
  if (!existsSync(build)) return undefined;
  for (const d of readdirSync(build)) {
    if (!d.startsWith('webview2-com-sys-')) continue;
    const p = join(build, d, 'out', 'x64', 'WebView2Loader.dll');
    if (existsSync(p)) return p;
  }
  return undefined;
}

const host = /host: (\S+)/.exec(execFileSync('rustc', ['-vV'], { encoding: 'utf8' }))?.[1];
const gnullvm = host?.endsWith('-gnullvm') ?? false;
const hasCli = existsSync(join(repo, 'apps', 'cli', 'Cargo.toml'));

// Cartella di uscita: si ricrea da zero (mai seguendo un collegamento).
if (existsSync(out)) {
  if (lstatSync(out).isSymbolicLink()) throw new Error(`${out} è un collegamento: non lo tocco`);
  rmSync(out, { recursive: true, force: true });
}
mkdirSync(out);

// 1. Pagina e binari di rilascio.
must(pnpm(['--filter', '@swapdeck/desktop', 'build']), 'pagina di @swapdeck/desktop');
must(
  run('cargo', ['build', '--release', '-p', 'swapdeck-desktop', '--features', 'custom-protocol']),
  'swapdeck.exe',
);
if (hasCli) {
  must(
    run('cargo', ['build', '--release', '-p', 'swapdeck-cli', '--bin', 'swapdeck-cli']),
    'swapdeck-cli.exe',
  );
} else {
  console.warn('apps/cli assente (step 7 non integrato): niente swapdeck-cli né .vsix');
}

const dlls = [];
if (gnullvm) {
  const unwind = findInPath('libunwind.dll');
  must(unwind, 'libunwind.dll nel PATH (llvm-mingw, vedi apps/desktop/README.md)');
  dlls.push(unwind);
}
const loader = findWebView2Loader();
if (loader) dlls.push(loader);
else if (gnullvm) must(false, 'WebView2Loader.dll di webview2-com-sys');

// 2. Installer NSIS per utente con le DLL accanto all'eseguibile.
mkdirSync(binaries, { recursive: true });
const resources = {};
for (const dll of dlls) {
  const name = dll.split(/[\\/]/).pop();
  copyFileSync(dll, join(binaries, name));
  resources[`binaries/${name}`] = name;
}
const overlay = { bundle: { active: true, targets: ['nsis'], resources } };
if (hasCli) {
  copyFileSync(join(release, 'swapdeck-cli.exe'), join(binaries, `swapdeck-cli-${host}.exe`));
  overlay.bundle.externalBin = ['binaries/swapdeck-cli'];
}
const overlayFile = join(tauriDir, 'tauri.local.conf.json');
writeFileSync(overlayFile, JSON.stringify(overlay));
const nsisOk = pnpm(
  [
    '--filter',
    '@swapdeck/desktop',
    'exec',
    'tauri',
    'build',
    '--bundles',
    'nsis',
    '--config',
    overlayFile,
  ],
  { env: { ...process.env, CI: 'true' } },
);
rmSync(overlayFile, { force: true });
let installer;
const nsisDir = join(release, 'bundle', 'nsis');
if (nsisOk && existsSync(nsisDir)) {
  installer = readdirSync(nsisDir).find((f) => f.endsWith('-setup.exe') && f.includes(version));
}
if (installer) copyFileSync(join(nsisDir, installer), join(out, installer));
else console.warn('installer NSIS non prodotto: resta la versione portatile');

// 3. Cartella portatile e zip (tar di Windows, formato scelto dall'estensione).
const portableName = `SwapDeck-${version}-portatile-win-x64`;
const portable = join(out, portableName);
mkdirSync(portable);
copyFileSync(join(release, 'swapdeck.exe'), join(portable, 'swapdeck.exe'));
if (hasCli) copyFileSync(join(release, 'swapdeck-cli.exe'), join(portable, 'swapdeck-cli.exe'));
for (const dll of dlls) copyFileSync(dll, join(portable, dll.split(/[\\/]/).pop()));
const tar = join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe');
must(run(tar, ['-a', '-c', '-f', join(out, `${portableName}.zip`), '-C', portable, '.']), 'zip');

// 4. Estensione VS Code win32-x64 (script dello step 7).
let vsix;
if (hasCli) {
  must(pnpm(['--filter', 'swapdeck-vscode', 'package']), '.vsix win32-x64');
  const dist = join(repo, 'apps', 'vscode', 'dist');
  vsix = readdirSync(dist).find((f) => f.endsWith('.vsix') && f.includes('win32-x64'));
  must(vsix, '.vsix in apps/vscode/dist');
  copyFileSync(join(dist, vsix), join(out, vsix));
}

// 5. LEGGIMI.txt
const lines = [
  `SwapDeck ${version} - build locale per Windows (${new Date().toISOString().slice(0, 10)})`,
  '',
  'Build di prova fatta sul PC di sviluppo con il toolchain Rust gnullvm (llvm-mingw).',
  'Gli installer ufficiali escono dalla CI di GitHub con MSVC.',
  '',
  'CONTENUTO',
  installer
    ? `- ${installer}: installer per l'utente corrente (nessun privilegio di amministratore).`
    : '- (installer NSIS non prodotto in questa build: usare la versione portatile)',
  `- ${portableName}.zip: versione portatile, la stessa anche nella cartella accanto.`,
  vsix
    ? `- ${vsix}: estensione VS Code con swapdeck-cli incluso.`
    : '- (estensione VS Code non prodotta: swapdeck-cli non ancora integrato)',
  '',
  'INSTALLAZIONE',
  '1. Installer: doppio clic e avanti. Si installa in %LOCALAPPDATA%\\SwapDeck; se manca',
  '   WebView2 lo scarica (Windows 10 e 11 di solito lo hanno già).',
  '2. Portatile: estrarre lo zip in una cartella qualsiasi e avviare swapdeck.exe.',
  '   I file della cartella (exe e DLL) devono restare insieme.',
  '3. Estensione: in VS Code, Estensioni > menu "..." > Installa da VSIX, oppure',
  vsix ? `   code --install-extension ${vsix}` : '   code --install-extension <file>.vsix',
  '',
  'PRIMA PROVA',
  "- L'app parte nell'area di notifica (icona ad anello) e apre la finestra principale.",
  "  Il menu dell'icona ha il cambio rapido, il mini-widget ed Esci.",
  '- Il motore delle regole è SPENTO di default: SwapDeck legge soltanto lo stato di cswap',
  '  (cswap list) e non cambia account da solo. Per accenderlo, in %APPDATA%\\swapdeck\\config.json:',
  '  "engine": { "enabled": true }   oppure   "engine": { "dryRun": true }  (prova senza cambi).',
  "- Con un altro motore attivo (cswap auto o un'attività pianificata) il motore risulta «sospeso» (icona nella",
  '  variante sospesa e avviso sopra la dashboard): è voluto, per non cambiare account in',
  '  concorrenza con l\'altro motore.',
  '- Le azioni fatte a mano dalla dashboard (cambio di account e simili) agiscono sul cswap',
  '  reale e chiedono conferma.',
  '- Gli aggiornamenti automatici sono spenti finché il repo è privato.',
  '- Avvio automatico al login: attivato al primo avvio, si toglie dalla finestra principale.',
  '',
  'PROVA SENZA TOCCARE IL CSWAP REALE',
  '  set SWAPDECK_CSWAP=C:\\prova\\cswap.exe      (finto cswap)',
  '  set SWAPDECK_HOME=C:\\prova\\home            (config, lock e storico di prova)',
  '  swapdeck.exe',
  'La config di prova sta in C:\\prova\\home\\config\\config.json: con { "autostart": false }',
  "l'app non registra l'avvio automatico al primo avvio (la voce di Windows è unica per utente).",
  '',
  'DISINSTALLAZIONE',
  '- Installer: Impostazioni > App > SwapDeck > Disinstalla. I dati restano in %APPDATA%\\swapdeck.',
  "- Portatile: Esci dal menu dell'icona, togliere l'avvio automatico se attivo, cancellare la cartella.",
  '',
];
writeFileSync(join(out, 'LEGGIMI.txt'), lines.join('\r\n'), 'utf8');

if (values.clean) {
  console.log('pulizia della target di rilascio (--clean)');
  run('cargo', ['clean', '--release']);
}
console.log(`\n${out}:`);
for (const f of readdirSync(out)) console.log(`  ${f}`);
