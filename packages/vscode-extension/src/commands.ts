/**
 * Command handlers for the ChordSketch extension.
 *
 * - `chordsketch.openPreview` — open preview in the active column
 * - `chordsketch.openPreviewToSide` — open preview to the side
 * - `chordsketch.convertTo` — export the active ChordPro document as HTML/PDF/text
 */

import * as vscode from 'vscode';
import * as path from 'path';
import { createOrShow, notifyTranspose } from './preview.js';
import {
  FORMAT_HTML,
  FORMAT_TEXT,
  FORMAT_PDF,
  type ExportFormat,
  type WasmRenderModule,
  isWasmRenderModule,
  extensionForFormat,
  defaultExportPath,
} from './command-utils.js';
import { commandState } from './command-state.js';

export { resetCommandSingletons } from './command-state.js';

/**
 * Returns the shared ChordSketch output channel, creating it if necessary.
 *
 * On first creation the channel is pushed to `context.subscriptions` so VS Code
 * disposes it automatically when the extension is deactivated.
 */
function getExportChannel(context: vscode.ExtensionContext): vscode.OutputChannel {
  if (!commandState.exportOutputChannel) {
    const ch = vscode.window.createOutputChannel('ChordSketch');
    context.subscriptions.push(ch);
    commandState.exportOutputChannel = ch;
  }
  // exportOutputChannel is guaranteed non-undefined: it was either non-undefined
  // on entry or just assigned in the branch above.
  // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
  return commandState.exportOutputChannel!;
}

/**
 * Loads the `@chordsketch/wasm` Node.js CJS build from the extension's own
 * `dist/node/` directory.
 *
 * Using a runtime-computed path prevents esbuild from statically bundling the
 * module into `dist/extension.js`.  The loaded module's own `__dirname`
 * therefore points to `dist/node/`, where `chordsketch_wasm_bg.wasm` is
 * located, so WASM initialisation succeeds.
 *
 * The shape of the loaded module is validated by {@link isWasmRenderModule}
 * before caching so that a broken or incomplete deployment is detected
 * immediately rather than being permanently cached for the session.
 *
 * The result is cached so the WASM binary is only parsed once per session.
 *
 * **Note on `require.cache`**: Node.js's `require.cache` persists across
 * Extension Host restarts within the same VS Code process.  After
 * `resetCommandSingletons()` clears `wasmRenderCache`, the next call here
 * will invoke `require(modPath)` again, but Node.js returns the in-process
 * cached module rather than re-reading the file from disk.  A freshly
 * deployed WASM binary at the same path is only guaranteed to be picked up
 * after a full VS Code process restart.
 *
 * @throws {Error} If the module file is missing or its exports are absent.
 */
function loadWasmRender(extensionPath: string): WasmRenderModule {
  if (!commandState.wasmRenderCache) {
    const modPath = path.join(extensionPath, 'dist', 'node', 'chordsketch_wasm.js');
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const mod: unknown = require(modPath);
    if (!isWasmRenderModule(mod)) {
      throw new Error(
        'WASM module does not export the expected render functions',
      );
    }
    commandState.wasmRenderCache = mod;
  }
  // wasmRenderCache is guaranteed non-undefined here.
  // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
  return commandState.wasmRenderCache!;
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
    const defaultUri = vscode.Uri.file(defaultExportPath(doc.uri.fsPath, ext));

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
      const ch = getExportChannel(context);
      ch.appendLine(`[convertTo] Failed to load WASM renderer: ${String(err)}`);
      ch.show(true);
      void vscode.window.showErrorMessage(
        'ChordSketch: Failed to load WASM renderer. See the "ChordSketch" output channel for details.',
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
      const ch = getExportChannel(context);
      ch.appendLine(`[convertTo] Export failed: ${String(err)}`);
      ch.show(true);
      void vscode.window.showErrorMessage(
        'ChordSketch: Export failed. See the "ChordSketch" output channel for details.',
      );
      return;
    }

    if (format === FORMAT_PDF) {
      const openBtn = 'Open PDF';
      const choice = await vscode.window.showInformationMessage(
        `ChordSketch: Exported PDF to ${saveUri.fsPath}`,
        openBtn,
      );
      if (choice === openBtn) {
        await vscode.env.openExternal(saveUri);
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
