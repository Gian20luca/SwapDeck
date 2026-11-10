// Lancia il wrapper Gradle del plugin (`gradlew` o `gradlew.bat`) con gli argomenti dati.
// Serve un JDK 21 (JAVA_HOME o `java` nel PATH). Uso: `node scripts/gradle.mjs test buildPlugin`.
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const cwd = fileURLToPath(new URL('..', import.meta.url));
const windows = process.platform === 'win32';
const result = spawnSync(windows ? 'gradlew.bat' : './gradlew', process.argv.slice(2), {
  cwd,
  stdio: 'inherit',
  shell: windows,
});
if (result.error) {
  console.error(`Gradle non avviato: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
