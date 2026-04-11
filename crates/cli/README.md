<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# chordsketch

Command-line tool for rendering [ChordPro](https://www.chordpro.org/)
files to plain text, HTML, and PDF.

Part of the [ChordSketch](https://github.com/koedame/chordsketch) project.

## Installation

```bash
cargo install chordsketch
```

## Quick Start

```bash
# Render to plain text (default)
chordsketch song.cho

# Render to HTML
chordsketch -f html song.cho -o song.html

# Render to PDF
chordsketch -f pdf song.cho -o song.pdf

# Transpose up 2 semitones
chordsketch --transpose 2 song.cho
```

## Features

- Three output formats: text, HTML, PDF
- Chord transposition
- Configuration file support (RRJSON)
- Instrument selector filtering
- Multi-file processing

See `chordsketch --help` for all options.

## Documentation

[Full documentation on GitHub](https://github.com/koedame/chordsketch)

## License

[MIT](../../LICENSE)
