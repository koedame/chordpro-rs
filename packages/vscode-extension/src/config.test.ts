/**
 * Unit tests for the pure configuration utilities in config.ts.
 *
 * Run with:
 *   npm test
 *   node --experimental-transform-types --test src/config.test.ts
 *
 * No VS Code host or external test runner required.
 */

import { test } from 'node:test';
import assert from 'node:assert/strict';
// Use .ts extension — this file is run directly by Node with
// --experimental-transform-types, not compiled by tsc.
import { resolveDefaultMode } from './config.ts';

test('resolveDefaultMode: "text" maps to "text"', () => {
  assert.equal(resolveDefaultMode('text'), 'text');
});

test('resolveDefaultMode: "html" maps to "html"', () => {
  assert.equal(resolveDefaultMode('html'), 'html');
});

test('resolveDefaultMode: "HTML" (uppercase) maps to "html"', () => {
  assert.equal(resolveDefaultMode('HTML'), 'html');
});

test('resolveDefaultMode: unknown string maps to "html"', () => {
  assert.equal(resolveDefaultMode('unknown'), 'html');
});

test('resolveDefaultMode: empty string maps to "html"', () => {
  assert.equal(resolveDefaultMode(''), 'html');
});

test('resolveDefaultMode: undefined maps to "html"', () => {
  assert.equal(resolveDefaultMode(undefined), 'html');
});
