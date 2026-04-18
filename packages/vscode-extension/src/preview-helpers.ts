/**
 * VS Code-free helpers for the preview panel.
 *
 * Kept in a dedicated module so they can be unit-tested with plain Node
 * (`--experimental-transform-types`) — anything that imports the `vscode`
 * module (including `preview.ts`) cannot be loaded directly in Node tests
 * because the module is only resolvable inside the extension host.
 */

/**
 * Extracts the persisted source-document URI from a deserialized WebView state.
 *
 * Returns `undefined` when the state is missing, malformed, or the URI is not
 * a non-empty string. Used by the preview-panel serializer in `preview.ts`
 * to decide whether the state handed back by VS Code is actionable.
 */
export function parseSerializedState(state: unknown): { documentUri: string } | undefined {
  if (typeof state !== 'object' || state === null) {
    return undefined;
  }
  const raw = state as Record<string, unknown>;
  const uri = raw['documentUri'];
  if (typeof uri !== 'string' || uri.length === 0) {
    return undefined;
  }
  return { documentUri: uri };
}

/**
 * Minimal HTML-attribute escape for embedding values (URIs, config strings)
 * into the `content=` attribute of an injected `<meta>` element.
 *
 * Escapes `&` first so subsequent `&amp;` sequences are not re-escaped.
 */
export function escapeHtmlAttr(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}
