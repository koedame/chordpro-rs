#!/usr/bin/env node
/**
 * Build script for the ChordSketch VS Code extension.
 *
 * Produces two bundles:
 *   dist/extension.js    — Extension host (Node.js, CJS)
 *   dist/webview/preview.js — WebView script (browser, ESM)
 *
 * Also copies:
 *   syntaxes/*.json       → dist/syntaxes/
 *   packages/npm/web/chordsketch_wasm_bg.wasm → dist/webview/
 */

import * as esbuild from 'esbuild';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, '..', '..');
const watch = process.argv.includes('--watch');

// Ensure output directories exist.
fs.mkdirSync(path.join(here, 'dist', 'syntaxes'), { recursive: true });
fs.mkdirSync(path.join(here, 'dist', 'webview'), { recursive: true });

// Copy only the two VS Code-required files from repo root syntaxes/.
const syntaxesSrc = path.join(repoRoot, 'syntaxes');
const syntaxesDst = path.join(here, 'dist', 'syntaxes');
// Clean destination first to avoid stale files from previous builds.
if (fs.existsSync(syntaxesDst)) {
  fs.rmSync(syntaxesDst, { recursive: true });
}
fs.mkdirSync(syntaxesDst, { recursive: true });
for (const file of ['chordpro.tmLanguage.json', 'language-configuration.json']) {
  const src = path.join(syntaxesSrc, file);
  if (fs.existsSync(src)) {
    fs.copyFileSync(src, path.join(syntaxesDst, file));
    console.log(`Copied syntaxes/${file}`);
  } else {
    throw new Error(`Required syntax file not found: ${src}`);
  }
}

// Copy the WASM binary for the WebView (browser build).
// Prefer the installed npm package's web build over the local monorepo build.
const wasmNpmWeb = path.join(here, 'node_modules', '@chordsketch', 'wasm', 'web', 'chordsketch_wasm_bg.wasm');
const wasmLocalWeb = path.join(repoRoot, 'packages', 'npm', 'web', 'chordsketch_wasm_bg.wasm');
const wasmSrc = fs.existsSync(wasmNpmWeb) ? wasmNpmWeb : wasmLocalWeb;
const wasmDst = path.join(here, 'dist', 'webview', 'chordsketch_wasm_bg.wasm');
if (fs.existsSync(wasmSrc)) {
  fs.copyFileSync(wasmSrc, wasmDst);
  console.log(`Copied ${path.relative(here, wasmSrc)} → dist/webview/chordsketch_wasm_bg.wasm`);
} else {
  console.warn('WARNING: chordsketch_wasm_bg.wasm not found in node_modules or local build.');
  console.warn('         Run `npm install` in packages/vscode-extension/ to fetch @chordsketch/wasm.');
}

/** @type {esbuild.BuildOptions} */
const extensionBuild = {
  entryPoints: ['src/extension.ts'],
  bundle: true,
  outfile: 'dist/extension.js',
  external: ['vscode'],
  format: 'cjs',
  platform: 'node',
  target: 'node18',
  sourcemap: true,
  logLevel: 'info',
};

// The WebView bundle imports from the wasm-pack web target's JS glue.
// Prefer the installed npm package over the local monorepo build.
const wasmJsNpm = path.join(here, 'node_modules', '@chordsketch', 'wasm', 'web', 'chordsketch_wasm.js');
const wasmJsLocal = path.join(repoRoot, 'packages', 'npm', 'web', 'chordsketch_wasm.js');
const wasmJsSrc = fs.existsSync(wasmJsNpm) ? wasmJsNpm : wasmJsLocal;

/** @type {esbuild.BuildOptions} */
const webviewBuild = {
  entryPoints: ['webview/preview.ts'],
  bundle: true,
  outfile: 'dist/webview/preview.js',
  format: 'esm',
  platform: 'browser',
  target: 'es2020',
  sourcemap: true,
  logLevel: 'info',
  alias: {
    // Resolve @chordsketch/wasm to the web (browser) build of the WASM package,
    // matching the pattern used in packages/playground/vite.config.ts.
    '@chordsketch/wasm': wasmJsSrc,
  },
};

if (watch) {
  const [extCtx, wvCtx] = await Promise.all([
    esbuild.context(extensionBuild),
    esbuild.context(webviewBuild),
  ]);
  await Promise.all([extCtx.watch(), wvCtx.watch()]);
  console.log('Watching for changes...');
} else {
  await Promise.all([
    esbuild.build(extensionBuild),
    esbuild.build(webviewBuild),
  ]);
}
