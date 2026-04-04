# chordpro-render-pdf

PDF renderer for [ChordPro](https://www.chordpro.org/) documents.
Generates PDF files with chord diagrams, multi-page layout, and
configurable formatting.

Part of the [chordpro-rs](https://github.com/koedame/chordpro-rs) project.

## Usage

```rust
use chordpro_core::parser::parse;
use chordpro_render_pdf::render_song;

let input = "{title: Amazing Grace}\n[G]Amazing [G7]grace";
let song = parse(input).unwrap();
let pdf_bytes = render_song(&song);

std::fs::write("output.pdf", &pdf_bytes).unwrap();
```

## Features

- A4 page layout with Helvetica font
- Multi-page support with page breaks
- Chord diagram rendering
- Multi-column layout
- Image embedding
- Font size and color configuration

## Documentation

[API documentation on docs.rs](https://docs.rs/chordpro-render-pdf)

## License

[MIT](../../LICENSE)
