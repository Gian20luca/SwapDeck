// Rigenera packages/types/src/generated dai tipi Rust di swapdeck-core (ts-rs).
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const result = spawnSync(
  'cargo',
  ['test', '-p', 'swapdeck-core', '--lib', 'ts_export::generated_types_are_current'],
  { cwd: root, stdio: 'inherit', env: { ...process.env, SWAPDECK_UPDATE_TYPES: '1' } },
);
if (result.error) {
  console.error(`cargo non avviato: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 1);
