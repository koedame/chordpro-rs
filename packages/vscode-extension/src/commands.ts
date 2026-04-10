/**
 * Command handlers for the ChordSketch extension.
 *
 * - `chordsketch.openPreview` — open preview in the active column
 * - `chordsketch.openPreviewToSide` — open preview to the side
 * - `chordsketch.convertTo` — export the active ChordPro document as HTML/PDF/text
 */

import * as vscode from 'vscode';
import * as path from 'path';
import { createOrShow, notifyTranspose } from './preview';

/**
 * Minimal type for the exports consumed from the `@chordsketch/wasm` Node.js
 * CJS build that is copied to `dist/node/` at build time.  Only the three
 * render functions used by `convertTo` are declared here.
 */
interface WasmRenderModule {
  render_html(input: string): string;
  render_text(input: string): string;
  render_pdf(input: string): Uint8Array;
}

/** Lazily loaded WASM render module singleton. */
let wasmRenderCache: WasmRenderModule | undefined;

/**
 * Loads the `@chordsketch/wasm` Node.js CJS build from the extension's own
 * `dist/node/` directory.
 *
 * Using a runtime-computed path prevents esbuild from statically bundling the
 * module into `dist/extension.js`.  The loaded module's own `__dirname`
 * therefore points to `dist/node/`, where `chordsketch_wasm_bg.wasm` is
 * located, so WASM initialisation succeeds.
 *
 * The result is cached so the WASM binary is only parsed once per session.
 */
function loadWasmRender(extensionPath: string): WasmRenderModule {
  if (!wasmRenderCache) {
    const modPath = path.join(extensionPath, 'dist', 'node', 'chordsketch_wasm.js');
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    wasmRenderCache = require(modPath) as WasmRenderModule;
  }
  return wasmRenderCache;
}

/**
 * Resolves the active ChordPro document from the active text editor.
 * Shows an error message and returns `undefined` if no suitable editor is open.
 */
function resolveActiveChordProDocument(): vscode.TextDocument | undefined {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    void vscode.window.showErrorMessage('ChordSketch: No active editor.');
    return undefined;
  }
  if (editor.document.languageId !== 'chordpro') {
    void vscode.window.showErrorMessage(
      'ChordSketch: The active file is not a ChordPro document (.cho, .chordpro, .chopro).',
    );
    return undefined;
  }
  return editor.document;
}

/** Opens the preview panel in the same column as the active editor. */
export function registerOpenPreview(
  context: vscode.ExtensionContext,
): vscode.Disposable {
  return vscode.commands.registerCommand('chordsketch.openPreview', () => {
    const doc = resolveActiveChordProDocument();
    if (doc) {
      createOrShow(context, doc, vscode.ViewColumn.Active);
    }
  });
}

/** Opens the preview panel to the side of the active editor. */
export function registerOpenPreviewToSide(
  context: vscode.ExtensionContext,
): vscode.Disposable {
  return vscode.commands.registerCommand('chordsketch.openPreviewToSide', () => {
    const doc = resolveActiveChordProDocument();
    if (doc) {
      createOrShow(context, doc, vscode.ViewColumn.Beside);
    }
  });
}

/**
 * Increments the transpose offset of the active document's preview panel by
 * +1 semitone. No-op if no ChordPro preview panel is open for the active
 * document. The `when` clause in `package.json` hides this command from the
 * command palette when a non-ChordPro file is focused, but programmatic
 * invocation (e.g. keyboard shortcuts without a `when` guard) can still reach
 * the handler — the `languageId` check here provides the same defence.
 */
export function registerTransposeUp(): vscode.Disposable {
  return vscode.commands.registerCommand('chordsketch.transposeUp', () => {
    const editor = vscode.window.activeTextEditor;
    if (editor && editor.document.languageId === 'chordpro') {
      notifyTranspose(editor.document.uri.toString(), 1);
    }
  });
}

/**
 * Decrements the transpose offset of the active document's preview panel by
 * −1 semitone. No-op if no ChordPro preview panel is open for the active
 * document. The `languageId` guard mirrors `registerTransposeUp` to prevent
 * an unexpected transpose action when a non-ChordPro editor is focused.
 */
export function registerTransposeDown(): vscode.Disposable {
  return vscode.commands.registerCommand('chordsketch.transposeDown', () => {
    const editor = vscode.window.activeTextEditor;
    if (editor && editor.document.languageId === 'chordpro') {
      notifyTranspose(editor.document.uri.toString(), -1);
    }
  });
}

/** Label constants for the export format QuickPick. */
const FORMAT_HTML = 'HTML' as const;
const FORMAT_TEXT = 'Plain text' as const;
const FORMAT_PDF = 'PDF' as const;
type ExportFormat = typeof FORMAT_HTML | typeof FORMAT_TEXT | typeof FORMAT_PDF;

/**
 * Returns the default file extension for an export format.
 *
 * @param format - One of the three supported export formats.
 * @returns The file extension string including the leading dot.
 */
function extensionForFormat(format: ExportFormat): string {
  if (format === FORMAT_PDF) return '.pdf';
  if (format === FORMAT_TEXT) return '.txt';
  return '.html';
}

/**
 * Exports the active ChordPro document as HTML, plain text, or PDF.
 *
 * The command:
 * 1. Resolves the active ChordPro document (errors if none is open).
 * 2. Prompts the user to choose an output format via QuickPick.
 * 3. Opens a Save dialog pre-filled with the source file name (extension
 *    replaced by the chosen format's extension).
 * 4. Loads the `@chordsketch/wasm` Node.js CJS build lazily and renders the
 *    document in the chosen format.
 * 5. Writes the output to the chosen path and offers to open/reveal it.
 *
 * The WASM module is loaded from `dist/node/chordsketch_wasm.js` (copied
 * there by `esbuild.mjs` at build time) so its `__dirname` correctly points
 * to the directory that contains `chordsketch_wasm_bg.wasm`.
 */
export function registerConvertTo(context: vscode.ExtensionContext): vscode.Disposable {
  return vscode.commands.registerCommand('chordsketch.convertTo', async () => {
    const doc = resolveActiveChordProDocument();
    if (!doc) {
      return;
    }

    const format = await vscode.window.showQuickPick([FORMAT_HTML, FORMAT_TEXT, FORMAT_PDF], {
      placeHolder: 'Select output format',
    });
    if (!format) {
      return;
    }

    const ext = extensionForFormat(format as ExportFormat);
    const defaultUri = vscode.Uri.file(doc.uri.fsPath.replace(/\.[^.]+$/, ext));

    let filters: { [name: string]: string[] };
    if (format === FORMAT_PDF) {
      filters = { 'PDF Documents': ['pdf'] };
    } else if (format === FORMAT_TEXT) {
      filters = { 'Plain Text': ['txt'] };
    } else {
      filters = { 'HTML Documents': ['html', 'htm'] };
    }

    const saveUri = await vscode.window.showSaveDialog({ defaultUri, filters });
    if (!saveUri) {
      return;
    }

    let wasm: WasmRenderModule;
    try {
      wasm = loadWasmRender(context.extensionPath);
    } catch (err) {
      void vscode.window.showErrorMessage(
        `ChordSketch: Failed to load WASM renderer: ${String(err)}`,
      );
      return;
    }

    try {
      if (format === FORMAT_PDF) {
        const bytes = wasm.render_pdf(doc.getText());
        await vscode.workspace.fs.writeFile(saveUri, bytes);
      } else if (format === FORMAT_TEXT) {
        const rendered = wasm.render_text(doc.getText());
        await vscode.workspace.fs.writeFile(saveUri, Buffer.from(rendered, 'utf-8'));
      } else {
        const rendered = wasm.render_html(doc.getText());
        await vscode.workspace.fs.writeFile(saveUri, Buffer.from(rendered, 'utf-8'));
      }
    } catch (err) {
      void vscode.window.showErrorMessage(`ChordSketch: Export failed: ${String(err)}`);
      return;
    }

    if (format === FORMAT_PDF) {
      const revealBtn = 'Reveal in Explorer';
      const choice = await vscode.window.showInformationMessage(
        `ChordSketch: Exported PDF to ${saveUri.fsPath}`,
        revealBtn,
      );
      if (choice === revealBtn) {
        await vscode.commands.executeCommand('revealFileInOS', saveUri);
      }
    } else {
      const openBtn = 'Open File';
      const choice = await vscode.window.showInformationMessage(
        `ChordSketch: Exported to ${saveUri.fsPath}`,
        openBtn,
      );
      if (choice === openBtn) {
        const opened = await vscode.workspace.openTextDocument(saveUri);
        await vscode.window.showTextDocument(opened);
      }
    }
  });
}
