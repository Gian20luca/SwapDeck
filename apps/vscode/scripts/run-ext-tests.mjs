// Test dell'estensione in un VS Code isolato (cartelle utente ed estensioni temporanee), con il
// finto cswap e una cartella dati temporanea. Non tocca il VS Code né il cswap dell'utente.
// Variabili: VSCODE_TEST_VERSION (default `stable`), SWAPDECK_SKIP_CARGO=1 (binari già pronti).
import { runTests } from '@vscode/test-electron';
import { build } from 'esbuild';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ext = fileURLToPath(new URL('..', import.meta.url));
const repo = join(ext, '..', '..');
const exe = process.platform === 'win32' ? '.exe' : '';
const targetDir = process.env.CARGO_TARGET_DIR ?? join(repo, 'target');

if (process.env.SWAPDECK_SKIP_CARGO !== '1') {
  const r = spawnSync('cargo', ['build', '-p', 'swapdeck-cli', '--bins'], {
    cwd: repo,
    stdio: 'inherit',
  });
  if (r.status !== 0) process.exit(r.status ?? 1);
}
const cli = join(targetDir, 'debug', `swapdeck-cli${exe}`);
const fakeSrc = join(targetDir, 'debug', `swapdeck-fake-cswap${exe}`);

await build({
  absWorkingDir: ext,
  entryPoints: { suite: 'test/suite.ts' },
  outdir: 'out-test',
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node20',
  external: ['vscode'],
  logLevel: 'warning',
});

const root = mkdtempSync(join(tmpdir(), 'swapdeck-ext-'));
let code = 1;
try {
  const fakeDir = join(root, 'fake');
  mkdirSync(fakeDir);
  const cswap = join(fakeDir, `cswap${exe}`);
  copyFileSync(fakeSrc, cswap);
  const fakeState = join(fakeDir, 'state.json');
  copyFileSync(join(repo, 'fixtures', 'cswap', 'list.json'), fakeState);
  writeFileSync(
    join(fakeDir, 'fake.json'),
    JSON.stringify({ fixtures: join(repo, 'fixtures', 'cswap'), state: fakeState }),
  );
  const user = join(root, 'user');
  mkdirSync(join(user, 'User'), { recursive: true });
  writeFileSync(
    join(user, 'User', 'settings.json'),
    JSON.stringify({
      'swapdeck.cliPath': cli,
      'swapdeck.engine.enabled': false,
      'security.workspace.trust.enabled': false,
      'update.mode': 'none',
      'telemetry.telemetryLevel': 'off',
    }),
  );
  // Config di prova: niente avvio al login anche se l'app desktop leggesse questa cartella.
  mkdirSync(join(root, 'home', 'config'), { recursive: true });
  writeFileSync(join(root, 'home', 'config', 'config.json'), JSON.stringify({ autostart: false }));
  const workspace = join(root, 'workspace');
  mkdirSync(workspace);
  const env = {
    SWAPDECK_CSWAP: cswap,
    SWAPDECK_HOME: join(root, 'home'),
    SWAPDECK_TEST_CLI: cli,
    SWAPDECK_TEST_FAKE_STATE: fakeState,
  };
  // Se lanciato da un terminale di VS Code, Code.exe partirebbe come Node.
  delete process.env.ELECTRON_RUN_AS_NODE;
  // Anche il processo di VS Code eredita le variabili: niente cswap reale in nessun caso.
  Object.assign(process.env, env);
  code = await runTests({
    version: process.env.VSCODE_TEST_VERSION ?? 'stable',
    extensionDevelopmentPath: ext,
    extensionTestsPath: join(ext, 'out-test', 'suite.js'),
    extensionTestsEnv: env,
    launchArgs: [
      workspace,
      `--user-data-dir=${user}`,
      `--extensions-dir=${join(root, 'extensions')}`,
      '--disable-extensions',
      '--disable-workspace-trust',
      '--skip-welcome',
      '--skip-release-notes',
      '--disable-telemetry',
    ],
  })
    .then(() => 0)
    .catch((e) => {
      console.error(`test dell'estensione falliti: ${e}`);
      return 1;
    });
} finally {
  rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
}
process.exit(code);
