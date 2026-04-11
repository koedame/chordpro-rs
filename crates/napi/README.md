<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# @chordsketch/node

[ChordSketch](https://github.com/koedame/chordsketch) native Node.js addon —
parse and render [ChordPro](https://www.chordpro.org/) files with native
performance via [napi-rs](https://napi.rs/).

**Prebuilt binaries — no Rust toolchain required.**
Prefer this over `@chordsketch/wasm` when running in Node.js: no WASM overhead,
`Buffer` output for PDF, and better throughput for batch workloads. Use
`@chordsketch/wasm` for browser environments.

## Installation

```bash
npm install @chordsketch/node
```

Prebuilt binaries are published for five platforms and installed automatically
as optional dependencies:

| Platform | Package |
|----------|---------|
| Linux x86\_64 (glibc) | `@chordsketch/node-linux-x64-gnu` |
| Linux arm64 (glibc) | `@chordsketch/node-linux-arm64-gnu` |
| macOS x86\_64 | `@chordsketch/node-darwin-x64` |
| macOS arm64 (Apple Silicon) | `@chordsketch/node-darwin-arm64` |
| Windows x86\_64 | `@chordsketch/node-win32-x64-msvc` |

## Quick Start

```js
import { renderHtml, renderText, renderPdf, version } from '@chordsketch/node';

const source = `{title: Amazing Grace}
{key: G}

[G]Amazing [G7]grace, how [C]sweet the [G]sound`;

const html = renderHtml(source);   // string — full HTML document
const text = renderText(source);   // string — plain text with chord lines above lyrics
const pdf  = renderPdf(source);    // Buffer — raw PDF bytes

console.log(`ChordSketch ${version()}`);
```

## API

### Rendering

| Function | Returns | Description |
|----------|---------|-------------|
| `renderText(source)` | `string` | Plain text: chord names above lyric lines |
| `renderHtml(source)` | `string` | Full HTML document |
| `renderPdf(source)` | `Buffer` | Raw PDF bytes |
| `renderTextWithOptions(source, options)` | `string` | Text with transposition / config |
| `renderHtmlWithOptions(source, options)` | `string` | HTML with transposition / config |
| `renderPdfWithOptions(source, options)` | `Buffer` | PDF with transposition / config |

> **Note:** `renderPdf` returns a Node.js `Buffer` (not `Uint8Array`). This
> differs from `@chordsketch/wasm` where PDF output is `Uint8Array`.

### Validation

```js
import { validate } from '@chordsketch/node';

const errors = validate(source); // string[] — empty array if clean
for (const msg of errors) {
  console.warn(msg);
}
```

### Utility

| Function | Returns |
|----------|---------|
| `version()` | `string` — library version |

## Options

```ts
interface RenderOptions {
  /** Semitone transposition offset. Default: 0.
   *  Values outside the i8 range (-128..127) are clamped, then reduced mod 12. */
  transpose?: number;

  /** Preset name ("guitar", "ukulele") or inline RRJSON configuration string. */
  config?: string;
}
```

### Rendering with options

```js
import { renderHtmlWithOptions, renderTextWithOptions } from '@chordsketch/node';

// Transpose up 2 semitones using the ukulele preset
const html = renderHtmlWithOptions(source, { transpose: 2, config: 'ukulele' });

// Inline RRJSON config
const text = renderTextWithOptions(source, {
  config: '{"settings": {"notation": "solfege"}}',
});
```

## Error handling

Functions throw a JavaScript `Error` if the `config` option is not a known
preset name and is also not valid RRJSON.

```js
import { renderHtml } from '@chordsketch/node';

try {
  renderHtml(source);
} catch (err) {
  console.error(err.message);
}
```

Parse errors in the ChordPro input are **not** thrown — the renderer is
lenient and produces a best-effort result. Call `validate()` to surface
diagnostics without rendering.

## Links

- **Project**: https://github.com/koedame/chordsketch
- **Playground**: https://chordsketch.koeda.me
- **Issues**: https://github.com/koedame/chordsketch/issues
- **WASM package** (browser): [`@chordsketch/wasm`](https://www.npmjs.com/package/@chordsketch/wasm)

## License

MIT
