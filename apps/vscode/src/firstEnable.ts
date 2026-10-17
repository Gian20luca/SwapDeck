import type { EngineRules } from '@swapdeck/types';
import { enableConfirmed } from '@swapdeck/ui/engineRules';

/** Risposta al dialogo della prima accensione; `undefined` = annullato. */
export type FirstEnableAnswer = 'on' | 'observe' | undefined;

/**
 * Regole con cui «SwapDeck: Avvia motore» accende il motore. Alla prima accensione (vedi
 * `EngineRules::enable_confirmed`) chiede conferma con `ask`, come la scheda Regole: «Accendi»
 * o «Solo osservazione»; dalle volte successive parte subito. `null` = annullato, non si scrive nulla.
 */
export async function rulesToStart(
  rules: EngineRules,
  ask: () => Promise<FirstEnableAnswer>,
): Promise<EngineRules | null> {
  if (enableConfirmed(rules)) return { ...rules, enabled: true };
  const answer = await ask();
  if (!answer) return null;
  return {
    ...rules,
    enabled: true,
    dryRun: rules.dryRun || answer === 'observe',
    firstEnableConfirmed: true,
  };
}
