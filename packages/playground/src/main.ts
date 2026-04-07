import init, {
  render_html,
  render_text,
  render_pdf,
  render_html_with_options,
  render_text_with_options,
  render_pdf_with_options,
} from '../../npm/web/chordsketch_wasm.js';
import { SAMPLE_CHORDPRO } from './sample';

const editor = document.getElementById('editor') as HTMLTextAreaElement;
const formatSelect = document.getElementById('format') as HTMLSelectElement;
const transposeInput = document.getElementById('transpose') as HTMLInputElement;
const preview = document.getElementById('preview') as HTMLIFrameElement;
const textOutput = document.getElementById('text-output') as HTMLPreElement;
const pdfPane = document.getElementById('pdf-pane') as HTMLDivElement;
const downloadPdfBtn = document.getElementById('download-pdf') as HTMLButtonElement;
const errorDiv = document.getElementById('error') as HTMLDivElement;

let debounceTimer: ReturnType<typeof setTimeout> | null = null;

function getTranspose(): number {
  const val = parseInt(transposeInput.value, 10);
  return isNaN(val) ? 0 : Math.max(-12, Math.min(12, val));
}

function showError(msg: string): void {
  errorDiv.textContent = msg;
  errorDiv.classList.remove('hidden');
}

function hideError(): void {
  errorDiv.classList.add('hidden');
}

/**
 * Format a thrown value into a readable error message. `String(e)` flattens
 * structured errors (e.g. JsError objects with line/col info) to "[object
 * Object]"; preferring `e.message` when available preserves the underlying
 * Rust error string. See #1060.
 */
function formatError(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === 'string') return e;
  return String(e);
}

function showPane(pane: 'html' | 'text' | 'pdf'): void {
  preview.classList.toggle('hidden', pane !== 'html');
  textOutput.classList.toggle('hidden', pane !== 'text');
  pdfPane.classList.toggle('hidden', pane !== 'pdf');
}

function render(): void {
  const input = editor.value;
  if (!input.trim()) {
    hideError();
    showPane('html');
    preview.srcdoc = '';
    textOutput.textContent = '';
    return;
  }

  const format = formatSelect.value;
  const transpose = getTranspose();

  try {
    if (format === 'html') {
      showPane('html');
      const html =
        transpose !== 0
          ? render_html_with_options(input, { transpose })
          : render_html(input);
      preview.srcdoc = wrapHtml(html);
      hideError();
    } else if (format === 'text') {
      showPane('text');
      const text =
        transpose !== 0
          ? render_text_with_options(input, { transpose })
          : render_text(input);
      textOutput.textContent = text;
      hideError();
    } else if (format === 'pdf') {
      showPane('pdf');
      hideError();
    }
  } catch (e) {
    showError(formatError(e));
  }
}

function wrapHtml(body: string): string {
  return `<!DOCTYPE html>
<html>
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

function downloadPdf(): void {
  const input = editor.value;
  if (!input.trim()) return;

  const transpose = getTranspose();

  try {
    const pdfBytes =
      transpose !== 0
        ? render_pdf_with_options(input, { transpose })
        : render_pdf(input);
    const blob = new Blob([pdfBytes as BlobPart], { type: 'application/pdf' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'chordsketch-output.pdf';
    a.click();
    URL.revokeObjectURL(url);
    hideError();
  } catch (e) {
    showError(formatError(e));
  }
}

function scheduleRender(): void {
  if (debounceTimer !== null) {
    clearTimeout(debounceTimer);
  }
  debounceTimer = setTimeout(render, 300);
}

async function main(): Promise<void> {
  try {
    await init();
  } catch (e) {
    showError(`Failed to initialize WASM: ${formatError(e)}`);
    return;
  }

  editor.value = SAMPLE_CHORDPRO;

  editor.addEventListener('input', scheduleRender);
  formatSelect.addEventListener('change', render);
  transposeInput.addEventListener('input', scheduleRender);
  downloadPdfBtn.addEventListener('click', downloadPdf);

  render();
}

main();
