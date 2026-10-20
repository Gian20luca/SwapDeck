/**
 * Webview con l'app completa di `@swapdeck/ui`, in due contenitori: la vista della barra laterale
 * (`SidebarView`) e la scheda dell'editor (`DashboardPanel`). Stesso HTML, CSP con nonce, risorse
 * solo da `out/webview` e stesso gestore delle richieste (`WebviewHub`).
 */
import { randomBytes } from 'node:crypto';
import * as vscode from 'vscode';
import { toCswapError } from './cli.js';
import {
  viewLocale,
  type ProgressEvent,
  type HostMessage,
  type ViewSettings,
  type WebviewMessage,
} from './protocol.js';

/** `signal` si interrompe quando il webview che ha fatto la richiesta si chiude o si scarica. */
export type RequestHandler = (
  method: unknown,
  params: unknown,
  progress?: (event: ProgressEvent) => void,
  signal?: AbortSignal,
) => Promise<unknown>;

/** Id della vista nella barra delle attività (`contributes.views` in `package.json`). */
export const SIDEBAR_VIEW_ID = 'swapdeck.view';

export function makeNonce(): string {
  return randomBytes(18).toString('base64url');
}

/** HTML del webview: script e stile solo con nonce o dalla cartella dell'estensione. */
export function webviewHtml(opts: {
  cspSource: string;
  nonce: string;
  scriptUri: string;
  styleUri: string;
  lang: string;
  title: string;
}): string {
  const csp = [
    "default-src 'none'",
    `img-src ${opts.cspSource} data:`,
    `font-src ${opts.cspSource}`,
    `style-src ${opts.cspSource}`,
    `script-src 'nonce-${opts.nonce}'`,
    "base-uri 'none'",
    "form-action 'none'",
  ].join('; ');
  return `<!doctype html>
<html lang="${opts.lang}">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<meta name="viewport" content="width=device-width, initial-scale=1">
<link rel="stylesheet" href="${opts.styleUri}">
<title>${opts.title}</title>
</head>
<body>
<div id="root"></div>
<script nonce="${opts.nonce}" src="${opts.scriptUri}"></script>
</body>
</html>`;
}

/** Lingua scelta in SwapDeck (`language` della config comune); senza, quella di VS Code. */
let preferredLocale: ViewSettings['locale'] | undefined;

/** Imposta la lingua scelta in SwapDeck; `true` se è cambiata. */
export function setPreferredLocale(locale: ViewSettings['locale'] | undefined): boolean {
  if (locale === preferredLocale) return false;
  preferredLocale = locale;
  return true;
}

export function viewSettings(): ViewSettings {
  const kind = vscode.window.activeColorTheme.kind;
  const light =
    kind === vscode.ColorThemeKind.Light || kind === vscode.ColorThemeKind.HighContrastLight;
  return {
    locale: viewLocale(preferredLocale, vscode.env.language),
    theme: light ? 'light' : 'dark',
  };
}

/**
 * Tutti i webview aperti in questa finestra (vista laterale e scheda): HTML, messaggi e
 * diffusione dello stato. `onReady` riceve la funzione che scrive solo al webview appena pronto.
 */
export class WebviewHub implements vscode.Disposable {
  private readonly webviews = new Set<vscode.Webview>();
  /** Per webview: interrotto quando si chiude o si scarica (richieste lunghe come «Rifai login»). */
  private readonly lifetimes = new Map<vscode.Webview, AbortController>();
  private readonly disposables: vscode.Disposable[] = [];
  private readyCount = 0;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly handle: RequestHandler,
    private readonly onReady: (post: (message: HostMessage) => void) => void,
  ) {
    this.disposables.push(vscode.window.onDidChangeActiveColorTheme(() => this.refreshView()));
  }

  /** Cartella delle risorse del webview (unica radice permessa). */
  get root(): vscode.Uri {
    return vscode.Uri.joinPath(this.extensionUri, 'out', 'webview');
  }

  /** Webview che hanno caricato lo script e chiesto lo stato almeno una volta. */
  get readyWebviews(): number {
    return this.readyCount;
  }

  /** Prepara il webview (opzioni, HTML, messaggi); il `Disposable` lo stacca. */
  attach(webview: vscode.Webview): vscode.Disposable {
    const root = this.root;
    webview.options = { enableScripts: true, localResourceRoots: [root] };
    webview.html = webviewHtml({
      cspSource: webview.cspSource,
      nonce: makeNonce(),
      scriptUri: webview.asWebviewUri(vscode.Uri.joinPath(root, 'main.js')).toString(),
      styleUri: webview.asWebviewUri(vscode.Uri.joinPath(root, 'main.css')).toString(),
      lang: viewSettings().locale,
      title: 'SwapDeck',
    });
    this.webviews.add(webview);
    this.lifetimes.set(webview, new AbortController());
    const sub = webview.onDidReceiveMessage(
      (message: WebviewMessage) => void this.receive(webview, message),
    );
    return new vscode.Disposable(() => {
      sub.dispose();
      this.webviews.delete(webview);
      this.lifetimes.get(webview)?.abort();
      this.lifetimes.delete(webview);
    });
  }

  /** Lingua e tema attuali a tutti i webview: la UI si rimonta sulla scheda salvata. */
  refreshView(): void {
    this.post({ type: 'view', settings: viewSettings() });
  }

  /** A tutti i webview aperti. */
  post(message: HostMessage): void {
    for (const webview of this.webviews) void webview.postMessage(message);
  }

  private async receive(webview: vscode.Webview, message: WebviewMessage): Promise<void> {
    if (typeof message !== 'object' || message === null) return;
    const post = (m: HostMessage) => void webview.postMessage(m);
    if (message.type === 'ready') {
      this.readyCount++;
      post({ type: 'view', settings: viewSettings() });
      this.onReady(post);
      return;
    }
    if (message.type !== 'request' || !Number.isSafeInteger(message.id)) return;
    const id = message.id;
    try {
      const result = await this.handle(
        message.method,
        message.params,
        (event) => post({ type: 'progress', id, event }),
        this.lifetimes.get(webview)?.signal,
      );
      post({ type: 'response', id, ok: true, result });
    } catch (error) {
      post({ type: 'response', id, ok: false, error: toCswapError(error) });
    }
  }

  dispose(): void {
    for (const d of this.disposables) d.dispose();
    for (const lifetime of this.lifetimes.values()) lifetime.abort();
    this.lifetimes.clear();
    this.webviews.clear();
  }
}

/**
 * Vista SwapDeck nella barra delle attività: l'app completa alla larghezza della barra laterale.
 * Registrata con `retainContextWhenHidden` (in `extension.ts`): nascosta resta viva, così un «Rifai
 * login» in attesa del codice tiene link e campo; si annulla solo alla chiusura (`onDidDispose`).
 */
export class SidebarView implements vscode.WebviewViewProvider, vscode.Disposable {
  private view: vscode.WebviewView | undefined;
  private attached: vscode.Disposable | undefined;

  constructor(private readonly hub: WebviewHub) {}

  get isResolved(): boolean {
    return this.view !== undefined;
  }

  resolveWebviewView(view: vscode.WebviewView): void {
    this.attached?.dispose();
    this.view = view;
    this.attached = this.hub.attach(view.webview);
    view.onDidDispose(() => {
      if (this.view !== view) return;
      this.attached?.dispose();
      this.attached = undefined;
      this.view = undefined;
    });
  }

  dispose(): void {
    this.attached?.dispose();
  }
}

/** «Apri in una scheda»: la stessa app in un pannello dell'editor, uno solo per finestra. */
export class DashboardPanel implements vscode.Disposable {
  private panel: vscode.WebviewPanel | undefined;

  constructor(private readonly hub: WebviewHub) {}

  get isOpen(): boolean {
    return this.panel !== undefined;
  }

  show(): void {
    if (this.panel) {
      this.panel.reveal();
      return;
    }
    const panel = vscode.window.createWebviewPanel(
      'swapdeck.dashboard',
      'SwapDeck',
      vscode.ViewColumn.Active,
      { enableScripts: true, localResourceRoots: [this.hub.root], retainContextWhenHidden: true },
    );
    this.panel = panel;
    const attached = this.hub.attach(panel.webview);
    panel.onDidDispose(() => {
      attached.dispose();
      this.panel = undefined;
    });
  }

  dispose(): void {
    this.panel?.dispose();
  }
}
