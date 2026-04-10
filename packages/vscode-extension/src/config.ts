/**
 * Pure configuration utilities for the ChordSketch extension.
 *
 * Functions in this module have no dependencies on the VS Code API so they can
 * be unit-tested with Node.js's built-in test runner without a VS Code host.
 */

/**
 * Maps a raw `chordsketch.preview.defaultMode` configuration value to a valid
 * view-mode string.
 *
 * Any value that is not exactly `'text'` maps to `'html'` so that:
 * - Unrecognised future enum values degrade gracefully to the default.
 * - Case-variants (`'HTML'`, `'Text'`) are treated as unknown rather than
 *   silently accepted.
 *
 * Used in `preview.ts` and independently testable without a VS Code host.
 */
export function resolveDefaultMode(raw: string | undefined): 'html' | 'text' {
  return raw === 'text' ? 'text' : 'html';
}
