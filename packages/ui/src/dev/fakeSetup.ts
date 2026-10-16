import type {
  CswapError,
  SetupDiagnosis,
  SetupOutcome,
  SetupProgress,
  SetupStep,
  UpdateInfo,
} from '@swapdeck/types';
import { BackendError } from '../backend/types.js';

/** Stato iniziale simulato di cswap per la procedura di primo avvio. */
export type FakeSetupState = 'ready' | 'missing' | 'unsupportedVersion' | 'unsupportedSchema';

export interface FakeSetupOptions {
  /** Default `ready`. */
  state?: FakeSetupState;
  /** uv già presente (default `true`). */
  uv?: boolean;
  /** Versione installata (default `0.27.2`). */
  installed?: string;
  /** Ultima versione su PyPI (default uguale all'installata). */
  latest?: string;
  /** Errore restituito da installazione e aggiornamento (con `StepFailed` sul passo). */
  failure?: CswapError;
}

const CSWAP_PATH = 'C:\\Users\\user\\.local\\bin\\cswap.exe';
const UV_PATH = 'C:\\Users\\user\\.local\\bin\\uv.exe';

/** Righe di esempio di `uv tool install` (come le passa il Rust: già ripulite). */
const INSTALL_LINES = [
  'Resolved 17 packages in 4.75s',
  'Prepared 17 packages in 29.31s',
  'Installed 17 packages in 2.91s',
  '+ claude-swap==0.27.2',
  'Installed 2 executables: claude-swap, cswap',
];

function cmp(a: string, b: string): number {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < 3; i += 1) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/** Simulazione di diagnosi, installazione, controllo versione e aggiornamento di cswap. */
export class FakeSetup {
  private state: FakeSetupState;
  private uv: boolean;
  private installed: string;
  private readonly latest: string;
  private dismissed: string | null = null;
  private readonly failure: CswapError | undefined;
  /** Chiamate ricevute, utili ai test. */
  readonly calls: string[] = [];

  constructor(
    options: FakeSetupOptions,
    private readonly accountCount: () => number,
    private readonly delay: () => Promise<void>,
  ) {
    this.state = options.state ?? 'ready';
    this.uv = options.uv ?? true;
    this.installed = options.installed ?? '0.27.2';
    this.latest = options.latest ?? this.installed;
    this.failure = options.failure;
  }

  async diagnose(): Promise<SetupDiagnosis> {
    this.calls.push('diagnose');
    await this.delay();
    return this.diagnosis();
  }

  private diagnosis(): SetupDiagnosis {
    const base: SetupDiagnosis = {
      status: this.state === 'missing' ? 'cswapMissing' : this.state,
      minVersion: '0.26.0',
      supportedSchemaVersion: 1,
      ...(this.uv ? { uv: { path: UV_PATH, version: '0.13.1' } } : {}),
    };
    if (this.state === 'missing') return base;
    const withCswap: SetupDiagnosis = {
      ...base,
      cswap: { path: CSWAP_PATH, version: this.installed },
    };
    switch (this.state) {
      case 'ready':
        return { ...withCswap, schemaVersion: 1, accountCount: this.accountCount() };
      case 'unsupportedVersion':
        return {
          ...withCswap,
          error: {
            kind: 'unsupportedVersion',
            message: `cswap ${this.installed} non supportato: serve almeno 0.26.0`,
          },
        };
      case 'unsupportedSchema':
        return {
          ...withCswap,
          schemaVersion: 2,
          error: { kind: 'unsupportedSchema', message: 'schemaVersion 2 non supportato' },
        };
    }
  }

  private async step(
    step: SetupStep,
    onProgress: (event: SetupProgress) => void,
    lines: string[] = [],
  ): Promise<void> {
    onProgress({ event: 'stepStarted', step });
    for (const text of lines) {
      await this.delay();
      onProgress({ event: 'line', step, text });
    }
    if (this.failure && (step === 'installCswap' || step === 'upgradeCswap')) {
      onProgress({ event: 'stepFailed', step, error: this.failure });
      throw new BackendError(this.failure);
    }
    onProgress({ event: 'stepDone', step });
  }

  async install(onProgress: (event: SetupProgress) => void): Promise<SetupOutcome> {
    this.calls.push('install');
    const uvInstalled = !this.uv;
    if (this.uv) {
      onProgress({ event: 'stepSkipped', step: 'downloadUv' });
      onProgress({ event: 'stepSkipped', step: 'installUv' });
    } else {
      await this.step('downloadUv', onProgress, ['https://astral.sh/uv/install.ps1']);
      await this.step('installUv', onProgress, [
        'downloading uv 0.13.1',
        "everything's installed!",
      ]);
      this.uv = true;
    }
    await this.step('installCswap', onProgress, INSTALL_LINES);
    this.state = 'ready';
    await this.step('verifyCswap', onProgress, [`cswap ${this.installed} · schemaVersion 1`]);
    return { diagnosis: this.diagnosis(), uvInstalled };
  }

  async checkUpdate(): Promise<UpdateInfo> {
    this.calls.push('checkUpdate');
    await this.delay();
    if (this.state === 'missing')
      throw new BackendError({ kind: 'notInstalled', message: 'cswap non trovato' });
    return {
      installed: this.installed,
      latest: this.latest,
      updateAvailable: cmp(this.latest, this.installed) > 0,
      dismissed: this.dismissed === this.latest,
      checkedAt: Math.floor(Date.now() / 1000),
    };
  }

  async dismiss(version: string): Promise<void> {
    this.calls.push(`dismiss:${version}`);
    await this.delay();
    this.dismissed = version;
  }

  async upgrade(onProgress: (event: SetupProgress) => void): Promise<SetupOutcome> {
    this.calls.push('upgrade');
    if (!this.uv) throw new BackendError({ kind: 'notInstalled', message: 'uv non trovato' });
    const previous = this.installed;
    await this.step('upgradeCswap', onProgress, [
      `Updated claude-swap v${previous} -> v${this.latest}`,
    ]);
    this.installed = this.latest;
    this.state = 'ready';
    await this.step('verifyCswap', onProgress, [`cswap ${this.installed} · schemaVersion 1`]);
    return { diagnosis: this.diagnosis(), uvInstalled: false, previousVersion: previous };
  }
}
