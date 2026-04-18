<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# chordsketch-render-pdf

PDF renderer for [ChordPro](https://www.chordpro.org/) documents.
Generates PDF files with chord diagrams, multi-page layout, and
configurable formatting.

Part of the [ChordSketch](https://github.com/koedame/chordsketch) project.

## Usage

```rust
use chordsketch_core::parser::parse;
use chordsketch_render_pdf::render_song;

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

## Links

- Project repository: <https://github.com/koedame/chordsketch>
- Live playground: <https://chordsketch.koeda.me>
- API docs: <https://docs.rs/chordsketch-render-pdf>
- Issue tracker: <https://github.com/koedame/chordsketch/issues>

## License

[MIT](../../LICENSE)
