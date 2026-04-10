/**
 * WebView preview panel for ChordPro files.
 *
 * Manages a `vscode.WebviewPanel` that renders the active ChordPro document as
 * HTML using `@chordsketch/wasm` loaded in the WebView context. Updates are
 * debounced at 300 ms to avoid flooding the WASM renderer on every keystroke.
 */

import * as vscode from 'vscode';
import * as crypto from 'crypto';
import * as path from 'path';

/** Message types sent from the extension host to the WebView. */
type ExtToWebview = { type: 'update'; text: string };

/** Message types received from the WebView in the extension host. */
type WebviewToExt = { type: 'ready' } | { type: 'error'; message: string };

/**
 * Type guard for messages received from the WebView.
 *
 * Validates the shape of `raw` before casting to `WebviewToExt` so that
 * field accesses are safe even if the WebView sends a malformed message.
 */
function isWebviewToExt(raw: unknown): raw is WebviewToExt {
  if (typeof raw !== 'object' || raw === null) {
    return false;
  }
  const r = raw as Record<string, unknown>;
  if (r['type'] === 'ready') {
    return true;
  }
  if (r['type'] === 'error') {
    return typeof r['message'] === 'string';
  }
  return false;
}

/** Debounce delay in milliseconds. */
const DEBOUNCE_MS = 300;

/** Tracks all open preview panels keyed by document URI string. */
const panels = new Map<string, PreviewPanel>();

/**
 * Opens or reveals the preview panel for the given document.
 *
 * If a panel already exists for the document it is revealed; otherwise a new
 * panel is created in the specified view column.
 */
export function createOrShow(
  context: vscode.ExtensionContext,
  document: vscode.TextDocument,
  column: vscode.ViewColumn,
): void {
  const key = document.uri.toString();
  const existing = panels.get(key);
  if (existing) {
    existing.reveal(column);
    return;
  }
  const panel = new PreviewPanel(context, document);
  panel.show(column);
  panels.set(key, panel);
}

/**
 * Notifies all open preview panels about a document change.
 * Called from the `onDidChangeTextDocument` handler in `extension.ts`.
 */
export function notifyDocumentChanged(event: vscode.TextDocumentChangeEvent): void {
  const key = event.document.uri.toString();
  const panel = panels.get(key);
  if (panel) {
    panel.scheduleUpdate(event.document.getText());
  }
}

/** Disposes all open preview panels. Called on extension deactivation. */
export function disposeAll(): void {
  for (const panel of panels.values()) {
    panel.dispose();
  }
  panels.clear();
}

/** Manages a single WebView preview panel for one ChordPro document. */
class PreviewPanel {
  private readonly panel: vscode.WebviewPanel;
  private readonly context: vscode.ExtensionContext;
  private readonly document: vscode.TextDocument;
  private debounceTimer: NodeJS.Timeout | undefined;
  private pendingText: string | undefined;
  /** Lazily created output channel for surfacing render errors from this panel. */
  private outputChannel: vscode.OutputChannel | undefined;

  constructor(context: vscode.ExtensionContext, document: vscode.TextDocument) {
    this.context = context;
    this.document = document;

    const fileName = path.basename(document.uri.fsPath);
    this.panel = vscode.window.createWebviewPanel(
      'chordsketchPreview',
      `Preview: ${fileName}`,
      vscode.ViewColumn.Active,
      {
        enableScripts: true,
        // Restrict the WebView to loading resources only from dist/webview/.
        localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview')],
        // Retain context across hide/show cycles (Phase A simplicity).
        retainContextWhenHidden: true,
      },
    );

    this.panel.webview.html = this.buildHtml();

    // Handle messages from the WebView.
    this.panel.webview.onDidReceiveMessage((raw: unknown) => {
      if (!isWebviewToExt(raw)) {
        // Unknown or malformed message — silently ignore.
        return;
      }
      if (raw.type === 'ready') {
        // WebView is ready — send the current document content.
        this.sendUpdate(this.document.getText());
      } else if (raw.type === 'error') {
        // The WebView surfaces render errors; they are also displayed inline
        // in the panel so we only log here and don't show a notification.
        // The channel lifetime tracks this panel — it is disposed in dispose().
        if (!this.outputChannel) {
          this.outputChannel = vscode.window.createOutputChannel('ChordSketch Preview');
        }
        this.outputChannel.appendLine(`Preview render error: ${raw.message}`);
      }
    });

    // Remove this panel from the map when the user closes it.
    this.panel.onDidDispose(() => {
      panels.delete(this.document.uri.toString());
    });
  }

  /** Shows the panel in the given column. */
  show(column: vscode.ViewColumn): void {
    this.panel.reveal(column);
  }

  /** Reveals the panel in the given column (no-op if already visible). */
  reveal(column: vscode.ViewColumn): void {
    this.panel.reveal(column);
  }

  /**
   * Schedules a debounced update with new document text.
   * Clears any pending timer before setting a new one.
   */
  scheduleUpdate(text: string): void {
    this.pendingText = text;
    if (this.debounceTimer !== undefined) {
      clearTimeout(this.debounceTimer);
    }
    this.debounceTimer = setTimeout(() => {
      this.debounceTimer = undefined;
      if (this.pendingText !== undefined) {
        this.sendUpdate(this.pendingText);
        this.pendingText = undefined;
      }
    }, DEBOUNCE_MS);
  }

  /** Sends an update message to the WebView immediately. */
  private sendUpdate(text: string): void {
    const msg: ExtToWebview = { type: 'update', text };
    void this.panel.webview.postMessage(msg);
  }

  /** Disposes the panel, its output channel, and any pending debounce timer. */
  dispose(): void {
    if (this.debounceTimer !== undefined) {
      clearTimeout(this.debounceTimer);
      this.debounceTimer = undefined;
    }
    this.outputChannel?.dispose();
    this.outputChannel = undefined;
    this.panel.dispose();
  }

  /**
   * Builds the WebView HTML.
   *
   * Uses a CSP nonce to allow only the bundled script. The WASM binary URI
   * is injected via a `<meta name="chordsketch-wasm-uri">` element so the
   * WebView script can read it at runtime. A `data-` attribute on
   * `<script type="module">` cannot be used because `document.currentScript`
   * is always `null` for ES module scripts (HTML spec).
   */
  private buildHtml(): string {
    const webview = this.panel.webview;

    // Stable random nonce for this session (regenerated on each HTML build).
    const nonce = crypto.randomBytes(16).toString('hex');

    const scriptUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this.context.extensionUri, 'dist', 'webview', 'preview.js'),
    );
    const wasmUri = webview.asWebviewUri(
      vscode.Uri.joinPath(
        this.context.extensionUri,
        'dist',
        'webview',
        'chordsketch_wasm_bg.wasm',
      ),
    );

    // cspSource includes the extension's own dist/webview/ origin.
    const csp = webview.cspSource;

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="
    default-src 'none';
    script-src 'nonce-${nonce}' 'wasm-unsafe-eval';
    style-src ${csp} 'unsafe-inline';
    img-src ${csp} data:;
    font-src ${csp};
    connect-src ${csp};
  ">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta name="chordsketch-wasm-uri" content="${wasmUri}">
  <title>ChordSketch Preview</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: var(--vscode-font-family, sans-serif); height: 100vh; display: flex; flex-direction: column; }
    #loading {
      padding: 1rem;
      color: var(--vscode-descriptionForeground);
      font-style: italic;
    }
    #error {
      display: none;
      padding: 0.75rem 1rem;
      background: var(--vscode-inputValidation-errorBackground, #f2dede);
      border-left: 4px solid var(--vscode-inputValidation-errorBorder, #c00);
      color: var(--vscode-inputValidation-errorForeground, #c00);
      font-size: 0.875rem;
      white-space: pre-wrap;
      word-break: break-word;
    }
    #preview-frame {
      flex: 1;
      border: none;
      width: 100%;
      display: none;
      background: white;
    }
  </style>
</head>
<body>
  <div id="loading">Initializing ChordSketch preview…</div>
  <div id="error"></div>
  <iframe
    id="preview-frame"
    sandbox="allow-popups allow-popups-to-escape-sandbox"
    title="ChordPro preview"
  ></iframe>
  <script nonce="${nonce}" src="${scriptUri}" type="module"></script>
</body>
</html>`;
  }
}
