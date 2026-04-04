# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - Unreleased

Initial release of chordpro-rs.

### Added

#### Core Parser (`chordpro-core`)

- Full ChordPro file format parser with zero external dependencies
- 100+ directive types supported
- Structured AST representation of songs
- Chord transposition (by semitone count)
- Metadata extraction (`{title}`, `{subtitle}`, `{artist}`, `{key}`, `{meta}`, etc.)
- Section environments: verse, chorus, tab, grid, custom sections
- Chorus recall (`{chorus}`)
- Inline markup (bold, italic, superscript, subscript)
- Delegate environments (ABC, Lilypond, SVG, textblock)
- Conditional directive selectors (instrument, user)
- Multi-song file support (`{new_song}` / `{ns}`)
- Font, size, and color directives (legacy formatting)
- Image directive
- Chord definition and diagram directives (`{define}`, `{chord}`)
- Configuration file system with RRJSON support
- `{transpose}` directive
- Input size limits and parser safety controls

#### Text Renderer (`chordpro-render-text`)

- Plain text output with chords above lyrics
- Unicode-aware column alignment
- Multi-column layout support
- Section label rendering

#### HTML Renderer (`chordpro-render-html`)

- Self-contained HTML5 document output
- Chord positioning above lyrics
- Metadata display (title, subtitle, artist)
- Section styling

#### PDF Renderer (`chordpro-render-pdf`)

- PDF document generation (A4, Helvetica)
- Multi-page layout with page breaks
- Chord diagrams rendering
- Multi-column layout
- Text clipping at column boundaries
- Image embedding
- Font size and color support

#### CLI (`chordpro-rs`)

- Three output formats: text, HTML, PDF
- Chord transposition via `--transpose`
- Configuration file loading via `--config`
- Runtime config overrides via `--define`
- Instrument selection via `--instrument`
- Multiple input file processing
- Optional default config suppression via `--no-default-configs`

### Security

- Input size limits to prevent memory exhaustion
- Path traversal protections for file operations
- No unsafe code in the core parser

### Compatibility

- Tested against the Perl ChordPro reference implementation
- See [docs/known-deviations.md](docs/known-deviations.md) for known differences

[0.1.0]: https://github.com/koedame/chordpro-rs/releases/tag/v0.1.0
