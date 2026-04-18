<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# chordsketch-wasm

The Rust-side WebAssembly bindings crate for [ChordSketch](https://github.com/koedame/chordsketch).
It wraps `chordsketch-core` and the three renderers with `wasm-bindgen`
entry points that are consumed from JavaScript/TypeScript.

**If you want to use ChordSketch from Node.js or a browser, install
[`@chordsketch/wasm`](https://www.npmjs.com/package/@chordsketch/wasm)
from npm instead.** That package is built from this crate and ships the
dual-package (ESM + CommonJS) layout, type definitions, and pre-compiled
`.wasm` binary. This crate is the source this README lives next to; it
is published to crates.io primarily so that `docs.rs` can render the
Rust-side API and so that other Rust crates can compile the same bindings
with different `wasm-pack` targets if needed.

Part of the [ChordSketch](https://github.com/koedame/chordsketch) project.

## JavaScript/TypeScript usage (npm)

```bash
npm install @chordsketch/wasm
```

```js
import { render_html, render_text, render_pdf, version } from '@chordsketch/wasm';

const chordpro = `{title: Amazing Grace}
{key: G}

[G]Amazing [G7]grace, how [C]sweet the [G]sound`;

const html = render_html(chordpro);
const text = render_text(chordpro);
const pdfBytes = render_pdf(chordpro); // Uint8Array
console.log(version());
```

See [`packages/npm/README.md`](https://github.com/koedame/chordsketch/blob/main/packages/npm/README.md)
for browser-vs-Node specifics, options (`transpose`, `config`), and the
dual-package resolution rules.

## Building the crate locally (Rust developers only)

```bash
wasm-pack build crates/wasm --target web
```

The canonical build script used by the playground and npm publishing is
`packages/npm/scripts/build.mjs` (runs `wasm-pack` twice — once for the
browser target and once for the Node target — and merges the output into
a dual-package layout).

## API

The exported wasm-bindgen functions are:

| Function | Description | Return type |
|---|---|---|
| `render_text(input)` | Render ChordPro to plain text | `string` |
| `render_html(input)` | Render ChordPro to HTML5 | `string` |
| `render_pdf(input)` | Render ChordPro to a PDF byte array | `Uint8Array` |
| `render_text_with_options(input, opts)` | Same, with `{ transpose?, config? }` | `string` |
| `render_html_with_options(input, opts)` | Same, with `{ transpose?, config? }` | `string` |
| `render_pdf_with_options(input, opts)` | Same, with `{ transpose?, config? }` | `Uint8Array` |
| `render_songs_with_warnings(input, opts?)` | Multi-song variant that returns warnings | `{ songs: string[], warnings: Warning[] }` |
| `version()` | Library version string | `string` |

## Links

- Project repository: <https://github.com/koedame/chordsketch>
- npm package: <https://www.npmjs.com/package/@chordsketch/wasm>
- Live playground: <https://chordsketch.koeda.me>
- API docs: <https://docs.rs/chordsketch-wasm>
- Issue tracker: <https://github.com/koedame/chordsketch/issues>

## License

[MIT](../../LICENSE)
