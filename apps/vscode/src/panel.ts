/** Pannello con la dashboard di `@swapdeck/ui`: CSP con nonce, risorse solo da `out/webview`. */
import { randomBytes } from 'node:crypto';
import * as vscode from 'vscode';
import { toCswapError } from './cli.js';
import type { HostMessage, ViewSettings, WebviewMessage } from './protocol.js';

export type RequestHandler = (method: unknown, params: unknown) => Promise<unknown>;

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

/** Un solo pannello per finestra; le richieste passano dal gestore dell'host. */
export class DashboardPanel implements vscode.Disposable {
  private panel: vscode.WebviewPanel | undefined;
  private readonly disposables: vscode.Disposable[] = [];

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly handle: RequestHandler,
    private readonly onReady: () => void,
  ) {
    this.disposables.push(
      vscode.window.onDidChangeActiveColorTheme(() =>
        this.post({ type: 'view', settings: viewSettings() }),
      ),
    );
  }

  get isOpen(): boolean {
    return this.panel !== undefined;
  }

  show(): void {
    if (this.panel) {
      this.panel.reveal();
      return;
    }
    const root = vscode.Uri.joinPath(this.extensionUri, 'out', 'webview');
    const panel = vscode.window.createWebviewPanel(
      'swapdeck.dashboard',
      'SwapDeck',
      vscode.ViewColumn.Active,
      { enableScripts: true, localResourceRoots: [root], retainContextWhenHidden: false },
    );
    this.panel = panel;
    const webview = panel.webview;
    webview.html = webviewHtml({
      cspSource: webview.cspSource,
      nonce: makeNonce(),
      scriptUri: webview.asWebviewUri(vscode.Uri.joinPath(root, 'main.js')).toString(),
      styleUri: webview.asWebviewUri(vscode.Uri.joinPath(root, 'main.css')).toString(),
      lang: viewSettings().locale,
      title: 'SwapDeck',
    });
    const sub = webview.onDidReceiveMessage(
      (message: WebviewMessage) => void this.receive(message),
    );
    panel.onDidDispose(() => {
      sub.dispose();
      this.panel = undefined;
    });
  }

  post(message: HostMessage): void {
    void this.panel?.webview.postMessage(message);
  }

  private async receive(message: WebviewMessage): Promise<void> {
    if (typeof message !== 'object' || message === null) return;
    if (message.type === 'ready') {
      this.post({ type: 'view', settings: viewSettings() });
      this.onReady();
      return;
    }
    if (message.type !== 'request' || !Number.isSafeInteger(message.id)) return;
    try {
      const result = await this.handle(message.method, message.params);
      this.post({ type: 'response', id: message.id, ok: true, result });
    } catch (error) {
      this.post({ type: 'response', id: message.id, ok: false, error: toCswapError(error) });
    }
  }

  dispose(): void {
    this.panel?.dispose();
    for (const d of this.disposables) d.dispose();
  }
}
