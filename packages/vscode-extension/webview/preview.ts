/**
 * WebView-side script for the ChordSketch preview panel.
 *
 * Runs in the sandboxed WebView context (browser environment). Initialises
 * the `@chordsketch/wasm` module using the WASM binary URI provided by the
 * extension host, then listens for document-update messages and renders them
 * using the active view mode (HTML or plain text).
 *
 * The WASM URI is injected by the extension host as
 * `<meta name="chordsketch-wasm-uri" content="...">`. A `data-` attribute on
 * the `<script>` element cannot be used because `document.currentScript` is
 * always `null` for `type="module"` scripts (HTML spec).
 */

import init, { render_html, render_text } from '@chordsketch/wasm';

/** VS Code WebView API acquired from the global injected by the host. */
declare function acquireVsCodeApi(): {
  postMessage(msg: unknown): void;
  getState(): unknown;
  setState(state: unknown): void;
};

const vscode = acquireVsCodeApi();

/** View mode for the preview panel. */
type ViewMode = 'html' | 'text';

/** Persisted panel state saved and restored via the VS Code WebView API. */
interface PanelState {
  mode?: ViewMode;
}

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

const toolbar = document.getElementById('toolbar') as HTMLDivElement;
const loadingEl = document.getElementById('loading') as HTMLDivElement;
const errorEl = document.getElementById('error') as HTMLDivElement;
const previewFrame = document.getElementById('preview-frame') as HTMLIFrameElement;
const textFrame = document.getElementById('text-frame') as HTMLPreElement;
const btnHtml = document.getElementById('btn-html') as HTMLButtonElement;
const btnText = document.getElementById('btn-text') as HTMLButtonElement;

/** Currently active view mode. Loaded from persisted state in `main()`. */
let viewMode: ViewMode = 'html';

/**
 * Most recently rendered source text.
 *
 * Kept so that switching view modes re-renders the existing content without
 * waiting for the next document-change message from the extension host.
 */
let lastText = '';

function showError(msg: string): void {
  errorEl.textContent = msg;
  errorEl.style.display = 'block';
  previewFrame.style.display = 'none';
  textFrame.style.display = 'none';
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

/** Syncs toggle button active classes to the current `viewMode`. */
function syncButtonStates(): void {
  btnHtml.classList.toggle('active', viewMode === 'html');
  btnText.classList.toggle('active', viewMode === 'text');
}

/**
 * Renders the given ChordPro source text according to the active view mode.
 *
 * In HTML mode, `render_html` is called and the output is loaded into the
 * sandboxed iframe via `srcdoc`. In plain text mode, `render_text` is called
 * and the output is set as `textContent` of the `<pre>` element (safe — no
 * HTML parsing occurs).
 */
function renderPreview(text: string): void {
  lastText = text;

  if (!text.trim()) {
    hideError();
    if (viewMode === 'html') {
      previewFrame.srcdoc = '';
      previewFrame.style.display = 'block';
      textFrame.style.display = 'none';
    } else {
      textFrame.textContent = '';
      textFrame.style.display = 'block';
      previewFrame.style.display = 'none';
    }
    return;
  }

  try {
    if (viewMode === 'html') {
      // TODO(Phase B): call render_html_with_options with transpose options instead of render_html.
      const html = render_html(text);
      hideError();
      previewFrame.srcdoc = wrapHtml(html);
      previewFrame.style.display = 'block';
      textFrame.style.display = 'none';
    } else {
      const plain = render_text(text);
      hideError();
      // textContent assignment is safe: no HTML parsing, no XSS risk.
      textFrame.textContent = plain;
      textFrame.style.display = 'block';
      previewFrame.style.display = 'none';
    }
  } catch (e) {
    showError(formatError(e));
  }
}

/**
 * Switches to the given view mode and immediately re-renders the current text.
 *
 * The chosen mode is persisted via `vscode.setState` so it survives the
 * WebView being hidden and re-shown (`retainContextWhenHidden` is set).
 * Called only after WASM has successfully loaded.
 */
function setViewMode(mode: ViewMode): void {
  if (mode === viewMode) {
    return; // No-op: avoid redundant WASM call and iframe flicker.
  }
  viewMode = mode;
  // Spread the existing state before writing back so that any fields added to
  // PanelState in a future PR are not silently wiped on every mode toggle.
  vscode.setState({ ...(vscode.getState() as PanelState | null) ?? {}, mode } satisfies PanelState);
  syncButtonStates();

  renderPreview(lastText);
}

async function main(): Promise<void> {
  // Restore the persisted view mode so the user's choice survives hide/show.
  const saved = vscode.getState() as PanelState | null;
  if (saved?.mode === 'html' || saved?.mode === 'text') {
    viewMode = saved.mode;
    syncButtonStates();
  }

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

  // Enable the toolbar only after WASM is ready so clicking buttons before
  // init is not possible. The CSS sets pointer-events:none on the toolbar
  // by default; removing the class re-enables it.
  toolbar.classList.remove('disabled');

  // Register toggle button handlers after WASM is ready.
  btnHtml.addEventListener('click', () => setViewMode('html'));
  btnText.addEventListener('click', () => setViewMode('text'));

  // Listen for messages from the extension host.
  window.addEventListener('message', (event: MessageEvent) => {
    if (!isExtToWebview(event.data)) {
      // Unknown or malformed message — silently ignore.
      return;
    }
    // isExtToWebview guarantees type === 'update'; no inner dispatch needed.
    renderPreview(event.data.text);
  });

  // Tell the extension host that the WebView is ready to receive content.
  vscode.postMessage({ type: 'ready' });
}

void main();
