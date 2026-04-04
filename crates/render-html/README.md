# chordpro-render-html

HTML renderer for [ChordPro](https://www.chordpro.org/) documents.
Produces self-contained HTML5 documents with chords positioned above
lyrics.

Part of the [chordpro-rs](https://github.com/koedame/chordpro-rs) project.

## Usage

```rust
use chordpro_core::parser::parse;
use chordpro_render_html::render_song;

let input = "{title: Amazing Grace}\n[G]Amazing [G7]grace";
let song = parse(input).unwrap();
let html = render_song(&song);
```

## Features

- Self-contained HTML5 output
- Chord positioning above lyrics
- Metadata display (title, subtitle, artist)
- Section styling
- HTML escaping for safe output

## Documentation

[API documentation on docs.rs](https://docs.rs/chordpro-render-html)

## License

[MIT](../../LICENSE)
