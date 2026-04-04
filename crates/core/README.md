# chordpro-core

Parser, AST, and transforms for the [ChordPro](https://www.chordpro.org/)
file format. This crate has **zero external dependencies**.

Part of the [chordpro-rs](https://github.com/koedame/chordpro-rs) project.

## Usage

```rust
use chordpro_core::parser::parse;

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

[API documentation on docs.rs](https://docs.rs/chordpro-core)

## License

[MIT](../../LICENSE)
