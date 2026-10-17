import type { MessageKey } from '../i18n/index.js';

/** Chiavi di `cswap config` e valori ammessi (`docs/CSWAP-CONTRATTO.md`, §3.8). */
export type ConfigKind =
  | { type: 'number'; min: number; max: number; integer?: boolean }
  | { type: 'choice'; choices: string[] }
  | { type: 'bool' }
  | { type: 'string' };

export interface ConfigKeyInfo {
  kind: ConfigKind;
  label: MessageKey;
  /** Spiegazione mostrata sotto il campo. */
  hint?: MessageKey;
  /** Spiegazione della chiave (la «i»); assente se basta quella visibile sotto il campo (`hint`). */
  info?: MessageKey;
  /** Valore di default di cswap, come testo. */
  default: string;
}

export const CONFIG_KEYS: Record<string, ConfigKeyInfo> = {
  'autoswitch.threshold': {
    kind: { type: 'number', min: 50, max: 99.9 },
    label: 'config.key.threshold',
    info: 'info.config.threshold',
    default: '90',
  },
  'autoswitch.intervalSeconds': {
    kind: { type: 'number', min: 15, max: 3600 },
    label: 'config.key.intervalSeconds',
    info: 'info.config.intervalSeconds',
    default: '60',
  },
  'autoswitch.cooldownSeconds': {
    kind: { type: 'number', min: 0, max: 86400 },
    label: 'config.key.cooldownSeconds',
    info: 'info.config.cooldownSeconds',
    default: '300',
  },
  'autoswitch.hysteresisPct': {
    kind: { type: 'number', min: 0, max: 50 },
    label: 'config.key.hysteresisPct',
    info: 'info.config.hysteresisPct',
    default: '10',
  },
  'autoswitch.strategy': {
    kind: { type: 'choice', choices: ['best', 'consume-first'] },
    label: 'config.key.strategy',
    info: 'info.config.strategy',
    default: 'best',
  },
  'autoswitch.includeApiKeyAccounts': {
    kind: { type: 'bool' },
    label: 'config.key.includeApiKeyAccounts',
    info: 'info.config.includeApiKeyAccounts',
    default: 'false',
  },
  'autoswitch.unhealthyTicks': {
    kind: { type: 'number', min: 1, max: 100, integer: true },
    label: 'config.key.unhealthyTicks',
    info: 'info.config.unhealthyTicks',
    default: '3',
  },
  'autoswitch.model': {
    kind: { type: 'string' },
    label: 'config.key.model',
    info: 'info.config.model',
    default: '',
  },
  'ui.theme': {
    kind: { type: 'choice', choices: ['dark', 'light', 'auto'] },
    label: 'config.key.theme',
    hint: 'config.key.theme.hint',
    default: 'auto',
  },
};

/** Errore di formato per il valore scritto (`null` = valido); cswap valida comunque in modo rigido. */
export function validateConfigValue(key: string, text: string): MessageKey | null {
  const kind = CONFIG_KEYS[key]?.kind;
  if (!kind) return text.trim() === '' ? 'config.err.empty' : null;
  switch (kind.type) {
    case 'number': {
      const n = Number(text);
      if (text.trim() === '' || !Number.isFinite(n)) return 'config.err.number';
      if (kind.integer && !Number.isInteger(n)) return 'config.err.integer';
      return n < kind.min || n > kind.max ? 'config.err.range' : null;
    }
    case 'choice':
      return kind.choices.includes(text) ? null : 'config.err.choice';
    case 'bool':
      return ['true', 'false'].includes(text) ? null : 'config.err.choice';
    case 'string':
      return text.trim() === '' ? 'config.err.empty' : null;
  }
}

/** Testo da mostrare nel campo per il valore JSON di `config --json`. */
export function configValueText(value: unknown): string {
  if (value === null || value === undefined) return '';
  return typeof value === 'string' ? value : String(value);
}
