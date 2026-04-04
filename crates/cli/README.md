# chordpro-rs

Command-line tool for rendering [ChordPro](https://www.chordpro.org/)
files to plain text, HTML, and PDF.

Part of the [chordpro-rs](https://github.com/koedame/chordpro-rs) project.

## Installation

```bash
cargo install chordpro-rs
```

## Quick Start

```bash
# Render to plain text (default)
chordpro song.cho

# Render to HTML
chordpro -f html song.cho -o song.html

# Render to PDF
chordpro -f pdf song.cho -o song.pdf

# Transpose up 2 semitones
chordpro --transpose 2 song.cho
```

## Features

- Three output formats: text, HTML, PDF
- Chord transposition
- Configuration file support (RRJSON)
- Instrument selector filtering
- Multi-file processing

See `chordpro --help` for all options.

## Documentation

[Full documentation on GitHub](https://github.com/koedame/chordpro-rs)

## License

[MIT](../../LICENSE)
