//! Chord transposition — shift all chords in a song by N semitones.
//!
//! The [`transpose`] function walks every chord in a [`Song`] and shifts its
//! root (and bass note, for slash chords) by the given number of semitones,
//! wrapping around the 12-tone chromatic scale.
//!
//! Chords whose notation could not be parsed structurally (i.e., `detail` is
//! `None`) are left unchanged — only their raw `name` is preserved.

use crate::ast::{Chord, Line, LyricsLine, LyricsSegment, Song};
use crate::chord::{Accidental, ChordDetail, Note};

/// The chromatic scale using sharps for enharmonic equivalents.
const SHARP_NAMES: [(Note, Option<Accidental>); 12] = [
    (Note::C, None),
    (Note::C, Some(Accidental::Sharp)),
    (Note::D, None),
    (Note::D, Some(Accidental::Sharp)),
    (Note::E, None),
    (Note::F, None),
    (Note::F, Some(Accidental::Sharp)),
    (Note::G, None),
    (Note::G, Some(Accidental::Sharp)),
    (Note::A, None),
    (Note::A, Some(Accidental::Sharp)),
    (Note::B, None),
];

/// The chromatic scale using flats for enharmonic equivalents.
const FLAT_NAMES: [(Note, Option<Accidental>); 12] = [
    (Note::C, None),
    (Note::D, Some(Accidental::Flat)),
    (Note::D, None),
    (Note::E, Some(Accidental::Flat)),
    (Note::E, None),
    (Note::F, None),
    (Note::G, Some(Accidental::Flat)),
    (Note::G, None),
    (Note::A, Some(Accidental::Flat)),
    (Note::A, None),
    (Note::B, Some(Accidental::Flat)),
    (Note::B, None),
];

/// Convert a note + accidental to a semitone index (0 = C, 11 = B).
fn note_to_semitone(note: Note, accidental: Option<Accidental>) -> u8 {
    let base = match note {
        Note::C => 0,
        Note::D => 2,
        Note::E => 4,
        Note::F => 5,
        Note::G => 7,
        Note::A => 9,
        Note::B => 11,
    };
    match accidental {
        Some(Accidental::Sharp) => (base + 1) % 12,
        Some(Accidental::Flat) => (base + 11) % 12,
        None => base,
    }
}

/// Shift a semitone index by the given number of semitones.
fn shift_semitone(semitone: u8, shift: i8) -> u8 {
    ((i16::from(semitone) + i16::from(shift)).rem_euclid(12)) as u8
}

/// Choose sharp or flat spelling based on the original accidental.
///
/// If the original chord used a flat, the transposed result prefers flats.
/// Otherwise it uses sharps. This preserves the musical intent of the
/// original notation.
fn transposed_note(semitone: u8, prefer_flat: bool) -> (Note, Option<Accidental>) {
    if prefer_flat {
        FLAT_NAMES[semitone as usize]
    } else {
        SHARP_NAMES[semitone as usize]
    }
}

/// Transpose a [`ChordDetail`] by the given number of semitones.
///
/// Returns a new `ChordDetail` with the root and bass note shifted.
/// Quality and extensions are preserved.
#[must_use]
pub fn transpose_detail(detail: &ChordDetail, semitones: i8) -> ChordDetail {
    let prefer_flat = detail.root_accidental == Some(Accidental::Flat);

    let root_semitone = note_to_semitone(detail.root, detail.root_accidental);
    let new_semitone = shift_semitone(root_semitone, semitones);
    let (new_root, new_acc) = transposed_note(new_semitone, prefer_flat);

    let new_bass = detail.bass_note.map(|(bass_note, bass_acc)| {
        let bass_prefer_flat = bass_acc == Some(Accidental::Flat);
        let bass_semitone = note_to_semitone(bass_note, bass_acc);
        let new_bass_semitone = shift_semitone(bass_semitone, semitones);
        transposed_note(new_bass_semitone, bass_prefer_flat)
    });

    ChordDetail {
        root: new_root,
        root_accidental: new_acc,
        quality: detail.quality,
        extension: detail.extension.clone(),
        bass_note: new_bass,
    }
}

/// Transpose a single [`Chord`] by the given number of semitones.
///
/// If the chord has a parsed `detail`, both the detail and the display
/// `name` are updated. If the chord could not be parsed (detail is `None`),
/// it is returned unchanged.
#[must_use]
pub fn transpose_chord(chord: &Chord, semitones: i8) -> Chord {
    match &chord.detail {
        Some(detail) => {
            let new_detail = transpose_detail(detail, semitones);
            let new_name = new_detail.to_string();
            Chord {
                name: new_name,
                detail: Some(new_detail),
                display: chord.display.clone(),
            }
        }
        None => chord.clone(),
    }
}

/// Transpose all chords in a [`Song`] by the given number of semitones.
///
/// Returns a new `Song` with every chord shifted. Metadata, directives,
/// comments, and lyrics text are preserved. Chords without a parsed
/// `detail` are left unchanged.
///
/// A shift of 0 produces an equivalent copy.
///
/// # Examples
///
/// ```
/// use chordpro_core::parse;
/// use chordpro_core::transpose::transpose;
///
/// let song = parse("[G]Hello [C]world").unwrap();
/// let transposed = transpose(&song, 2); // up 2 semitones
/// // G → A, C → D
/// let first_line = &transposed.lines[0];
/// if let chordpro_core::ast::Line::Lyrics(l) = first_line {
///     assert_eq!(l.segments[0].chord.as_ref().unwrap().name, "A");
///     assert_eq!(l.segments[1].chord.as_ref().unwrap().name, "D");
/// }
/// ```
#[must_use]
pub fn transpose(song: &Song, semitones: i8) -> Song {
    let new_lines = song
        .lines
        .iter()
        .map(|line| match line {
            Line::Lyrics(lyrics_line) => Line::Lyrics(transpose_lyrics(lyrics_line, semitones)),
            other => other.clone(),
        })
        .collect();

    Song {
        metadata: song.metadata.clone(),
        lines: new_lines,
    }
}

/// Transpose all chords in a lyrics line.
fn transpose_lyrics(lyrics_line: &LyricsLine, semitones: i8) -> LyricsLine {
    LyricsLine {
        segments: lyrics_line
            .segments
            .iter()
            .map(|seg| LyricsSegment {
                spans: seg.spans.clone(),
                chord: seg.chord.as_ref().map(|c| transpose_chord(c, semitones)),
                text: seg.text.clone(),
            })
            .collect(),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::{ChordQuality, parse_chord};

    // --- note_to_semitone ---

    #[test]
    fn test_note_to_semitone() {
        assert_eq!(note_to_semitone(Note::C, None), 0);
        assert_eq!(note_to_semitone(Note::C, Some(Accidental::Sharp)), 1);
        assert_eq!(note_to_semitone(Note::D, Some(Accidental::Flat)), 1);
        assert_eq!(note_to_semitone(Note::D, None), 2);
        assert_eq!(note_to_semitone(Note::E, None), 4);
        assert_eq!(note_to_semitone(Note::F, None), 5);
        assert_eq!(note_to_semitone(Note::G, None), 7);
        assert_eq!(note_to_semitone(Note::A, None), 9);
        assert_eq!(note_to_semitone(Note::B, None), 11);
        assert_eq!(note_to_semitone(Note::B, Some(Accidental::Sharp)), 0);
    }

    // --- transpose_detail ---

    #[test]
    fn test_transpose_c_up_2() {
        let detail = parse_chord("C").unwrap();
        let t = transpose_detail(&detail, 2);
        assert_eq!(t.root, Note::D);
        assert_eq!(t.root_accidental, None);
    }

    #[test]
    fn test_transpose_g_up_2() {
        let detail = parse_chord("G").unwrap();
        let t = transpose_detail(&detail, 2);
        assert_eq!(t.root, Note::A);
        assert_eq!(t.root_accidental, None);
    }

    #[test]
    fn test_transpose_b_up_1_wraps() {
        let detail = parse_chord("B").unwrap();
        let t = transpose_detail(&detail, 1);
        assert_eq!(t.root, Note::C);
        assert_eq!(t.root_accidental, None);
    }

    #[test]
    fn test_transpose_c_down_1() {
        let detail = parse_chord("C").unwrap();
        let t = transpose_detail(&detail, -1);
        assert_eq!(t.root, Note::B);
        assert_eq!(t.root_accidental, None);
    }

    #[test]
    fn test_transpose_preserves_quality() {
        let detail = parse_chord("Am7").unwrap();
        let t = transpose_detail(&detail, 3);
        assert_eq!(t.root, Note::C);
        assert_eq!(t.quality, ChordQuality::Minor);
        assert_eq!(t.extension.as_deref(), Some("7"));
    }

    #[test]
    fn test_transpose_sharp_chord() {
        let detail = parse_chord("F#m").unwrap();
        let t = transpose_detail(&detail, 2);
        assert_eq!(t.root, Note::G);
        assert_eq!(t.root_accidental, Some(Accidental::Sharp));
        assert_eq!(t.quality, ChordQuality::Minor);
    }

    #[test]
    fn test_transpose_flat_preserves_flat_spelling() {
        let detail = parse_chord("Bb").unwrap();
        let t = transpose_detail(&detail, 2);
        // Bb (10) + 2 = 0 = C
        assert_eq!(t.root, Note::C);
        assert_eq!(t.root_accidental, None);
    }

    #[test]
    fn test_transpose_flat_to_flat() {
        let detail = parse_chord("Eb").unwrap();
        let t = transpose_detail(&detail, 2);
        // Eb (3) + 2 = 5 = F
        assert_eq!(t.root, Note::F);
        assert_eq!(t.root_accidental, None);
    }

    #[test]
    fn test_transpose_slash_chord() {
        let detail = parse_chord("G/B").unwrap();
        let t = transpose_detail(&detail, 2);
        assert_eq!(t.root, Note::A);
        assert_eq!(t.bass_note, Some((Note::C, Some(Accidental::Sharp))));
    }

    #[test]
    fn test_transpose_zero_is_noop() {
        let detail = parse_chord("Am7").unwrap();
        let t = transpose_detail(&detail, 0);
        assert_eq!(t, detail);
    }

    #[test]
    fn test_transpose_full_cycle() {
        let detail = parse_chord("C").unwrap();
        let t = transpose_detail(&detail, 12);
        assert_eq!(t.root, Note::C);
        assert_eq!(t.root_accidental, None);
    }

    #[test]
    fn test_transpose_negative_full_cycle() {
        let detail = parse_chord("Am").unwrap();
        let t = transpose_detail(&detail, -12);
        assert_eq!(t, detail);
    }

    // --- transpose_chord ---

    #[test]
    fn test_transpose_chord_updates_name() {
        let chord = Chord::new("G");
        let t = transpose_chord(&chord, 2);
        assert_eq!(t.name, "A");
    }

    #[test]
    fn test_transpose_chord_unparseable_unchanged() {
        let chord = Chord {
            name: "N.C.".to_string(),
            detail: None,
            display: None,
        };
        let t = transpose_chord(&chord, 5);
        assert_eq!(t.name, "N.C.");
        assert!(t.detail.is_none());
    }

    // --- transpose (song-level) ---

    #[test]
    fn test_transpose_song() {
        let song = crate::parse("[G]Hello [C]world").unwrap();
        let t = transpose(&song, 2);
        if let Line::Lyrics(l) = &t.lines[0] {
            assert_eq!(l.segments[0].chord.as_ref().unwrap().name, "A");
            assert_eq!(l.segments[1].chord.as_ref().unwrap().name, "D");
        } else {
            panic!("expected lyrics line");
        }
    }

    #[test]
    fn test_transpose_preserves_lyrics_text() {
        let song = crate::parse("[Am]Hello").unwrap();
        let t = transpose(&song, 3);
        if let Line::Lyrics(l) = &t.lines[0] {
            assert_eq!(l.segments[0].text, "Hello");
            assert_eq!(l.segments[0].chord.as_ref().unwrap().name, "Cm");
        } else {
            panic!("expected lyrics line");
        }
    }

    #[test]
    fn test_transpose_preserves_metadata() {
        let song = crate::parse("{title: Test}\n[G]Hello").unwrap();
        let t = transpose(&song, 5);
        assert_eq!(t.metadata.title.as_deref(), Some("Test"));
    }

    #[test]
    fn test_transpose_all_12_keys() {
        let expected_roots = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        for (i, expected) in expected_roots.iter().enumerate() {
            let detail = parse_chord("C").unwrap();
            let t = transpose_detail(&detail, i as i8);
            assert_eq!(t.to_string(), *expected, "transpose C by {i}");
        }
    }
}
