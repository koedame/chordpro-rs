/**
 * Unit tests for pure utility functions in command-utils.ts.
 *
 * Run with:
 *   node --experimental-transform-types --test src/command-utils.test.ts
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  extensionForFormat,
  isWasmRenderModule,
  defaultExportPath,
  FORMAT_HTML,
  FORMAT_TEXT,
  FORMAT_PDF,
} from './command-utils.ts';

// ── extensionForFormat ────────────────────────────────────────────────────────

test('extensionForFormat: HTML maps to .html', () => {
  assert.equal(extensionForFormat(FORMAT_HTML), '.html');
});

test('extensionForFormat: Plain text maps to .txt', () => {
  assert.equal(extensionForFormat(FORMAT_TEXT), '.txt');
});

test('extensionForFormat: PDF maps to .pdf', () => {
  assert.equal(extensionForFormat(FORMAT_PDF), '.pdf');
});

// ── isWasmRenderModule ────────────────────────────────────────────────────────

test('isWasmRenderModule: accepts object with all three render functions', () => {
  const mod = {
    render_html: (_: string) => '',
    render_text: (_: string) => '',
    render_pdf: (_: string) => new Uint8Array(0),
  };
  assert.ok(isWasmRenderModule(mod));
});

test('isWasmRenderModule: rejects null', () => {
  assert.equal(isWasmRenderModule(null), false);
});

test('isWasmRenderModule: rejects non-object (string)', () => {
  assert.equal(isWasmRenderModule('module'), false);
});

test('isWasmRenderModule: rejects non-object (number)', () => {
  assert.equal(isWasmRenderModule(42), false);
});

test('isWasmRenderModule: rejects object missing render_html', () => {
  const mod = { render_text: () => '', render_pdf: () => new Uint8Array(0) };
  assert.equal(isWasmRenderModule(mod), false);
});

test('isWasmRenderModule: rejects object missing render_text', () => {
  const mod = { render_html: () => '', render_pdf: () => new Uint8Array(0) };
  assert.equal(isWasmRenderModule(mod), false);
});

test('isWasmRenderModule: rejects object missing render_pdf', () => {
  const mod = { render_html: () => '', render_text: () => '' };
  assert.equal(isWasmRenderModule(mod), false);
});

test('isWasmRenderModule: rejects object where render_html is not a function', () => {
  const mod = { render_html: 'not a function', render_text: () => '', render_pdf: () => new Uint8Array(0) };
  assert.equal(isWasmRenderModule(mod), false);
});

test('isWasmRenderModule: rejects empty object', () => {
  assert.equal(isWasmRenderModule({}), false);
});

test('isWasmRenderModule: rejects undefined', () => {
  assert.equal(isWasmRenderModule(undefined), false);
});

test('isWasmRenderModule: rejects object where render_text is not a function', () => {
  const mod = { render_html: () => '', render_text: 42, render_pdf: () => new Uint8Array(0) };
  assert.equal(isWasmRenderModule(mod), false);
});

test('isWasmRenderModule: rejects object where render_pdf is not a function', () => {
  const mod = { render_html: () => '', render_text: () => '', render_pdf: null };
  assert.equal(isWasmRenderModule(mod), false);
});

// ── defaultExportPath ─────────────────────────────────────────────────────────

test('defaultExportPath: replaces .cho with .html', () => {
  const result = defaultExportPath('/home/user/song.cho', '.html');
  assert.equal(result, '/home/user/song.html');
});

test('defaultExportPath: replaces .chordpro with .pdf', () => {
  const result = defaultExportPath('/songs/track.chordpro', '.pdf');
  assert.equal(result, '/songs/track.pdf');
});

test('defaultExportPath: appends extension when source has no extension', () => {
  const result = defaultExportPath('/home/user/mysong', '.html');
  assert.equal(result, '/home/user/mysong.html');
});

test('defaultExportPath: appends extension to hidden file (dotfile, no ext)', () => {
  // path.extname('.chordpro') returns '' so the stem is '.chordpro'
  const result = defaultExportPath('/home/user/.chordpro', '.html');
  assert.equal(result, '/home/user/.chordpro.html');
});

test('defaultExportPath: preserves directory', () => {
  const result = defaultExportPath('/a/b/c/song.cho', '.txt');
  assert.equal(result, '/a/b/c/song.txt');
});
