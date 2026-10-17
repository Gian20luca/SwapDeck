/**
 * Webview con l'app completa di `@swapdeck/ui`, in due contenitori: la vista della barra laterale
 * (`SidebarView`) e la scheda dell'editor (`DashboardPanel`). Stesso HTML, CSP con nonce, risorse
 * solo da `out/webview` e stesso gestore delle richieste (`WebviewHub`).
 */
import { randomBytes } from 'node:crypto';
import * as vscode from 'vscode';
import { toCswapError } from './cli.js';
import type { SetupProgress } from '@swapdeck/types';
import type { HostMessage, ViewSettings, WebviewMessage } from './protocol.js';

export type RequestHandler = (
  method: unknown,
  params: unknown,
  progress?: (event: SetupProgress) => void,
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

export function viewSettings(): ViewSettings {
  const kind = vscode.window.activeColorTheme.kind;
  const light =
    kind === vscode.ColorThemeKind.Light || kind === vscode.ColorThemeKind.HighContrastLight;
  return {
    locale: vscode.env.language.toLowerCase().startsWith('it') ? 'it' : 'en',
    theme: light ? 'light' : 'dark',
  };
}

/**
 * Tutti i webview aperti in questa finestra (vista laterale e scheda): HTML, messaggi e
 * diffusione dello stato. `onReady` riceve la funzione che scrive solo al webview appena pronto.
 */
export class WebviewHub implements vscode.Disposable {
  private readonly webviews = new Set<vscode.Webview>();
  private readonly disposables: vscode.Disposable[] = [];
  private readyCount = 0;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly handle: RequestHandler,
    private readonly onReady: (post: (message: HostMessage) => void) => void,
  ) {
    this.disposables.push(
      vscode.window.onDidChangeActiveColorTheme(() =>
        this.post({ type: 'view', settings: viewSettings() }),
      ),
    );
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
    const sub = webview.onDidReceiveMessage(
      (message: WebviewMessage) => void this.receive(webview, message),
    );
    return new vscode.Disposable(() => {
      sub.dispose();
      this.webviews.delete(webview);
    });
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
      const result = await this.handle(message.method, message.params, (event) =>
        post({ type: 'progress', id, event }),
      );
      post({ type: 'response', id, ok: true, result });
    } catch (error) {
      post({ type: 'response', id, ok: false, error: toCswapError(error) });
    }
  }

  dispose(): void {
    for (const d of this.disposables) d.dispose();
    this.webviews.clear();
  }
}

/**
 * Vista SwapDeck nella barra delle attività: l'app completa alla larghezza della barra laterale.
 * Senza `retainContextWhenHidden`: nascosta si scarica, e alla riapertura il webview riparte dalla
 * scheda salvata con `setState` e chiede di nuovo lo stato all'host.
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
      { enableScripts: true, localResourceRoots: [this.hub.root], retainContextWhenHidden: false },
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
