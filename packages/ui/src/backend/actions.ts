import type { CswapAction, RunOptions, SwitchStrategy } from '@swapdeck/types';

/** Costruttori delle azioni: un solo punto che conosce i campi obbligatori del tipo generato. */
export const actions = {
  switchTo(target: number, options: { force?: boolean } = {}): CswapAction {
    return { kind: 'switch', target, force: options.force ?? false };
  },
  switchBy(strategy: SwitchStrategy, models?: string[]): CswapAction {
    return models
      ? { kind: 'switch', strategy, models, force: false }
      : { kind: 'switch', strategy, force: false };
  },
  /** `cswap add`: cattura l'account loggato in Claude Code. */
  addCurrent(options: { slot?: number; alias?: string; overwrite?: boolean } = {}): CswapAction {
    return { kind: 'add', ...options };
  },
  /** `cswap add-token`: il token passa solo nell'azione verso l'host, mai nello stato della UI. */
  addToken(
    token: string,
    options: { email?: string; slot?: number; overwrite?: boolean } = {},
  ): CswapAction {
    return { kind: 'addToken', token, ...options };
  },
  remove(account: number): CswapAction {
    return { kind: 'remove', account };
  },
  disable(account: number): CswapAction {
    return { kind: 'disable', account };
  },
  enable(account: number): CswapAction {
    return { kind: 'enable', account };
  },
  setAlias(account: number, alias: string | null): CswapAction {
    return { kind: 'setAlias', account, alias };
  },
  swap(a: number, b: number): CswapAction {
    return { kind: 'swap', a, b };
  },
  move(account: number, slot: number): CswapAction {
    return { kind: 'move', account, slot };
  },
  map(account: number, path: string): CswapAction {
    return { kind: 'map', account, path };
  },
  unmap(path: string): CswapAction {
    return { kind: 'unmap', path };
  },
  configSet(key: string, value: string): CswapAction {
    return { kind: 'configSet', key, value };
  },
  configUnset(key: string): CswapAction {
    return { kind: 'configUnset', key };
  },
  exportTo(path: string, target: { account: number } | { full: true }): CswapAction {
    return 'account' in target
      ? { kind: 'export', path, account: target.account }
      : { kind: 'export', path, full: true };
  },
  importFrom(path: string, force = false): CswapAction {
    return { kind: 'import', path, force };
  },
  unclaimedPurge(id: string): CswapAction {
    return { kind: 'unclaimedPurge', id };
  },
  run(account: number, options: Omit<RunOptions, 'account'> = {}): CswapAction {
    return { kind: 'run', options: { ...options, account } };
  },
  purge(): CswapAction {
    return { kind: 'purge' };
  },
};
