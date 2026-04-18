/**
 * WebView preview panel for ChordPro files.
 *
 * Manages a `vscode.WebviewPanel` that renders the active ChordPro document as
 * HTML using `@chordsketch/wasm` loaded in the WebView context. Updates are
 * debounced at 300 ms to avoid flooding the WASM renderer on every keystroke.
 *
 * Panel state (view mode, transpose offset, source document URI) is persisted
 * via the WebView's `vscode.setState` API so that preview tabs are restored
 * across VS Code restarts by [`ChordSketchPreviewSerializer`].
 */

import * as vscode from 'vscode';
import * as crypto from 'crypto';
import * as path from 'path';
import { resolveDefaultMode } from './config.js';
import { escapeHtmlAttr, parseSerializedState } from './preview-helpers.js';

/** Message types sent from the extension host to the WebView. */
type ExtToWebview = { type: 'update'; text: string } | { type: 'transpose'; delta: 1 | -1 };

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

/** WebView-panel `viewType` identifier — also the key under which VS Code
 *  looks up the registered serializer. */
export const PREVIEW_VIEW_TYPE = 'chordsketchPreview';

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
  const panel = PreviewPanel.createNew(context, document, column);
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

/**
 * Sends a transpose delta to the preview panel for the given document URI.
 *
 * Looks up the panel by URI string (the key used in `panels`). No-op if no
 * panel is currently open for that document — the command degrades gracefully
 * when no preview is open.
 */
export function notifyTranspose(documentUri: string, delta: 1 | -1): void {
  const panel = panels.get(documentUri);
  if (panel) {
    panel.transpose(delta);
  }
}

/** Disposes all open preview panels. Called on extension deactivation. */
export function disposeAll(): void {
  for (const panel of panels.values()) {
    panel.dispose();
  }
  panels.clear();
}

/**
 * Registers a WebView panel serializer so that preview tabs survive VS Code
 * restarts. Must be called once from `activate()`.
 *
 * On deserialization the source `vscode.TextDocument` is looked up via the
 * persisted `documentUri`. If the document is no longer available (file moved
 * or deleted) the panel renders a friendly message and is left open so the
 * user can close it without error.
 */
export function registerPreviewSerializer(context: vscode.ExtensionContext): vscode.Disposable {
  return vscode.window.registerWebviewPanelSerializer(PREVIEW_VIEW_TYPE, {
    async deserializeWebviewPanel(panel: vscode.WebviewPanel, state: unknown): Promise<void> {
      const parsed = parseSerializedState(state);
      if (!parsed) {
        renderUnavailableMessage(
          panel,
          'The previewed document could not be determined from the restored state.',
        );
        return;
      }

      let documentUri: vscode.Uri;
      try {
        documentUri = vscode.Uri.parse(parsed.documentUri, /* strict */ true);
      } catch {
        renderUnavailableMessage(
          panel,
          `Could not parse the restored preview source URI: ${parsed.documentUri}`,
        );
        return;
      }

      let document: vscode.TextDocument;
      try {
        document = await vscode.workspace.openTextDocument(documentUri);
      } catch {
        renderUnavailableMessage(
          panel,
          `The file previously previewed could no longer be opened:\n${documentUri.fsPath}`,
        );
        return;
      }

      // Make sure localResourceRoots still matches the extension install path —
      // VS Code restores the panel options but we re-apply to stay safe.
      panel.webview.options = {
        enableScripts: true,
        localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview')],
      };

      PreviewPanel.restore(context, document, panel);
    },
  });
}

function renderUnavailableMessage(panel: vscode.WebviewPanel, message: string): void {
  panel.webview.options = { enableScripts: false };
  // Reuse the same escape routine as `<meta>` attribute injection. Running
  // the attribute-level escape on body content is safe (and strictly
  // stronger than strictly necessary): it escapes `&`, `<`, `>`, `"`, `'`,
  // none of which can re-introduce markup when written into a `<p>` text
  // node. Line breaks in the input become `<br>` so multi-line messages
  // stay readable.
  const escaped = escapeHtmlAttr(message).replace(/\n/g, '<br>');
  panel.webview.html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline';">
  <title>ChordSketch Preview</title>
  <style>
    body {
      font-family: var(--vscode-font-family, sans-serif);
      padding: 1.5rem;
      color: var(--vscode-descriptionForeground, #888);
      line-height: 1.5;
    }
  </style>
</head>
<body>
  <p>${escaped}</p>
  <p>You can close this tab safely.</p>
</body>
</html>`;
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
  /** Set to true once the panel is disposed so stale timer callbacks are no-ops. */
  private disposed = false;

  private constructor(
    context: vscode.ExtensionContext,
    document: vscode.TextDocument,
    panel: vscode.WebviewPanel,
  ) {
    this.context = context;
    this.document = document;
    this.panel = panel;

    // `buildHtml` is called on every (re)attach so the injected
    // `<meta name="chordsketch-document-uri">` matches this instance's
    // document; the WebView persists the URI via `vscode.setState` on
    // startup so the serializer can find it after a restart.
    this.panel.webview.html = this.buildHtml();
    this.wireMessageHandling();
    this.wireDisposal();
  }

  /** Creates a fresh panel for `document` and displays it in `column`. */
  static createNew(
    context: vscode.ExtensionContext,
    document: vscode.TextDocument,
    column: vscode.ViewColumn,
  ): PreviewPanel {
    const fileName = path.basename(document.uri.fsPath);
    const panel = vscode.window.createWebviewPanel(
      PREVIEW_VIEW_TYPE,
      `Preview: ${fileName}`,
      column,
      {
        enableScripts: true,
        // Restrict the WebView to loading resources only from dist/webview/.
        localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview')],
        // Retain context across hide/show cycles (Phase A simplicity).
        retainContextWhenHidden: true,
      },
    );
    return new PreviewPanel(context, document, panel);
  }

  /**
   * Reattaches to a `vscode.WebviewPanel` handed back by VS Code's
   * `WebviewPanelSerializer`. The panel already exists — we rebuild its HTML
   * and re-register message and dispose handlers.
   */
  static restore(
    context: vscode.ExtensionContext,
    document: vscode.TextDocument,
    panel: vscode.WebviewPanel,
  ): PreviewPanel {
    const restored = new PreviewPanel(context, document, panel);
    panels.set(document.uri.toString(), restored);
    return restored;
  }

  private wireMessageHandling(): void {
    this.panel.webview.onDidReceiveMessage((raw: unknown) => {
      if (this.disposed) {
        return;
      }
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
        // The channel lifetime tracks this panel — disposed in onDidDispose (tab
        // close) and in dispose() (extension deactivation via disposeAll()).
        if (!this.outputChannel) {
          this.outputChannel = vscode.window.createOutputChannel('ChordSketch Preview');
        }
        this.outputChannel.appendLine(`Preview render error: ${raw.message}`);
      }
    });
  }

  private wireDisposal(): void {
    // Remove this panel from the map when the user closes it, and clean up
    // associated resources immediately so they do not outlive the panel tab.
    // Setting disposed=true before clearTimeout guards against a stale timer
    // callback that may already be queued when onDidDispose fires.
    this.panel.onDidDispose(() => {
      this.disposed = true;
      panels.delete(this.document.uri.toString());
      if (this.debounceTimer !== undefined) {
        clearTimeout(this.debounceTimer);
        this.debounceTimer = undefined;
      }
      this.outputChannel?.dispose();
      this.outputChannel = undefined;
    });
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
      if (this.pendingText !== undefined && !this.disposed) {
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

  /**
   * Sends a transpose delta to the WebView.
   *
   * The WebView applies the delta to its own in-memory transpose state (clamped
   * to [−11, +11]) and re-renders. No-op if the panel is already disposed.
   */
  transpose(delta: 1 | -1): void {
    if (this.disposed) {
      return;
    }
    const msg: ExtToWebview = { type: 'transpose', delta };
    void this.panel.webview.postMessage(msg);
  }

  /** Disposes the panel, its output channel, and any pending debounce timer. */
  dispose(): void {
    this.disposed = true;
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
   * Uses a CSP nonce to allow only the bundled script. Extension-provided
   * values are injected via `<meta>` elements so the WebView script can read
   * them at runtime:
   *   - `chordsketch-wasm-uri`: the WASM binary URI (VS Code WebView URI)
   *   - `chordsketch-default-mode`: the `chordsketch.preview.defaultMode`
   *     setting value, used as the initial view mode when no persisted state
   *     exists (only `"html"` and `"text"` are accepted; anything else falls
   *     back to `"html"` in the WebView script).
   *   - `chordsketch-document-uri`: the source-document URI. The WebView
   *     persists this via `vscode.setState` on startup so that
   *     [`registerPreviewSerializer`] can reopen the correct document after
   *     a VS Code restart.
   *
   * A `data-` attribute on `<script type="module">` cannot be used because
   * `document.currentScript` is always `null` for ES module scripts (HTML spec).
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

    // Read the default mode setting and clamp to the known-valid set so that
    // an out-of-range config value cannot affect the WebView's behaviour.
    const rawMode = vscode.workspace
      .getConfiguration('chordsketch')
      .get<string>('preview.defaultMode', 'html');
    const defaultMode = resolveDefaultMode(rawMode);

    // Escape the document URI for safe interpolation into a `content`
    // attribute. The URI is derived from VS Code's own `TextDocument.uri`
    // so it is already well-formed, but escaping `&`/`<`/`>`/`"` defends
    // against pathological file names that could otherwise break out of
    // the attribute.
    const documentUriAttr = escapeHtmlAttr(this.document.uri.toString());

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
  <meta name="chordsketch-default-mode" content="${defaultMode}">
  <meta name="chordsketch-document-uri" content="${documentUriAttr}">
  <title>ChordSketch Preview</title>
  <style>
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body { font-family: var(--vscode-font-family, sans-serif); height: 100vh; display: flex; flex-direction: column; }
    #toolbar {
      display: flex;
      align-items: center;
      gap: 2px;
      padding: 4px 8px;
      background: var(--vscode-editor-background, #1e1e1e);
      border-bottom: 1px solid var(--vscode-editorGroup-border, #444);
      flex-shrink: 0;
    }
    #toolbar .view-btn {
      background: transparent;
      border: 1px solid var(--vscode-button-secondaryBackground, #555);
      color: var(--vscode-foreground, #ccc);
      padding: 2px 10px;
      cursor: pointer;
      font-size: 0.75rem;
      border-radius: 3px;
      font-family: inherit;
    }
    #toolbar .view-btn.active {
      background: var(--vscode-button-background, #0078d4);
      color: var(--vscode-button-foreground, #fff);
      border-color: var(--vscode-button-background, #0078d4);
    }
    #toolbar .view-btn:not(.active):hover {
      background: var(--vscode-button-secondaryHoverBackground, #3a3a3a);
    }
    .toolbar-separator {
      width: 1px;
      height: 16px;
      background: var(--vscode-editorGroup-border, #444);
      margin: 0 6px;
      flex-shrink: 0;
    }
    #toolbar .transpose-btn {
      background: transparent;
      border: 1px solid var(--vscode-button-secondaryBackground, #555);
      color: var(--vscode-foreground, #ccc);
      padding: 2px 7px;
      cursor: pointer;
      font-size: 0.85rem;
      border-radius: 3px;
      font-family: inherit;
      line-height: 1;
    }
    #toolbar .transpose-btn:hover {
      background: var(--vscode-button-secondaryHoverBackground, #3a3a3a);
    }
    #transpose-label {
      font-size: 0.75rem;
      color: var(--vscode-foreground, #ccc);
      min-width: 2.5rem;
      text-align: center;
      font-variant-numeric: tabular-nums;
    }
    /* Toolbar is disabled until WASM finishes loading so that clicking
       buttons before init completes is not possible. The script removes
       the 'disabled' class after a successful init(). */
    #toolbar.disabled {
      pointer-events: none;
      opacity: 0.4;
    }
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
    #text-frame {
      flex: 1;
      display: none;
      padding: 1.5rem;
      overflow: auto;
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: var(--vscode-editor-font-size, 13px);
      line-height: 1.5;
      background: var(--vscode-editor-background, #1e1e1e);
      color: var(--vscode-editor-foreground, #d4d4d4);
      white-space: pre;
      word-break: normal;
    }
  </style>
</head>
<body>
  <div id="toolbar" class="disabled">
    <button id="btn-html" class="view-btn active" title="HTML preview">HTML</button>
    <button id="btn-text" class="view-btn" title="Plain text preview">Plain text</button>
    <span class="toolbar-separator" role="separator"></span>
    <button id="btn-transpose-down" class="transpose-btn" title="Transpose down one semitone">−</button>
    <span id="transpose-label" aria-live="polite" aria-label="Transpose offset" title="Semitone transposition offset">±0</span>
    <button id="btn-transpose-up" class="transpose-btn" title="Transpose up one semitone">+</button>
  </div>
  <div id="loading">Initializing ChordSketch preview…</div>
  <div id="error"></div>
  <iframe
    id="preview-frame"
    sandbox="allow-popups allow-popups-to-escape-sandbox"
    title="ChordPro preview"
  ></iframe>
  <pre id="text-frame" aria-label="Plain text preview"></pre>
  <script nonce="${nonce}" src="${scriptUri}" type="module"></script>
</body>
</html>`;
  }
}

