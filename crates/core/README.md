<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# chordsketch-core

Parser, AST, and transforms for the [ChordPro](https://www.chordpro.org/)
file format. This crate has **zero external dependencies**.

Part of the [ChordSketch](https://github.com/koedame/chordsketch) project.

## Usage

```rust
use chordsketch_core::parser::parse;

let input = "{title: Amazing Grace}\n[G]Amazing [G7]grace";
let song = parse(input).unwrap();

assert_eq!(song.metadata.title.as_deref(), Some("Amazing Grace"));
```

## Features

- Full ChordPro format parser (100+ directive types)
- Structured AST representation
- Chord transposition
- Configuration system with RRJSON support
- Multi-song file parsing (`{new_song}`)
- Inline markup, delegate environments, conditional selectors

## Documentation

[API documentation on docs.rs](https://docs.rs/chordsketch-core)

## Links

- Project repository: <https://github.com/koedame/chordsketch>
- Live playground: <https://chordsketch.koeda.me>
- API docs: <https://docs.rs/chordsketch-core>
- Issue tracker: <https://github.com/koedame/chordsketch/issues>

## License

[MIT](../../LICENSE)
