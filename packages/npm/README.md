<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# @chordsketch/wasm

[ChordSketch](https://github.com/koedame/chordsketch) compiled to
WebAssembly — parse and render [ChordPro](https://www.chordpro.org/) files
in the browser **or** in Node.js with the same package.

> Requires `>=0.1.1`. Version `0.1.0` is published but does not work in
> Node.js (it tries to `fetch()` the wasm file via `file://`, which Node's
> undici does not support). `0.1.1` introduced a dual-package layout that
> resolves to a Node-compatible build automatically.

## Installation

```bash
npm install '@chordsketch/wasm@>=0.1.1'
```

## Usage

The package ships **two builds** under one name and uses Node's
conditional `exports` to pick the right one for the runtime:

| Runtime | Build | Init required? |
|---|---|---|
| Browser (Vite, webpack, native ESM) | `wasm-pack --target web` | Yes — `await init()` |
| Node.js (≥ 20) | `wasm-pack --target nodejs` | No — auto-loaded synchronously |

### Browser

```js
import init, {
  render_html,
  render_text,
  render_pdf,
  validate,
  version,
} from '@chordsketch/wasm';

// Browsers must initialize the WASM module first.
await init();

const chordpro = `{title: Amazing Grace}
{key: G}

[G]Amazing [G7]grace, how [C]sweet the [G]sound`;

const errors = validate(chordpro); // [] if valid
const html = render_html(chordpro);
const text = render_text(chordpro);
const pdfBytes = render_pdf(chordpro); // Uint8Array
console.log(version());
```

### Node.js

```js
// No init() — the wasm-pack nodejs build auto-loads the .wasm file
// synchronously when the module is imported.
import {
  render_html,
  render_text,
  render_pdf,
  validate,
  version,
} from '@chordsketch/wasm';

const chordpro = `{title: Amazing Grace}
{key: G}

[G]Amazing [G7]grace, how [C]sweet the [G]sound`;

const errors = validate(chordpro); // [] if valid
const html = render_html(chordpro);
const text = render_text(chordpro);
const pdfBytes = render_pdf(chordpro); // Uint8Array
console.log(version());
```

### Rendering with options (both runtimes)

```js
// Browser
import init, { render_html_with_options } from '@chordsketch/wasm';
await init();

// Node.js
import { render_html_with_options } from '@chordsketch/wasm';

const html = render_html_with_options(input, {
  transpose: 2,        // semitone offset (any integer; renderer reduces mod 12)
  config: 'ukulele',   // preset name or inline RRJSON config
});
```

## API

### Basic rendering

| Function | Input | Output |
|----------|-------|--------|
| `render_html(input)` | ChordPro string | HTML string |
| `render_text(input)` | ChordPro string | Plain text string |
| `render_pdf(input)` | ChordPro string | `Uint8Array` (PDF bytes) |

### Rendering with options

| Function | Input | Output |
|----------|-------|--------|
| `render_html_with_options(input, options)` | ChordPro string + options | HTML string |
| `render_text_with_options(input, options)` | ChordPro string + options | Plain text string |
| `render_pdf_with_options(input, options)` | ChordPro string + options | `Uint8Array` (PDF bytes) |

### Options

```ts
interface RenderOptions {
  transpose?: number;  // Semitone offset (any integer in i8 range; renderer reduces mod 12), default 0
  config?: string;     // Preset name ("guitar", "ukulele") or inline RRJSON
}
```

### Validation

| Function | Input | Output |
|----------|-------|--------|
| `validate(input)` | ChordPro string | `string[]` — parse errors (empty if valid) |

### Utility

| Function | Output |
|----------|--------|
| `version()` | Library version string |

## Building from source

```bash
# From the repository root
cd packages/npm && npm run build
```

## License

MIT
