/**
 * Regole di validazione delle richieste del pannello, lette da `protocol.json`: il pannello non è
 * fidato, l'host le applica prima di chiamare `swapdeck-cli` (che rivalida tutto). Le lunghezze
 * sono in unità UTF-16 (`String.length` in JavaScript e in Kotlin).
 */
import type { CswapAction } from '@swapdeck/types';
import protocol from '../protocol.json' with { type: 'json' };

export type ActionKind = CswapAction['kind'];

/**
 * Tutti i `kind` di `CswapAction`: un `kind` fuori elenco è `invalidUsage` già nell'host.
 * `rules.test.ts` controlla a compilazione che l'elenco copra l'unione del Rust.
 */
export const ACTION_KINDS: readonly ActionKind[] = protocol.actionKinds as ActionKind[];

/**
 * Argomenti di `claude` ammessi per `run` dal pannello: solo flag senza valore e senza effetti sui
 * permessi (il comando si apre in un terminale dell'utente).
 */
export const SAFE_CLAUDE_ARGS: ReadonlySet<string> = new Set(protocol.safeClaudeArgs);

const { limits } = protocol;

/** Chiave della config di cswap (`getConfigValue`). */
export const CONFIG_KEY = new RegExp(limits.configKeyPattern);
/** Versione di claude-swap (PEP 440, solo caratteri sicuri; `dismissCswapUpdate`). */
export const VERSION = new RegExp(limits.versionPattern);
/** Codice di «Rifai login»: una riga sola, al massimo questa lunghezza. */
export const MAX_RELOGIN_CODE_CHARS: number = limits.reloginCodeMaxLength;
/** Testo massimo copiato negli appunti su richiesta del pannello. */
export const MAX_COPY_CHARS: number = limits.copyTextMaxLength;

/** Opzioni dei selettori nativi di file e cartelle. */
export const PICK_LIMITS = {
  textFields: limits.pick.textFields as ReadonlyArray<'title' | 'defaultPath'>,
  textMaxLength: limits.pick.textMaxLength,
  maxFilters: limits.pick.maxFilters,
  filterNameMaxLength: limits.pick.filterNameMaxLength,
  extension: new RegExp(limits.pick.extensionPattern),
} as const;

export type UsageWindow = '5h' | '7d' | 'custom';
/** Finestre ammesse per `getUsageBreakdown`. */
export const USAGE_WINDOWS: readonly UsageWindow[] = limits.usage.windows as UsageWindow[];
/** Raggruppamenti ammessi per `getUsageBreakdown`. */
export const USAGE_GROUPS: readonly string[] = limits.usage.groupBy;

/** Periodo di `getHistory` senza `sinceSeconds`. */
export const DEFAULT_HISTORY_SECONDS: number = protocol.defaults.historySeconds;
