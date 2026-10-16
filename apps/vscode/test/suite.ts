/**
 * Test dell'estensione dentro un'istanza isolata di VS Code (`scripts/run-ext-tests.mjs`):
 * finto cswap (`SWAPDECK_CSWAP`) e cartella dati temporanea (`SWAPDECK_HOME`).
 */
import * as assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import * as path from 'node:path';
import * as vscode from 'vscode';
import type { SwapDeckApi } from '../src/extension.js';

async function waitFor(what: string, cond: () => boolean, ms = 30_000): Promise<void> {
  const start = Date.now();
  while (!cond()) {
    if (Date.now() - start > ms) throw new Error(`attesa scaduta: ${what}`);
    await new Promise((r) => setTimeout(r, 200));
  }
}

const tests: Array<[string, (api: SwapDeckApi) => Promise<void>]> = [
  [
    'comandi registrati',
    async () => {
      const all = await vscode.commands.getCommands(true);
      for (const c of ['openPanel', 'refresh', 'switchAccount', 'startEngine', 'stopEngine']) {
        assert.ok(all.includes(`swapdeck.${c}`), c);
      }
    },
  ],
  [
    'stato e barra di stato dal finto cswap',
    async (api) => {
      const state = await api.refresh();
      assert.equal(state.error, null, JSON.stringify(state.error));
      assert.equal(state.list?.activeAccountNumber, 6);
      assert.match(api.statusText(), /alias6 5h 66% · 7d 77%/);
      assert.equal(path.resolve(api.cliPath()), path.resolve(process.env['SWAPDECK_TEST_CLI']!));
    },
  ],
  [
    'pannello',
    async (api) => {
      await vscode.commands.executeCommand('swapdeck.openPanel');
      assert.ok(api.panelOpen());
      await waitFor('webview pronto', () => api.panelReady());
    },
  ],
  [
    'cambio di account',
    async (api) => {
      const outcome = await api.switchTo(3);
      assert.equal(outcome?.switch?.switched, true, JSON.stringify(outcome));
      assert.equal(api.state()?.list?.activeAccountNumber, 3);
      assert.match(api.statusText(), /alias3/);
      const fake = JSON.parse(readFileSync(process.env['SWAPDECK_TEST_FAKE_STATE']!, 'utf8')) as {
        activeAccountNumber: number;
      };
      assert.equal(fake.activeAccountNumber, 3);
      await api.switchTo('best');
    },
  ],
  [
    'motore avviato e fermato (lock preso e rilasciato)',
    async (api) => {
      const lock = path.join(process.env['SWAPDECK_HOME']!, 'data', 'engine.lock');
      await vscode.commands.executeCommand('swapdeck.startEngine');
      await waitFor('motore attivo', () => api.engineRunning());
      await waitFor('lock preso', () => existsSync(lock));
      await vscode.commands.executeCommand('swapdeck.stopEngine');
      await waitFor('motore fermo', () => !api.engineRunning());
      await waitFor('lock rilasciato', () => !existsSync(lock), 10_000);
    },
  ],
];

export async function run(): Promise<void> {
  const ext = vscode.extensions.getExtension<SwapDeckApi>('swapdeck.swapdeck-vscode');
  assert.ok(ext, 'estensione non trovata');
  const api = await ext.activate();
  const failures: string[] = [];
  for (const [name, fn] of tests) {
    try {
      await fn(api);
      console.log(`  ok  ${name}`);
    } catch (error) {
      console.log(`  FAIL ${name}: ${error instanceof Error ? error.stack : String(error)}`);
      failures.push(name);
    }
  }
  if (failures.length > 0)
    throw new Error(`${failures.length} test falliti: ${failures.join(', ')}`);
}
