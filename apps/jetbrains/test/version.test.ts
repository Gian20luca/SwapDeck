// La versione del plugin (`gradle.properties`) e quella del pacchetto pnpm seguono il Rust
// (`[workspace.package]` del `Cargo.toml` alla radice), come l'estensione VS Code.
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const read = (path: string) => readFileSync(new URL(path, import.meta.url), 'utf8');

describe('versione del plugin', () => {
  const cargo = /^version = "([^"]+)"/m.exec(read('../../../Cargo.toml'))?.[1];

  it('gradle.properties = Cargo.toml', () => {
    const gradle = /^pluginVersion\s*=\s*(\S+)\s*$/m.exec(read('../gradle.properties'))?.[1];
    expect(cargo).toBeTruthy();
    expect(gradle).toBe(cargo);
  });

  it('package.json = Cargo.toml', () => {
    expect((JSON.parse(read('../package.json')) as { version: string }).version).toBe(cargo);
  });
});
