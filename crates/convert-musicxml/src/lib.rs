//! MusicXML ↔ ChordPro bidirectional converter.
//!
//! This crate provides:
//!
//! - [`from_musicxml`]: parse a MusicXML 4.0 document into a ChordSketch
//!   [`Song`](chordsketch_core::ast::Song) AST
//! - [`to_musicxml`]: serialize a [`Song`](chordsketch_core::ast::Song) to a
//!   MusicXML 4.0 string
//!
//! # What is preserved across a round-trip
//!
//! - Song title and composer/artist metadata
//! - Key signature and tempo
//! - Chord symbols (root, accidental, quality, extension, bass note)
//! - Lyrics text
//! - Section structure (verse/chorus/bridge)
//!
//! # Limitations
//!
//! - Only the first `<part>` of a multi-part MusicXML file is imported.
//! - Rhythmic values are not preserved: all exported notes are whole notes.
//! - Staff notation (pitches, durations, articulations) is discarded on import.
//! - Chord diagrams, PDF layout settings, and inline markup are not exported.
//!
//! # Examples
//!
//! ```rust
//! use chordsketch_convert_musicxml::{from_musicxml, to_musicxml};
//! use chordsketch_core::ast::{Song, Chord, Line, LyricsLine, LyricsSegment};
//!
//! // Build a simple song
//! let mut song = Song::new();
//! song.metadata.title = Some("My Song".to_string());
//! let mut ll = LyricsLine::new();
//! ll.segments = vec![
//!     LyricsSegment::new(Some(Chord::new("C")), "Hello "),
//!     LyricsSegment::new(Some(Chord::new("G")), "world"),
//! ];
//! song.lines.push(Line::Lyrics(ll));
//!
//! // Export to MusicXML
//! let xml = to_musicxml(&song);
//! assert!(xml.contains("<root-step>C</root-step>"));
//! assert!(xml.contains("<text>Hello</text>"));
//!
//! // Import back
//! let reimported = from_musicxml(&xml).unwrap();
//! assert_eq!(reimported.metadata.title.as_deref(), Some("My Song"));
//! ```

pub use export::to_musicxml;
pub use import::{ImportError, from_musicxml};

mod export;
mod import;
mod xml;
