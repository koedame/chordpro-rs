/**
 * ChordSketch VS Code extension entry point.
 *
 * Activated when any ChordPro file (`.cho`, `.chordpro`, `.chopro`) is opened,
 * thanks to the `contributes.languages` declaration in `package.json`.
 *
 * Responsibilities:
 *   - Register preview / transpose / convert commands
 *   - Listen for document changes and propagate them to open preview panels
 *   - Start the `chordsketch-lsp` language server (if enabled and found)
 *   - Restart the LSP when the user changes `chordsketch.lsp.path`
 *
 * All non-LSP disposables are registered **before** the LSP is started so
 * that an LSP initialization failure cannot leave any contributed command
 * unregistered. See `lsp-activation.ts` for the start guard.
 */

import * as vscode from 'vscode';
import { startLspClient, stopLspClient } from './lsp.js';
import { startLspClientSafely } from './lsp-activation.js';
import { notifyDocumentChanged, disposeAll, registerPreviewSerializer } from './preview.js';
import { registerOpenPreview, registerOpenPreviewToSide, registerTransposeUp, registerTransposeDown, registerConvertTo, resetCommandSingletons } from './commands.js';

/**
 * Output channel used for LSP start-failure diagnostics. Lazily created so we
 * do not add a `ChordSketch LSP` entry to the Output dropdown when the LSP
 * starts cleanly (in that case `vscode-languageclient` creates its own
 * channel with the same name).
 */
let lspDiagnosticChannel: vscode.OutputChannel | undefined;

function ensureLspDiagnosticChannel(context: vscode.ExtensionContext): vscode.OutputChannel {
  if (!lspDiagnosticChannel) {
    lspDiagnosticChannel = vscode.window.createOutputChannel('ChordSketch LSP');
    context.subscriptions.push(lspDiagnosticChannel);
  }
  return lspDiagnosticChannel;
}

async function startLspGuarded(context: vscode.ExtensionContext): Promise<void> {
  // Do NOT call ensureLspDiagnosticChannel eagerly here — VS Code registers
  // output channels in the dropdown as soon as they are created, even when
  // empty. If the LSP starts successfully, vscode-languageclient creates its
  // own 'ChordSketch LSP' channel; creating a second one unconditionally would
  // produce a duplicate entry. Defer channel creation into the callbacks so it
  // only materialises on failure.
  await startLspClientSafely({
    start: () => startLspClient(context),
    log: (message) => ensureLspDiagnosticChannel(context).appendLine(message),
    notify: (message) => {
      void vscode.window.showInformationMessage(message, 'Open Output').then((choice) => {
        if (choice === 'Open Output') {
          ensureLspDiagnosticChannel(context).show();
        }
      });
    },
  });
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // Register everything that does not depend on the LSP FIRST. If a later
  // `startLspClient` call throws, this block has already completed, so
  // preview / transpose / convert commands remain available.
  context.subscriptions.push(
    registerOpenPreview(context),
    registerOpenPreviewToSide(context),
    registerTransposeUp(),
    registerTransposeDown(),
    registerConvertTo(context),
    // Register the preview-panel serializer so that preview tabs survive
    // VS Code restarts. Registration is synchronous and purely structural; it
    // does not touch the LSP.
    registerPreviewSerializer(context),
  );

  // Propagate document changes to open preview panels (debounced inside PreviewPanel).
  context.subscriptions.push(
    vscode.workspace.onDidChangeTextDocument(notifyDocumentChanged),
  );

  // Restart the LSP when the user changes the binary path or enables/disables it.
  // The restart is wrapped in `startLspGuarded` so a subsequent bad setting
  // cannot tear down command registrations either.
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (
        event.affectsConfiguration('chordsketch.lsp.enabled') ||
        event.affectsConfiguration('chordsketch.lsp.path')
      ) {
        await stopLspClient();
        await startLspGuarded(context);
      }
    }),
  );

  // Start the LSP client last, under a failure guard.
  await startLspGuarded(context);
}

export async function deactivate(): Promise<void> {
  disposeAll();
  await stopLspClient();
  resetCommandSingletons();
  lspDiagnosticChannel = undefined;
}
