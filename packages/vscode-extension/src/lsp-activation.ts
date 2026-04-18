/**
 * VS Code-free helpers that isolate LSP startup failures from `activate()`.
 *
 * Kept in its own module so the logic can be unit-tested with plain Node
 * (`--experimental-transform-types`) — the `vscode` module is not resolvable
 * outside the extension host, so any code that imports it (including
 * `extension.ts` and `lsp.ts`) cannot be loaded directly in Node tests.
 *
 * All VS Code types are referenced via `import type` so they are erased at
 * compile time.
 */

/**
 * Minimal shape of a `vscode-languageclient` `LanguageClient` used by
 * [`tryStartLanguageClient`]. Matches the subset of the real type we depend
 * on so tests can pass a plain object without importing the real client.
 */
export interface StartableClient {
  start(): Promise<void>;
  dispose(): Promise<void> | void;
}

/**
 * Start `client` and publish it via `setClient` on success.
 *
 * If `start()` throws, calls `client.dispose()` (swallowing any dispose
 * failure — the primary error is what the caller cares about), sets the
 * module-level client reference to `undefined`, and re-throws the original
 * error. This prevents the caller's module from leaking a half-initialized
 * client that a subsequent `stop()` would operate on.
 */
export async function tryStartLanguageClient<C extends StartableClient>(
  client: C,
  setClient: (c: C | undefined) => void,
): Promise<void> {
  try {
    await client.start();
    setClient(client);
  } catch (err) {
    try {
      await client.dispose();
    } catch {
      // Dispose failure on an already-failed client is noise — surface the
      // primary error instead.
    }
    setClient(undefined);
    throw err;
  }
}

/**
 * Run `start` inside a guard that catches any failure, logs it, and notifies
 * the user instead of propagating the error.
 *
 * Used by `activate()` to ensure an LSP initialization failure cannot abort
 * activation and leave the preview / transpose / convert commands
 * unregistered (VS Code would then surface `command '...' not found` to any
 * user who invokes them).
 */
export async function startLspClientSafely(params: {
  /** The start action — typically `() => startLspClient(context)`. */
  start: () => Promise<void>;
  /** Called with any diagnostic line that should land in the LSP output channel. */
  log: (message: string) => void;
  /** Called once with a short user-facing message when start fails. */
  notify: (message: string) => void;
}): Promise<void> {
  try {
    await params.start();
  } catch (err) {
    const detail = formatError(err);
    params.log(`Failed to start chordsketch-lsp: ${detail}`);
    params.log('Preview and transpose/convert commands remain available.');
    params.notify(
      'ChordSketch: LSP failed to start — preview and transpose/convert commands remain available.',
    );
  }
}

function formatError(err: unknown): string {
  if (err instanceof Error) {
    return err.stack ?? `${err.name}: ${err.message}`;
  }
  return String(err);
}
