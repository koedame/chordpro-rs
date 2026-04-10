/**
 * ChordSketch VS Code extension entry point.
 *
 * Activated when any ChordPro file (`.cho`, `.chordpro`, `.chopro`) is opened,
 * thanks to the `contributes.languages` declaration in `package.json`.
 *
 * Responsibilities:
 *   - Start the `chordsketch-lsp` language server (if enabled and found)
 *   - Register preview commands
 *   - Listen for document changes and propagate them to open preview panels
 *   - Restart the LSP when the user changes `chordsketch.lsp.path`
 */

import * as vscode from 'vscode';
import { startLspClient, stopLspClient } from './lsp.js';
import { notifyDocumentChanged, disposeAll } from './preview.js';
import { registerOpenPreview, registerOpenPreviewToSide, registerTransposeUp, registerTransposeDown, registerConvertTo } from './commands.js';

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // Start the LSP client (gracefully degraded if binary not found).
  await startLspClient(context);

  // Register preview commands.
  context.subscriptions.push(
    registerOpenPreview(context),
    registerOpenPreviewToSide(context),
    registerTransposeUp(),
    registerTransposeDown(),
    registerConvertTo(context),
  );

  // Propagate document changes to open preview panels (debounced inside PreviewPanel).
  context.subscriptions.push(
    vscode.workspace.onDidChangeTextDocument(notifyDocumentChanged),
  );

  // Restart the LSP when the user changes the binary path or enables/disables it.
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (
        event.affectsConfiguration('chordsketch.lsp.enabled') ||
        event.affectsConfiguration('chordsketch.lsp.path')
      ) {
        await stopLspClient();
        await startLspClient(context);
      }
    }),
  );
}

export async function deactivate(): Promise<void> {
  disposeAll();
  await stopLspClient();
}
