/**
 * WebView-side script for the ChordSketch preview panel.
 *
 * Runs in the sandboxed WebView context (browser environment). Initialises
 * the `@chordsketch/wasm` module using the WASM binary URI provided by the
 * extension host, then listens for document-update messages and renders them
 * using the active view mode (HTML or plain text) and transpose setting.
 *
 * The WASM URI is injected by the extension host as
 * `<meta name="chordsketch-wasm-uri" content="...">`. A `data-` attribute on
 * the `<script>` element cannot be used because `document.currentScript` is
 * always `null` for `type="module"` scripts (HTML spec).
 */

import init, { render_html_with_options, render_text_with_options } from '@chordsketch/wasm';

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
  /** Semitone transposition offset; clamped to [-11, +11] by adjustTranspose. */
  transpose?: number;
  /**
   * URI string of the source document this panel is previewing.
   *
   * Written on first run so that VS Code's `WebviewPanelSerializer` can look
   * up the document when restoring the panel after a restart. See
   * `registerPreviewSerializer` in `../src/preview.ts`.
   */
  documentUri?: string;
}

/** Message types received from the extension host. */
type ExtToWebview =
  | { type: 'update'; text: string }
  | { type: 'transpose'; delta: 1 | -1 };

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
  if (r['type'] === 'update') {
    return typeof r['text'] === 'string';
  }
  if (r['type'] === 'transpose') {
    return r['delta'] === 1 || r['delta'] === -1;
  }
  return false;
}

const toolbar = document.getElementById('toolbar') as HTMLDivElement;
const loadingEl = document.getElementById('loading') as HTMLDivElement;
const errorEl = document.getElementById('error') as HTMLDivElement;
const previewFrame = document.getElementById('preview-frame') as HTMLIFrameElement;
const textFrame = document.getElementById('text-frame') as HTMLPreElement;
const btnHtml = document.getElementById('btn-html') as HTMLButtonElement;
const btnText = document.getElementById('btn-text') as HTMLButtonElement;
const btnTransposeDown = document.getElementById('btn-transpose-down') as HTMLButtonElement;
const btnTransposeUp = document.getElementById('btn-transpose-up') as HTMLButtonElement;
const transposeLabel = document.getElementById('transpose-label') as HTMLSpanElement;

/** Currently active view mode. Loaded from persisted state in `main()`. */
let viewMode: ViewMode = 'html';

/**
 * Current semitone transposition offset.
 *
 * Any integer value is accepted — the WASM renderer reduces it modulo 12
 * internally (same behaviour as the CLI `--transpose` flag).
 */
let transpose = 0;

/**
 * Most recently rendered source text.
 *
 * Kept so that switching view modes or adjusting transpose re-renders the
 * existing content without waiting for the next document-change message.
 */
let lastText = '';

/**
 * Set to `true` once `init()` resolves successfully.
 *
 * Guards `renderPreview` from calling WASM exports before the module is
 * initialised. The `scheduleUpdate` debounce timer in the extension host can
 * fire an `update` message 300 ms after a keystroke — before WASM has
 * finished loading on a slow connection. Without this guard the WASM call
 * would throw and show a spurious error overlay. With it, the text is stored
 * in `lastText` and the correct render happens when the extension host sends
 * the next `update` after receiving `ready`.
 */
let wasmReady = false;

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

/** Formats the transpose value for the toolbar label (e.g. `±0`, `+3`, `−2`). */
function formatTranspose(t: number): string {
  if (t === 0) return '±0';
  // Use U+2212 MINUS SIGN to match the − character used on the decrement button.
  return t > 0 ? `+${t}` : `\u2212${Math.abs(t)}`;
}

/**
 * Returns a validated copy of the persisted WebView state.
 *
 * `vscode.getState()` returns `unknown`; this function narrows the result to a
 * well-typed `PanelState` with each field individually validated, so that a
 * corrupted or forward-incompatible stored value cannot bypass type-level checks.
 */
function safeGetState(): PanelState {
  const raw = vscode.getState() as Record<string, unknown> | null;
  const result: PanelState = {};
  if (raw?.['mode'] === 'html' || raw?.['mode'] === 'text') {
    result.mode = raw['mode'] as ViewMode;
  }
  if (typeof raw?.['transpose'] === 'number' && Number.isFinite(raw['transpose'])) {
    result.transpose = Math.max(-11, Math.min(11, raw['transpose'] as number));
  }
  if (typeof raw?.['documentUri'] === 'string' && raw['documentUri'].length > 0) {
    result.documentUri = raw['documentUri'];
  }
  return result;
}

/**
 * Renders the given ChordPro source text according to the active view mode
 * and current transpose offset.
 *
 * In HTML mode, `render_html_with_options` is called and the output is loaded
 * into the sandboxed iframe via `srcdoc`. In plain text mode,
 * `render_text_with_options` is called and the output is set as `textContent`
 * of the `<pre>` element (safe — no HTML parsing occurs).
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

  // Guard: do not call WASM exports until init() has completed. An 'update'
  // message can arrive before WASM is ready via the 300 ms scheduleUpdate
  // debounce timer in the extension host. Storing the text in lastText (above)
  // is still correct — the extension host re-sends it after receiving 'ready'.
  if (!wasmReady) {
    return;
  }

  const options = { transpose };

  try {
    if (viewMode === 'html') {
      const html = render_html_with_options(text, options);
      hideError();
      previewFrame.srcdoc = wrapHtml(html);
      previewFrame.style.display = 'block';
      textFrame.style.display = 'none';
    } else {
      const plain = render_text_with_options(text, options);
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
  vscode.setState({ ...safeGetState(), mode } satisfies PanelState);
  syncButtonStates();

  renderPreview(lastText);
}

/**
 * Adjusts the transpose offset by `delta` semitones and re-renders.
 *
 * The offset is clamped to [-11, +11]; values outside this range produce
 * the same chord output since the renderer reduces modulo 12 internally.
 * The clamp prevents the label from growing without bound on repeated clicks.
 *
 * May be called before WASM has loaded (e.g., via a `transpose` message sent
 * by the extension host during the init window). In that case `renderPreview`
 * exits early at the `wasmReady` guard without calling any WASM export; the
 * updated `transpose` value is applied on the next render after init completes.
 */
function adjustTranspose(delta: -1 | 1): void {
  const next = transpose + delta;
  // Clamp to [-11, +11]: one full chromatic octave in each direction.
  transpose = Math.max(-11, Math.min(11, next));
  transposeLabel.textContent = formatTranspose(transpose);
  // Disable the ± buttons when the limit is reached so the user gets explicit
  // visual feedback rather than a silent no-op click.
  btnTransposeDown.disabled = transpose === -11;
  btnTransposeUp.disabled = transpose === 11;
  vscode.setState({ ...safeGetState(), transpose } satisfies PanelState);

  renderPreview(lastText);
}

async function main(): Promise<void> {
  // Restore the persisted view mode and transpose so the user's choices
  // survive hide/show cycles. safeGetState() validates and clamps each field
  // so that stale or corrupted state from a previous extension version cannot
  // bypass the runtime invariants enforced by adjustTranspose().
  const saved = safeGetState();
  if (saved.mode !== undefined) {
    // Persisted state takes priority over the default-mode setting.
    viewMode = saved.mode;
  } else {
    // No persisted state — fall back to the chordsketch.preview.defaultMode
    // setting injected by the extension host as a <meta> element.
    const metaMode = document.querySelector<HTMLMetaElement>(
      'meta[name="chordsketch-default-mode"]',
    )?.content;
    if (metaMode === 'html' || metaMode === 'text') {
      viewMode = metaMode;
    }
  }
  syncButtonStates();
  if (saved.transpose !== undefined) {
    transpose = saved.transpose;
    transposeLabel.textContent = formatTranspose(transpose);
    btnTransposeDown.disabled = transpose === -11;
    btnTransposeUp.disabled = transpose === 11;
  }

  // Persist the source-document URI so VS Code's WebviewPanelSerializer can
  // look it up on the next extension-host activation (after a restart) and
  // restore this panel bound to the same TextDocument. The URI is injected
  // by the extension host at HTML build time via a <meta> element.
  const metaDocumentUri = document.querySelector<HTMLMetaElement>(
    'meta[name="chordsketch-document-uri"]',
  )?.content;
  if (metaDocumentUri && metaDocumentUri !== saved.documentUri) {
    vscode.setState({ ...safeGetState(), documentUri: metaDocumentUri } satisfies PanelState);
  }

  // Register the message listener early — before WASM init — so that
  // 'transpose' messages sent by the extension host in the narrow window
  // between panel creation and WASM readiness are not silently dropped.
  //
  // A 'transpose' message that arrives during init calls adjustTranspose(),
  // which updates the `transpose` variable and calls renderPreview(lastText).
  // Because lastText is '' at this point, renderPreview returns immediately
  // at the empty-text guard — no WASM call is made. The updated `transpose`
  // value is then applied when the first 'update' message arrives after init.
  //
  // An 'update' message is only sent by the extension host after it receives
  // 'ready', which is posted below. However, the 300 ms scheduleUpdate debounce
  // timer in the extension host can fire if the document changes during the
  // loading window — those early 'update' messages are handled safely by the
  // wasmReady guard in renderPreview (text is stored in lastText; no WASM call).
  window.addEventListener('message', (event: MessageEvent) => {
    if (!isExtToWebview(event.data)) {
      // Unknown or malformed message — silently ignore.
      return;
    }
    if (event.data.type === 'update') {
      renderPreview(event.data.text);
    } else if (event.data.type === 'transpose') {
      adjustTranspose(event.data.delta);
    }
  });

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

  // Unlock the wasmReady guard so renderPreview can now call WASM exports.
  wasmReady = true;

  // Enable the toolbar only after WASM is ready so clicking buttons before
  // init is not possible. The CSS sets pointer-events:none on the toolbar
  // by default; removing the class re-enables it.
  toolbar.classList.remove('disabled');

  // Register all toolbar button handlers after WASM is ready.
  btnHtml.addEventListener('click', () => setViewMode('html'));
  btnText.addEventListener('click', () => setViewMode('text'));
  btnTransposeDown.addEventListener('click', () => adjustTranspose(-1));
  btnTransposeUp.addEventListener('click', () => adjustTranspose(1));

  // Tell the extension host that the WebView is ready to receive content.
  vscode.postMessage({ type: 'ready' });
}

void main();
