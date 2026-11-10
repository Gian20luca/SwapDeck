// Le estensioni per le IDE non si riferiscono mai l'una all'altra: `apps/vscode` e `apps/jetbrains`
// usano solo `packages/*` e `swapdeck-cli`. Cerca nei sorgenti di ciascuna (import, require,
// dipendenze, percorsi tra virgolette nei file di build) un riferimento all'altra.
// Uso: `node scripts/check-app-isolation.mjs [radice]` (radice di default: il repository).
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), '..'));

/** App isolate tra loro, con i nomi di pacchetto con cui potrebbero essere importate. */
const APPS = [
  { dir: 'apps/vscode', names: ['swapdeck-vscode'] },
  { dir: 'apps/jetbrains', names: ['swapdeck-jetbrains'] },
];

const SKIP_DIRS = new Set([
  'node_modules',
  'out',
  'out-test',
  'dist',
  'build',
  'bin',
  '.gradle',
  '.intellijPlatform',
  '.idea',
  '.vscode-test',
  'generated',
]);
const CODE_EXT = /\.(?:[cm]?[jt]sx?|json|kts?|gradle|java|xml|properties|toml)$/;
/** Stringhe tra virgolette singole o doppie (non i template: nei commenti si usano i backtick). */
const QUOTED = /'([^'\n]*)'|"([^"\n]*)"/g;

function packageName(dir) {
  try {
    return JSON.parse(readFileSync(join(root, dir, 'package.json'), 'utf8')).name;
  } catch {
    return undefined;
  }
}

function* files(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (!SKIP_DIRS.has(entry.name)) yield* files(path);
    } else if (entry.isFile() && CODE_EXT.test(entry.name)) {
      yield path;
    }
  }
}

/**
 * Il riferimento `value`, trovato in `file` dell'app in `appDir`, punta all'altra app? I percorsi
 * relativi si risolvono sia dalla cartella del file (import) sia dalla radice dell'app (Gradle).
 */
function pointsTo(value, file, appDir, other) {
  const otherDir = join(root, other.dir);
  const normalized = value.replaceAll('\\', '/');
  if (normalized.includes(other.dir)) return true;
  if (other.names.some((n) => normalized === n || normalized.startsWith(`${n}/`))) return true;
  // Percorsi relativi (anche `file:`, `link:`, `workspace:` con percorso).
  const path = normalized.replace(/^(?:file:|link:|workspace:)/, '');
  if (path.startsWith('./') || path.startsWith('../')) {
    return [dirname(file), appDir].some((base) => {
      const target = resolve(base, path);
      return target === otherDir || target.startsWith(otherDir + sep);
    });
  }
  return false;
}

const problems = [];
for (const app of APPS) {
  const appDir = join(root, app.dir);
  if (!existsSync(appDir) || !statSync(appDir).isDirectory()) continue;
  for (const other of APPS) {
    if (other === app) continue;
    const name = packageName(other.dir);
    const target = { ...other, names: name ? [...other.names, name] : other.names };
    for (const file of files(appDir)) {
      const lines = readFileSync(file, 'utf8').split('\n');
      lines.forEach((line, i) => {
        for (const m of line.matchAll(QUOTED)) {
          const value = m[1] ?? m[2] ?? '';
          if (value && pointsTo(value, file, appDir, target)) {
            problems.push(
              `${relative(root, file)}:${i + 1}: riferimento a ${other.dir} (${value})`,
            );
          }
        }
      });
    }
  }
}

if (problems.length > 0) {
  console.error('Le estensioni non devono riferirsi l’una all’altra (usare packages/*):');
  for (const p of problems) console.error(`  ${p}`);
  process.exit(1);
}
console.log('isolamento delle app: ok');
