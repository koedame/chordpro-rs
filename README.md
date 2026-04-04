# chordpro-rs

A Rust implementation of the [ChordPro](https://www.chordpro.org/) file format
parser and renderer. Supports parsing ChordPro files into a structured AST and
rendering to plain text, HTML, and PDF.

## Features

- Full [ChordPro](https://www.chordpro.org/chordpro/) format parser with zero
  external dependencies in the core crate
- Three output formats: plain text, HTML, and PDF
- Chord transposition
- Configuration file system (chordpro.json)
- Inline markup (bold, italic, etc.)
- Chord diagrams and extended `{define}` directives
- Section environments (verse, chorus, tab, grid, custom)
- Delegate environments (ABC, Lilypond, SVG, textblock)
- Conditional directive selectors (instrument, user)
- Multi-song files (`{new_song}`)
- Font, size, and color directives
- Image directive
- Multi-page PDF with page control

## Installation

### From crates.io

```bash
cargo install chordpro-rs
```

### From source

Requires Rust 1.85 or later.

```bash
git clone https://github.com/koedame/chordpro-rs.git
cd chordpro-rs
cargo install --path crates/cli
```

## Usage

```bash
# Render to plain text (default)
chordpro song.cho

# Render to HTML
chordpro -f html song.cho -o song.html

# Render to PDF
chordpro -f pdf song.cho -o song.pdf

# Transpose up 2 semitones
chordpro --transpose 2 song.cho

# Use a custom config file
chordpro -c myconfig.json song.cho

# Process multiple files
chordpro -f pdf song1.cho song2.cho -o songbook.pdf
```

## Library Usage

The core parser and renderers are available as separate library crates:

```rust
use chordpro_core::parser::parse;
use chordpro_render_text::render_song;

let input = "{title: Amazing Grace}\n{subtitle: Traditional}\n\n[G]Amazing [G7]grace, how [C]sweet the [G]sound";
let song = parse(input).unwrap();
let text = render_song(&song);
println!("{text}");
```

## Workspace Structure

| Crate | Description |
|---|---|
| [`chordpro-core`](crates/core) | Parser, AST, and transforms (zero external dependencies) |
| [`chordpro-render-text`](crates/render-text) | Plain text renderer |
| [`chordpro-render-html`](crates/render-html) | HTML renderer |
| [`chordpro-render-pdf`](crates/render-pdf) | PDF renderer |
| [`chordpro-rs`](crates/cli) | Command-line tool |

## Links

- [ChordPro file format specification](https://www.chordpro.org/chordpro/)
- [Configuration guide](docs/configuration.md)
- [Versioning and release process](docs/releasing.md)
- [SECURITY.md](SECURITY.md)
- [CHANGELOG.md](CHANGELOG.md)

## License

[MIT](LICENSE)
