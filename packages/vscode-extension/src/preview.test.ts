/**
 * Unit tests for the VS Code-free helpers in preview-helpers.ts:
 * `parseSerializedState` and `escapeHtmlAttr`.
 *
 * The preview-panel lifecycle itself depends on the `vscode` host and is
 * covered by manual verification and the integration tests tracked in #1918.
 *
 * Run with:
 *   npm test
 *   node --experimental-transform-types --test src/preview.test.ts
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';
// Use .ts extension — executed directly by Node with --experimental-transform-types.
import { escapeHtmlAttr, parseSerializedState } from './preview-helpers.ts';

// --- parseSerializedState ---

test('parseSerializedState: valid state returns { documentUri }', () => {
  const state = { documentUri: 'file:///tmp/song.cho', mode: 'html', transpose: 2 };
  assert.deepEqual(parseSerializedState(state), {
    documentUri: 'file:///tmp/song.cho',
  });
});

test('parseSerializedState: documentUri alone is enough', () => {
  assert.deepEqual(parseSerializedState({ documentUri: 'untitled:Untitled-1' }), {
    documentUri: 'untitled:Untitled-1',
  });
});

test('parseSerializedState: null returns undefined', () => {
  assert.equal(parseSerializedState(null), undefined);
});

test('parseSerializedState: undefined returns undefined', () => {
  assert.equal(parseSerializedState(undefined), undefined);
});

test('parseSerializedState: primitive returns undefined', () => {
  assert.equal(parseSerializedState('file:///song.cho'), undefined);
  assert.equal(parseSerializedState(42), undefined);
  assert.equal(parseSerializedState(true), undefined);
});

test('parseSerializedState: missing documentUri returns undefined', () => {
  assert.equal(parseSerializedState({ mode: 'html', transpose: 0 }), undefined);
});

test('parseSerializedState: empty-string documentUri returns undefined', () => {
  assert.equal(parseSerializedState({ documentUri: '' }), undefined);
});

test('parseSerializedState: non-string documentUri returns undefined', () => {
  assert.equal(parseSerializedState({ documentUri: 42 }), undefined);
  assert.equal(parseSerializedState({ documentUri: null }), undefined);
  assert.equal(parseSerializedState({ documentUri: { nested: 'x' } }), undefined);
});

// --- escapeHtmlAttr ---

test('escapeHtmlAttr: ordinary URI is unchanged', () => {
  assert.equal(
    escapeHtmlAttr('file:///Users/me/song.cho'),
    'file:///Users/me/song.cho',
  );
});

test('escapeHtmlAttr: ampersand is escaped', () => {
  assert.equal(escapeHtmlAttr('a&b'), 'a&amp;b');
});

test('escapeHtmlAttr: double quote is escaped so attribute is not broken out', () => {
  assert.equal(escapeHtmlAttr('x"><script>y'), 'x&quot;&gt;&lt;script&gt;y');
});

test('escapeHtmlAttr: single quote is escaped', () => {
  assert.equal(escapeHtmlAttr("O'Brien"), 'O&#39;Brien');
});

test('escapeHtmlAttr: escapes combined & and < in order', () => {
  // & must be first so later &amp; sequences are not double-escaped.
  assert.equal(escapeHtmlAttr('&<'), '&amp;&lt;');
});

test('escapeHtmlAttr: empty string round-trips', () => {
  assert.equal(escapeHtmlAttr(''), '');
});
