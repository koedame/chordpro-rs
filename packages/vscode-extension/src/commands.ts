/**
 * Command handlers for the ChordSketch extension.
 *
 * - `chordsketch.openPreview` — open preview in the active column
 * - `chordsketch.openPreviewToSide` — open preview to the side
 */

import * as vscode from 'vscode';
import { createOrShow, notifyTranspose } from './preview';

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
