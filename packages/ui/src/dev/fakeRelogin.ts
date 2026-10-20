import type { ListResponse, ReloginEvent, ReloginOutcome, ReloginResult } from '@swapdeck/types';
import { BackendError } from '../backend/types.js';

/** «Rifai login» simulato (F21) per l'anteprima e i test: nessun processo reale. */
export interface FakeReloginOptions {
  /** Esito dopo il codice (default `updated`). */
  result?: Exclude<ReloginResult, 'cancelled' | 'timedOut'>;
  /** Utenze con il pulsante (default: `relogin_required` e `token_expired`). */
  accounts?: number[];
}

const FAKE_URL = 'https://claude.ai/oauth/authorize?code=true&client_id=fake&state=anteprima';

/** Stato di una sessione simulata. */
interface Session {
  account: number;
  previous: number | null;
  onEvent: (event: ReloginEvent) => void;
  resolve: (outcome: ReloginOutcome) => void;
}

export class FakeRelogin {
  /** Numero di codici ricevuti (mai il valore). */
  codes = 0;
  private session: Session | null = null;

  constructor(
    private readonly options: FakeReloginOptions,
    private readonly list: () => ListResponse,
    private readonly onUpdated: (account: number) => void,
  ) {}

  accounts(list: ListResponse): number[] {
    return (
      this.options.accounts ??
      list.accounts
        .filter((a) => a.usageStatus === 'relogin_required' || a.usageStatus === 'token_expired')
        .map((a) => a.number)
    );
  }

  start(account: number, onEvent: (event: ReloginEvent) => void): Promise<ReloginOutcome> {
    if (this.session) {
      return Promise.reject(
        new BackendError({ kind: 'busy', message: 'un «Rifai login» è già in corso' }),
      );
    }
    const previous = this.list().activeAccountNumber;
    return new Promise((resolve) => {
      this.session = { account, previous, onEvent, resolve };
      if (previous !== null) onEvent({ event: 'stepStarted', step: 'saveActive' });
      onEvent({ event: 'stepStarted', step: 'startLogin' });
      if (this.options.result === 'claudeNotFound') {
        this.finish({ result: 'claudeNotFound', restored: true });
        return;
      }
      onEvent({ event: 'link', url: FAKE_URL, timeoutSeconds: 300 });
      onEvent({ event: 'stepStarted', step: 'waitCode' });
    });
  }

  submit(code: string): void {
    const s = this.session;
    if (!s)
      throw new BackendError({ kind: 'invalidUsage', message: 'nessun «Rifai login» in corso' });
    this.codes += 1;
    if (code.trim() === '' || /\s/.test(code.trim())) {
      s.onEvent({ event: 'codeInvalid' });
      return;
    }
    s.onEvent({ event: 'codeSent' });
    s.onEvent({ event: 'stepStarted', step: 'verify' });
    const result = this.options.result ?? 'updated';
    if (result === 'updated') {
      s.onEvent({ event: 'stepStarted', step: 'saveAccount' });
      this.onUpdated(s.account);
    }
    if (s.previous !== null && (result !== 'updated' || s.previous !== s.account)) {
      s.onEvent({ event: 'stepStarted', step: 'restoreActive' });
    }
    this.finish({
      result,
      restored: s.previous !== null,
      ...(result === 'loginFailed' ? { detail: 'Invalid code' } : {}),
    });
  }

  cancel(): void {
    if (!this.session) {
      throw new BackendError({ kind: 'invalidUsage', message: 'nessun «Rifai login» in corso' });
    }
    this.finish({ result: 'cancelled', restored: true });
  }

  private finish(partial: Pick<ReloginOutcome, 'result' | 'restored' | 'detail'>): void {
    const s = this.session;
    if (!s) return;
    this.session = null;
    s.resolve({
      ...partial,
      account: s.account,
      ...(s.previous !== null ? { previous: s.previous } : {}),
      list: this.list(),
    });
  }
}
