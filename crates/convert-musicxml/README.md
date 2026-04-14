<p align="center">
  <img src="https://raw.githubusercontent.com/koedame/chordsketch/main/assets/logo.svg" alt="ChordSketch" width="80" height="80">
</p>

# chordsketch-convert-musicxml

MusicXML 4.0 to ChordPro bidirectional converter.

Part of the [ChordSketch](https://github.com/koedame/chordsketch) project.

## Usage

```rust
use chordsketch_convert_musicxml::{from_musicxml, to_musicxml};
use chordsketch_core::ast::{Song, Chord, Line, LyricsLine, LyricsSegment};

// Build a simple song
let mut song = Song::new();
song.metadata.title = Some("My Song".to_string());
let mut ll = LyricsLine::new();
ll.segments = vec![
    LyricsSegment::new(Some(Chord::new("C")), "Hello "),
    LyricsSegment::new(Some(Chord::new("G")), "world"),
];
song.lines.push(Line::Lyrics(ll));

// Export to MusicXML
let xml = to_musicxml(&song);

// Import back from MusicXML
let reimported = from_musicxml(&xml).unwrap();
assert_eq!(reimported.metadata.title.as_deref(), Some("My Song"));
```

## API

| Function | Input | Output |
|----------|-------|--------|
| `from_musicxml(xml)` | MusicXML string | `Result<Song, ImportError>` |
| `to_musicxml(song)` | `&Song` | MusicXML string |

## What is preserved across a round-trip

- Song title and composer/artist metadata
- Key signature and tempo
- Chord symbols (root, accidental, quality, extension, bass note)
- Lyrics text
- Section structure (verse/chorus/bridge)

## Limitations

- Only the first `<part>` of a multi-part MusicXML file is imported
- Rhythmic values are not preserved: all exported notes are whole notes
- Staff notation (pitches, durations, articulations) is discarded on import
- Chord diagrams, PDF layout settings, and inline markup are not exported

## Links

- [ChordSketch repository](https://github.com/koedame/chordsketch)
- [Playground](https://chordsketch.koeda.me)
- [Issue tracker](https://github.com/koedame/chordsketch/issues)

## License

MIT
