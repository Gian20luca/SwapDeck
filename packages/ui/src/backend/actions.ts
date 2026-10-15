import type { CswapAction, SwitchStrategy } from '@swapdeck/types';

/** Costruttori delle azioni più comuni: un solo punto che conosce i campi obbligatori del tipo generato. */
export const actions = {
  switchTo(target: number, options: { force?: boolean } = {}): CswapAction {
    return { kind: 'switch', target, force: options.force ?? false };
  },
  switchBy(strategy: SwitchStrategy, models?: string[]): CswapAction {
    return models
      ? { kind: 'switch', strategy, models, force: false }
      : { kind: 'switch', strategy, force: false };
  },
  purge(): CswapAction {
    return { kind: 'purge' };
  },
};
