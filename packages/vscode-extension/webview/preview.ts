/**
 * WebView-side script for the ChordSketch preview panel.
 *
 * Runs in the sandboxed WebView context (browser environment). Initialises
 * the `@chordsketch/wasm` module using the WASM binary URI provided by the
 * extension host, then listens for document-update messages and renders them
 * as HTML via the iframe.
 *
 * The WASM URI is injected by the extension host as
 * `<meta name="chordsketch-wasm-uri" content="...">`. A `data-` attribute on
 * the `<script>` element cannot be used because `document.currentScript` is
 * always `null` for `type="module"` scripts (HTML spec).
 */

import init, { render_html } from '@chordsketch/wasm';

/** VS Code WebView API acquired from the global injected by the host. */
declare function acquireVsCodeApi(): {
  postMessage(msg: unknown): void;
  getState(): unknown;
  setState(state: unknown): void;
};

const vscode = acquireVsCodeApi();

/** Message types received from the extension host. */
type ExtToWebview = { type: 'update'; text: string };

/**
 * Type guard for messages received from the extension host.
 *
 * Validates the shape of `event.data` before field access so that unknown
 * or malformed messages are silently ignored rather than causing TypeErrors.
 */
function isExtToWebview(raw: unknown): raw is ExtToWebview {
  if (typeof raw !== 'object' || raw === null) {
    return false;
  }
  const r = raw as Record<string, unknown>;
  return r['type'] === 'update' && typeof r['text'] === 'string';
}

const loadingEl = document.getElementById('loading') as HTMLDivElement;
const errorEl = document.getElementById('error') as HTMLDivElement;
const previewFrame = document.getElementById('preview-frame') as HTMLIFrameElement;

function showError(msg: string): void {
  errorEl.textContent = msg;
  errorEl.style.display = 'block';
  previewFrame.style.display = 'none';
  vscode.postMessage({ type: 'error', message: msg });
}

function hideError(): void {
  errorEl.style.display = 'none';
  errorEl.textContent = '';
}

/**
 * Formats a thrown value into a readable error message.
 *
 * Mirrors the `formatError` helper in `packages/playground/src/main.ts`
 * (see #1060, #1087): prefers `.message` from Error instances to avoid
 * `[object Object]` for structured JsError objects with line/col info.
 */
function formatError(e: unknown): string {
  if (e instanceof Error) {
    return e.message;
  }
  return String(e);
}

/**
 * Wraps rendered HTML body content with baseline CSS.
 *
 * Mirrors `wrapHtml()` in `packages/playground/src/main.ts:96-117`.
 * The rendered HTML from `chordsketch-render-html` contains only the body
 * content (sections, chords-above-lyrics, chord diagrams) without a full
 * document wrapper.
 */
function wrapHtml(body: string): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<style>
  body {
    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    padding: 1.5rem;
    line-height: 1.6;
    color: #333;
  }
  .chord { color: #e94560; font-weight: bold; }
  h1 { font-size: 1.4rem; margin-bottom: 0.25rem; }
  h2 { font-size: 1.1rem; color: #666; margin-bottom: 1rem; }
  section { margin-bottom: 1rem; }
  .song-separator { border-top: 2px solid #ddd; margin: 2rem 0; }
</style>
</head>
<body>${body}</body>
</html>`;
}

/** Renders the given ChordPro source text into the preview iframe. */
function renderPreview(text: string): void {
  if (!text.trim()) {
    hideError();
    previewFrame.srcdoc = '';
    previewFrame.style.display = 'block';
    return;
  }

  try {
    // TODO(Phase B): call render_html_with_options with transpose options instead of render_html.
    const html = render_html(text);
    hideError();
    previewFrame.srcdoc = wrapHtml(html);
    previewFrame.style.display = 'block';
  } catch (e) {
    showError(formatError(e));
  }
}

async function main(): Promise<void> {
  // Read the WASM binary URI injected by the extension host.
  // A <meta name="chordsketch-wasm-uri"> is used instead of a data- attribute
  // on the <script> tag because document.currentScript is null for ES modules.
  const wasmUri =
    document.querySelector<HTMLMetaElement>('meta[name="chordsketch-wasm-uri"]')?.content ?? '';

  try {
    if (wasmUri) {
      await init(wasmUri);
    } else {
      await init();
    }
  } catch (e) {
    showError(`Failed to initialize ChordSketch WASM: ${formatError(e)}`);
    return;
  }

  loadingEl.style.display = 'none';

  // Listen for messages from the extension host.
  window.addEventListener('message', (event: MessageEvent) => {
    if (!isExtToWebview(event.data)) {
      // Unknown or malformed message — silently ignore.
      return;
    }
    if (event.data.type === 'update') {
      renderPreview(event.data.text);
    }
  });

  // Tell the extension host that the WebView is ready to receive content.
  vscode.postMessage({ type: 'ready' });
}

void main();
