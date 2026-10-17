import type { EngineRules } from '@swapdeck/types';
import { enableConfirmed } from '@swapdeck/ui/engineRules';

/** Risposta al dialogo della prima accensione; `undefined` = annullato. */
export type FirstEnableAnswer = 'on' | 'observe' | undefined;

/**
 * Regole con cui «SwapDeck: Avvia motore» accende il motore. Alla prima accensione (vedi
 * `EngineRules::enable_confirmed`) chiede conferma con `ask`, come la scheda Regole: «Accendi»
 * o «Solo osservazione»; dalle volte successive parte subito. Dopo il dialogo le regole si
 * rileggono (un'altra finestra può averle cambiate nel frattempo). `null` = annullato.
 */
export async function rulesToStart(
  read: () => Promise<EngineRules>,
  ask: () => Promise<FirstEnableAnswer>,
): Promise<EngineRules | null> {
  const rules = await read();
  if (enableConfirmed(rules)) return { ...rules, enabled: true };
  const answer = await ask();
  if (!answer) return null;
  const fresh = await read();
  return {
    ...fresh,
    enabled: true,
    dryRun: fresh.dryRun || answer === 'observe',
    firstEnableConfirmed: true,
  };
}
