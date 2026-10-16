/**
 * Notifiche nell'estensione (F18): le decide il Rust (`swapdeck-core::notify`); qui solo il
 * mittente unico e la presentazione. Con il motore dell'estensione titolare del lock arrivano
 * come eventi JSON dalle righe di `headless`; senza motore vivo da `notifications poll`, che
 * ricontrolla il lock. Con un motore vivo altrove (app desktop o altra finestra) manda lui.
 */
import type { Notification, NotifyPoll } from '@swapdeck/types';

export type Lang = 'it' | 'en';
export type Level = 'info' | 'warning' | 'error';

/** Lingua dei testi dalla lingua di VS Code. */
export function langOf(vscodeLanguage: string): Lang {
  return vscodeLanguage.toLowerCase().startsWith('it') ? 'it' : 'en';
}

function isText(v: unknown): v is { it: string; en: string } {
  const o = v as { it?: unknown; en?: unknown } | null;
  return (
    typeof o === 'object' && o !== null && typeof o.it === 'string' && typeof o.en === 'string'
  );
}

/** Notifica da una riga dell'output del motore; `null` per le righe di log. */
export function parseEngineEvent(line: string): Notification | null {
  if (!line.startsWith('{')) return null;
  let value: unknown;
  try {
    value = JSON.parse(line);
  } catch {
    return null;
  }
  const ev = value as { event?: unknown; notification?: Partial<Notification> } | null;
  const n = ev?.notification;
  if (ev?.event !== 'notification' || !n || typeof n.kind !== 'string') return null;
  if (!isText(n.title) || !isText(n.body) || typeof n.severity !== 'string') return null;
  return n as Notification;
}

/**
 * L'estensione chiede le notifiche solo se nessun motore è vivo: né il suo (che le manda come
 * eventi) né un altro (`lockHeldElsewhere` dallo stato; `undefined` = non noto, decide la CLI).
 */
export function shouldPoll(
  localEngineRunning: boolean,
  lockHeldElsewhere: boolean | undefined,
): boolean {
  return !localEngineRunning && lockHeldElsewhere !== true;
}

/** Notifiche da mostrare dall'esito di `notifications poll`. */
export function fromPoll(poll: NotifyPoll): Notification[] {
  return poll.sender === 'caller' ? poll.notifications : [];
}

/** Testo e livello del messaggio di VS Code. */
export function messageOf(n: Notification, lang: Lang): { level: Level; text: string } {
  const level: Level =
    n.severity === 'critical' ? 'error' : n.severity === 'warning' ? 'warning' : 'info';
  return { level, text: `SwapDeck: ${n.title[lang]} — ${n.body[lang]}` };
}
