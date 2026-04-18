<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# chordsketch-render-html

HTML renderer for [ChordPro](https://www.chordpro.org/) documents.
Produces self-contained HTML5 documents with chords positioned above
lyrics.

Part of the [ChordSketch](https://github.com/koedame/chordsketch) project.

## Usage

```rust
use chordsketch_core::parser::parse;
use chordsketch_render_html::render_song;

let input = "{title: Amazing Grace}\n[G]Amazing [G7]grace";
let song = parse(input).unwrap();
let html = render_song(&song);
```

## Features

- Self-contained HTML5 output
- Chord positioning above lyrics
- Metadata display (title, subtitle, artist)
- Section styling
- HTML escaping for user-provided text content (note: delegate
  environments such as `{start_of_svg}` emit raw HTML by design;
  use a Content Security Policy when rendering untrusted input)

## Links

- Project repository: <https://github.com/koedame/chordsketch>
- Live playground: <https://chordsketch.koeda.me>
- API docs: <https://docs.rs/chordsketch-render-html>
- Issue tracker: <https://github.com/koedame/chordsketch/issues>

## License

[MIT](../../LICENSE)
