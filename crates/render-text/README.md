<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# chordsketch-render-text

Plain text renderer for [ChordPro](https://www.chordpro.org/) documents.
Renders songs with chords positioned above lyrics using Unicode-aware
column alignment.

Part of the [ChordSketch](https://github.com/koedame/chordsketch) project.

## Usage

```rust
use chordsketch_core::parser::parse;
use chordsketch_render_text::render_song;

let input = "{title: Amazing Grace}\n[G]Amazing [G7]grace";
let song = parse(input).unwrap();
let text = render_song(&song);

println!("{text}");
```

## Features

- Chords above lyrics with Unicode-aware alignment
- Multi-column layout
- Section labels (verse, chorus, etc.)

## Documentation

[API documentation on docs.rs](https://docs.rs/chordsketch-render-text)

## License

[MIT](../../LICENSE)
