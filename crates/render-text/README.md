# chordpro-render-text

Plain text renderer for [ChordPro](https://www.chordpro.org/) documents.
Renders songs with chords positioned above lyrics using Unicode-aware
column alignment.

Part of the [chordpro-rs](https://github.com/koedame/chordpro-rs) project.

## Usage

```rust
use chordpro_core::parser::parse;
use chordpro_render_text::render_song;

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

[API documentation on docs.rs](https://docs.rs/chordpro-render-text)

## License

[MIT](../../LICENSE)
