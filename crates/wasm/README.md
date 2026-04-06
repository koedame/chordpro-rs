# chordsketch-wasm

WebAssembly bindings for [ChordSketch](https://github.com/koedame/chordsketch),
exposing ChordPro parsing and rendering to JavaScript/TypeScript.

## API

### Basic rendering

```js
import init, { render_html, render_text, render_pdf, version } from '@chordsketch/wasm';

await init();

const chordpro = `{title: Amazing Grace}
{key: G}

[G]Amazing [G7]grace, how [C]sweet the [G]sound`;

const html = render_html(chordpro);
const text = render_text(chordpro);
const pdfBytes = render_pdf(chordpro); // Uint8Array
console.log(version());
```

### Rendering with options

```js
import init, { render_html_with_options } from '@chordsketch/wasm';

await init();

const html = render_html_with_options(input, {
  transpose: 2,            // semitone offset (-12 to +12)
  config: 'ukulele',       // preset name or inline RRJSON
});
```

## Building

```bash
wasm-pack build crates/wasm --target web
```

## License

MIT
