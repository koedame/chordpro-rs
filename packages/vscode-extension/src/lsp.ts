/**
 * LSP client lifecycle management.
 *
 * Starts and stops the `chordsketch-lsp` language server process. Exposes
 * diagnostics, completions (directive names, chord names, metadata keys),
 * hover (chord diagrams, directive docs), and document formatting — all
 * provided by the server, no client-side middleware needed.
 */

import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';
import { resolveLspBinary } from './platform.js';
import { tryStartLanguageClient } from './lsp-activation.js';

let client: LanguageClient | undefined;

/**
 * Single output channel reused across `startLspClient` calls to avoid
 * accumulating duplicate "ChordSketch LSP" entries in the Output dropdown
 * when the binary is not found and the user repeatedly toggles the setting.
 */
let notFoundChannel: vscode.OutputChannel | undefined;

/**
 * Starts the LSP client if it is not already running.
 *
 * If the `chordsketch-lsp` binary cannot be found, logs a warning and shows
 * a one-time information message offering to configure the path. Syntax
 * highlighting and the preview panel continue to work without the LSP.
 */
export async function startLspClient(context: vscode.ExtensionContext): Promise<void> {
  const config = vscode.workspace.getConfiguration('chordsketch.lsp');
  const enabled = config.get<boolean>('enabled', true);
  if (!enabled) {
    return;
  }

  const configuredPath = config.get<string>('path', '');
  const binaryPath = await resolveLspBinary(context.extensionPath, configuredPath);

  if (!binaryPath) {
    const msg =
      'ChordSketch: chordsketch-lsp binary not found. ' +
      'Install it via cargo/Homebrew/Scoop or set chordsketch.lsp.path in settings.';
    if (!notFoundChannel) {
      notFoundChannel = vscode.window.createOutputChannel('ChordSketch LSP');
      context.subscriptions.push(notFoundChannel);
    }
    notFoundChannel.appendLine(msg);
    notFoundChannel.appendLine(
      'Syntax highlighting and the preview panel will still work without the LSP.',
    );

    void vscode.window.showInformationMessage(msg, 'Open Settings').then((choice) => {
      if (choice === 'Open Settings') {
        void vscode.commands.executeCommand(
          'workbench.action.openSettings',
          'chordsketch.lsp.path',
        );
      }
    });
    return;
  }

  const serverOptions: ServerOptions = {
    run: {
      command: binaryPath,
      args: ['--stdio'],
      transport: TransportKind.stdio,
    },
    debug: {
      command: binaryPath,
      args: ['--stdio'],
      transport: TransportKind.stdio,
      options: {
        env: { ...process.env, RUST_LOG: 'chordsketch_lsp=debug' },
      },
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'chordpro' }],
    synchronize: {
      configurationSection: 'chordsketch',
    },
    outputChannelName: 'ChordSketch LSP',
  };

  // Create the client locally. The module-level `client` reference is only
  // assigned when `start()` resolves — otherwise a `stopLspClient` call from
  // the configuration-change listener would run `stop()` on a half-initialized
  // client, whose state is undefined in `vscode-languageclient` v9.
  const newClient = new LanguageClient(
    'chordsketch',
    'ChordSketch',
    serverOptions,
    clientOptions,
  );
  // `tryStartLanguageClient` disposes `newClient` on failure and re-throws,
  // so the push below is only reached on a successful start. This avoids
  // the double-dispose that would occur if we pushed before awaiting —
  // `LanguageClient.dispose()` is not contractually idempotent across
  // vscode-languageclient versions, so we register for VS Code cleanup
  // only after we know we have a live client that won't be disposed eagerly.
  await tryStartLanguageClient(newClient, (c) => {
    client = c;
  });
  context.subscriptions.push(newClient);
}

/** Stops the LSP client gracefully and resets the not-found channel reference. */
export async function stopLspClient(): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
  // Reset so the next activation registers a fresh channel in the new context.
  notFoundChannel = undefined;
}
