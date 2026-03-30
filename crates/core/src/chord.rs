//! Chord notation parser for structured chord analysis.
//!
//! This module parses chord strings like `"Am"`, `"C#m7"`, `"G/B"`, and
//! `"Dsus4"` into structured components: root note, accidental, quality,
//! extensions, and bass note.
//!
//! Parsing is best-effort: if a chord string cannot be parsed structurally,
//! [`parse_chord`] returns `None` and callers should fall back to storing
//! the raw string only.

// ---------------------------------------------------------------------------
// Note
// ---------------------------------------------------------------------------

/// A musical note name (A through G).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Note {
    /// The note C.
    C,
    /// The note D.
    D,
    /// The note E.
    E,
    /// The note F.
    F,
    /// The note G.
    G,
    /// The note A.
    A,
    /// The note B.
    B,
}

impl Note {
    /// Parses a single character into a `Note`, if valid.
    fn from_char(c: char) -> Option<Self> {
        match c {
            'C' => Some(Self::C),
            'D' => Some(Self::D),
            'E' => Some(Self::E),
            'F' => Some(Self::F),
            'G' => Some(Self::G),
            'A' => Some(Self::A),
            'B' => Some(Self::B),
            _ => None,
        }
    }
}

impl core::fmt::Display for Note {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Self::C => "C",
            Self::D => "D",
            Self::E => "E",
            Self::F => "F",
            Self::G => "G",
            Self::A => "A",
            Self::B => "B",
        };
        f.write_str(s)
    }
}

// ---------------------------------------------------------------------------
// Accidental
// ---------------------------------------------------------------------------

/// A sharp or flat modifier on a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Accidental {
    /// Sharp (`#`).
    Sharp,
    /// Flat (`b`).
    Flat,
}

impl core::fmt::Display for Accidental {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Sharp => f.write_str("#"),
            Self::Flat => f.write_str("b"),
        }
    }
}

// ---------------------------------------------------------------------------
// ChordQuality
// ---------------------------------------------------------------------------

/// The quality (type) of a chord.
///
/// This enum captures the most common chord qualities found in popular music
/// notation. Extended or unusual qualities are handled via the `extension`
/// field on [`ChordDetail`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChordQuality {
    /// Major chord (default when no quality marker is present).
    Major,
    /// Minor chord (`m` or `min`).
    Minor,
    /// Diminished chord (`dim` or `°`).
    Diminished,
    /// Augmented chord (`aug` or `+`).
    Augmented,
}

impl core::fmt::Display for ChordQuality {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Major => Ok(()),
            Self::Minor => f.write_str("m"),
            Self::Diminished => f.write_str("dim"),
            Self::Augmented => f.write_str("aug"),
        }
    }
}

// ---------------------------------------------------------------------------
// ChordDetail
// ---------------------------------------------------------------------------

/// Structured representation of a parsed chord.
///
/// This contains the individual components extracted from a chord string.
/// Not all chords can be parsed into this structure; for unparseable chords
/// the AST [`Chord`](crate::ast::Chord) falls back to storing only the raw
/// string.
///
/// # Examples
///
/// ```
/// use chordpro_core::chord::{ChordDetail, Note, Accidental, ChordQuality, parse_chord};
///
/// let detail = parse_chord("C#m7").unwrap();
/// assert_eq!(detail.root, Note::C);
/// assert_eq!(detail.root_accidental, Some(Accidental::Sharp));
/// assert_eq!(detail.quality, ChordQuality::Minor);
/// assert_eq!(detail.extension.as_deref(), Some("7"));
/// assert!(detail.bass_note.is_none());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChordDetail {
    /// The root note of the chord (e.g., C, D, E).
    pub root: Note,
    /// An optional sharp or flat on the root note.
    pub root_accidental: Option<Accidental>,
    /// The chord quality (major, minor, diminished, augmented).
    pub quality: ChordQuality,
    /// Optional extension string (e.g., `"7"`, `"maj7"`, `"9"`, `"sus4"`,
    /// `"add9"`, `"7sus4"`).
    ///
    /// Extensions are stored as-is from the source rather than being parsed
    /// into a sub-structure, since the variety of extensions in real-world
    /// chord charts is vast.
    pub extension: Option<String>,
    /// The bass note for slash chords (e.g., `B` in `G/B`).
    pub bass_note: Option<(Note, Option<Accidental>)>,
}

impl core::fmt::Display for ChordDetail {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.root)?;
        if let Some(ref acc) = self.root_accidental {
            write!(f, "{acc}")?;
        }
        write!(f, "{}", self.quality)?;
        if let Some(ref ext) = self.extension {
            f.write_str(ext)?;
        }
        if let Some((ref bass, ref bass_acc)) = self.bass_note {
            write!(f, "/{bass}")?;
            if let Some(acc) = bass_acc {
                write!(f, "{acc}")?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parses a chord string into a [`ChordDetail`].
///
/// Returns `None` if the string cannot be recognized as a valid chord
/// notation. This function is intentionally lenient: it extracts as much
/// structure as it can, and stores remaining text as the extension.
///
/// # Supported formats
///
/// - Basic chords: `C`, `Am`, `G`
/// - Accidentals: `C#`, `Db`, `F#m`
/// - Extended chords: `Cmaj7`, `Am7`, `G9`, `Dsus4`, `Cadd9`
/// - Slash chords: `G/B`, `C/E`, `Am7/G`
/// - Diminished/augmented: `Bdim`, `Cdim7`, `Faug`, `C+`
///
/// # Examples
///
/// ```
/// use chordpro_core::chord::{parse_chord, Note, ChordQuality};
///
/// let detail = parse_chord("Am").unwrap();
/// assert_eq!(detail.root, Note::A);
/// assert_eq!(detail.quality, ChordQuality::Minor);
///
/// // Unparseable strings return None
/// assert!(parse_chord("").is_none());
/// assert!(parse_chord("xyz").is_none());
/// ```
#[must_use]
pub fn parse_chord(input: &str) -> Option<ChordDetail> {
    let mut chars = input.chars().peekable();

    // --- Root note ---
    let root = Note::from_char(*chars.peek()?)?;
    chars.next();

    // --- Root accidental ---
    let root_accidental = match chars.peek() {
        Some('#') => {
            chars.next();
            Some(Accidental::Sharp)
        }
        Some('b') => {
            // Distinguish 'b' as flat from 'b' starting a quality/extension.
            // 'b' is flat only when it is NOT the start of a recognized
            // quality or extension token that begins with 'b'. In practice,
            // after a root note, 'b' is always flat because quality markers
            // don't start with 'b' (minor = 'm', dim = 'd', aug = 'a').
            // However, the note 'B' followed by 'b' (like in "Bb") is flat.
            chars.next();
            Some(Accidental::Flat)
        }
        _ => None,
    };

    // Collect the remaining characters (before any slash for bass note).
    let rest: String = chars.collect();

    // --- Split off bass note (slash chord) ---
    let (quality_ext_str, bass_str) = if let Some(slash_pos) = rest.find('/') {
        let (before, after) = rest.split_at(slash_pos);
        // `after` starts with '/', skip it.
        (before, Some(&after[1..]))
    } else {
        (rest.as_str(), None)
    };

    // --- Parse bass note ---
    let bass_note = if let Some(bass) = bass_str {
        parse_note_with_accidental(bass)
    } else {
        None
    };

    // If a bass string was given but couldn't be parsed, the chord is invalid.
    if bass_str.is_some() && bass_note.is_none() {
        // Could be something like "G/unknown" — treat as unparseable.
        // Exception: empty bass string (trailing slash) is also invalid.
        return None;
    }

    // --- Parse quality and extension from the remaining string ---
    let (quality, extension) = parse_quality_and_extension(quality_ext_str);

    Some(ChordDetail {
        root,
        root_accidental,
        quality,
        extension,
        bass_note,
    })
}

/// Parses a note letter optionally followed by `#` or `b`.
fn parse_note_with_accidental(s: &str) -> Option<(Note, Option<Accidental>)> {
    let mut chars = s.chars();
    let note = Note::from_char(chars.next()?)?;
    let accidental = match chars.next() {
        Some('#') => Some(Accidental::Sharp),
        Some('b') => Some(Accidental::Flat),
        Some(_) => return None, // unexpected character after note
        None => None,
    };
    // There should be nothing left after note + optional accidental.
    if chars.next().is_some() {
        return None;
    }
    Some((note, accidental))
}

/// Parses the quality and extension from the portion of a chord string after
/// the root note and accidental, but before any slash.
///
/// Returns `(quality, extension)`.
fn parse_quality_and_extension(s: &str) -> (ChordQuality, Option<String>) {
    if s.is_empty() {
        return (ChordQuality::Major, None);
    }

    // Try to match quality prefixes in order of specificity (longest first).
    // Note: we must be careful with "m" — it can be the start of "maj" or "min"
    // which have different meanings.

    // Diminished
    if let Some(rest) = s.strip_prefix("dim") {
        let ext = non_empty_string(rest);
        return (ChordQuality::Diminished, ext);
    }

    // Augmented (word form)
    if let Some(rest) = s.strip_prefix("aug") {
        let ext = non_empty_string(rest);
        return (ChordQuality::Augmented, ext);
    }

    // Augmented (symbol form: "+")
    if let Some(rest) = s.strip_prefix('+') {
        let ext = non_empty_string(rest);
        return (ChordQuality::Augmented, ext);
    }

    // "min" — minor (before "m" to avoid premature match)
    if let Some(rest) = s.strip_prefix("min") {
        let ext = non_empty_string(rest);
        return (ChordQuality::Minor, ext);
    }

    // "maj" — major with an extension (e.g., "maj7")
    if let Some(rest) = s.strip_prefix("maj") {
        // "maj" by itself or "maj7", "maj9", etc.
        let ext = if rest.is_empty() {
            Some("maj".to_string())
        } else {
            Some(format!("maj{rest}"))
        };
        return (ChordQuality::Major, ext);
    }

    // "m" — minor (must come after "maj" and "min" checks)
    if let Some(rest) = s.strip_prefix('m') {
        // Make sure 'm' is not followed by something that indicates it's not
        // a minor marker. For chord notation, 'm' is always minor.
        let ext = non_empty_string(rest);
        return (ChordQuality::Minor, ext);
    }

    // "sus" — treated as major with sus extension
    if s.starts_with("sus") {
        return (ChordQuality::Major, Some(s.to_string()));
    }

    // "add" — treated as major with add extension
    if s.starts_with("add") {
        return (ChordQuality::Major, Some(s.to_string()));
    }

    // Numeric extension on a major chord (e.g., "7", "9", "11", "13", "6")
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        return (ChordQuality::Major, Some(s.to_string()));
    }

    // "°" — diminished symbol
    if let Some(rest) = s.strip_prefix('°') {
        let ext = non_empty_string(rest);
        return (ChordQuality::Diminished, ext);
    }

    // If nothing matched, store the entire remaining string as extension
    // on a major chord. This handles unusual notations gracefully.
    (ChordQuality::Major, Some(s.to_string()))
}

/// Returns `Some(s.to_string())` if `s` is non-empty, otherwise `None`.
fn non_empty_string(s: &str) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Helper --------------------------------------------------------------

    /// Shorthand: parse and unwrap.
    fn pd(input: &str) -> ChordDetail {
        parse_chord(input).unwrap_or_else(|| panic!("expected Some for chord '{input}'"))
    }

    // -- Basic major chords --------------------------------------------------

    #[test]
    fn basic_major_chords() {
        for (input, expected_root) in [
            ("C", Note::C),
            ("D", Note::D),
            ("E", Note::E),
            ("F", Note::F),
            ("G", Note::G),
            ("A", Note::A),
            ("B", Note::B),
        ] {
            let detail = pd(input);
            assert_eq!(detail.root, expected_root, "root for '{input}'");
            assert_eq!(detail.root_accidental, None, "accidental for '{input}'");
            assert_eq!(detail.quality, ChordQuality::Major, "quality for '{input}'");
            assert_eq!(detail.extension, None, "extension for '{input}'");
            assert_eq!(detail.bass_note, None, "bass for '{input}'");
        }
    }

    // -- Minor chords --------------------------------------------------------

    #[test]
    fn minor_chords() {
        let detail = pd("Am");
        assert_eq!(detail.root, Note::A);
        assert_eq!(detail.quality, ChordQuality::Minor);
        assert_eq!(detail.extension, None);

        let detail = pd("Em");
        assert_eq!(detail.root, Note::E);
        assert_eq!(detail.quality, ChordQuality::Minor);

        let detail = pd("Dm");
        assert_eq!(detail.root, Note::D);
        assert_eq!(detail.quality, ChordQuality::Minor);
    }

    #[test]
    fn minor_with_min_suffix() {
        let detail = pd("Amin");
        assert_eq!(detail.root, Note::A);
        assert_eq!(detail.quality, ChordQuality::Minor);
        assert_eq!(detail.extension, None);
    }

    // -- Accidentals ---------------------------------------------------------

    #[test]
    fn sharp_major() {
        let detail = pd("C#");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.root_accidental, Some(Accidental::Sharp));
        assert_eq!(detail.quality, ChordQuality::Major);
    }

    #[test]
    fn flat_major() {
        let detail = pd("Db");
        assert_eq!(detail.root, Note::D);
        assert_eq!(detail.root_accidental, Some(Accidental::Flat));
        assert_eq!(detail.quality, ChordQuality::Major);
    }

    #[test]
    fn sharp_minor() {
        let detail = pd("F#m");
        assert_eq!(detail.root, Note::F);
        assert_eq!(detail.root_accidental, Some(Accidental::Sharp));
        assert_eq!(detail.quality, ChordQuality::Minor);
    }

    #[test]
    fn flat_minor() {
        let detail = pd("Bbm");
        assert_eq!(detail.root, Note::B);
        assert_eq!(detail.root_accidental, Some(Accidental::Flat));
        assert_eq!(detail.quality, ChordQuality::Minor);
    }

    #[test]
    fn bb_flat() {
        let detail = pd("Bb");
        assert_eq!(detail.root, Note::B);
        assert_eq!(detail.root_accidental, Some(Accidental::Flat));
        assert_eq!(detail.quality, ChordQuality::Major);
    }

    // -- Extended chords -----------------------------------------------------

    #[test]
    fn major_seventh() {
        let detail = pd("Cmaj7");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("maj7"));
    }

    #[test]
    fn minor_seventh() {
        let detail = pd("Am7");
        assert_eq!(detail.root, Note::A);
        assert_eq!(detail.quality, ChordQuality::Minor);
        assert_eq!(detail.extension.as_deref(), Some("7"));
    }

    #[test]
    fn dominant_seventh() {
        let detail = pd("G7");
        assert_eq!(detail.root, Note::G);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("7"));
    }

    #[test]
    fn ninth_chord() {
        let detail = pd("G9");
        assert_eq!(detail.root, Note::G);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("9"));
    }

    #[test]
    fn sus4() {
        let detail = pd("Dsus4");
        assert_eq!(detail.root, Note::D);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("sus4"));
    }

    #[test]
    fn sus2() {
        let detail = pd("Asus2");
        assert_eq!(detail.root, Note::A);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("sus2"));
    }

    #[test]
    fn add9() {
        let detail = pd("Cadd9");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("add9"));
    }

    #[test]
    fn minor_major_seventh() {
        // Cm followed by "maj7" — the 'm' is consumed as minor, then "aj7"
        // becomes the extension. This is a known limitation.
        // Actually let's check what happens:
        let detail = pd("Cmmaj7");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.quality, ChordQuality::Minor);
        // After stripping 'm', rest is "maj7"
        assert_eq!(detail.extension.as_deref(), Some("maj7"));
    }

    #[test]
    fn seventh_sus4() {
        let detail = pd("G7sus4");
        assert_eq!(detail.root, Note::G);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("7sus4"));
    }

    #[test]
    fn sixth_chord() {
        let detail = pd("C6");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("6"));
    }

    #[test]
    fn minor_sixth() {
        let detail = pd("Am6");
        assert_eq!(detail.root, Note::A);
        assert_eq!(detail.quality, ChordQuality::Minor);
        assert_eq!(detail.extension.as_deref(), Some("6"));
    }

    #[test]
    fn eleventh_chord() {
        let detail = pd("G11");
        assert_eq!(detail.root, Note::G);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("11"));
    }

    #[test]
    fn thirteenth_chord() {
        let detail = pd("C13");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("13"));
    }

    // -- Diminished and augmented --------------------------------------------

    #[test]
    fn diminished() {
        let detail = pd("Bdim");
        assert_eq!(detail.root, Note::B);
        assert_eq!(detail.quality, ChordQuality::Diminished);
        assert_eq!(detail.extension, None);
    }

    #[test]
    fn diminished_seventh() {
        let detail = pd("Cdim7");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.quality, ChordQuality::Diminished);
        assert_eq!(detail.extension.as_deref(), Some("7"));
    }

    #[test]
    fn diminished_symbol() {
        let detail = pd("B°");
        assert_eq!(detail.root, Note::B);
        assert_eq!(detail.quality, ChordQuality::Diminished);
        assert_eq!(detail.extension, None);
    }

    #[test]
    fn augmented() {
        let detail = pd("Faug");
        assert_eq!(detail.root, Note::F);
        assert_eq!(detail.quality, ChordQuality::Augmented);
        assert_eq!(detail.extension, None);
    }

    #[test]
    fn augmented_plus_symbol() {
        let detail = pd("C+");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.quality, ChordQuality::Augmented);
        assert_eq!(detail.extension, None);
    }

    #[test]
    fn augmented_seventh() {
        let detail = pd("Caug7");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.quality, ChordQuality::Augmented);
        assert_eq!(detail.extension.as_deref(), Some("7"));
    }

    // -- Slash chords --------------------------------------------------------

    #[test]
    fn slash_chord_simple() {
        let detail = pd("G/B");
        assert_eq!(detail.root, Note::G);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.bass_note, Some((Note::B, None)));
    }

    #[test]
    fn slash_chord_minor() {
        let detail = pd("Am/E");
        assert_eq!(detail.root, Note::A);
        assert_eq!(detail.quality, ChordQuality::Minor);
        assert_eq!(detail.bass_note, Some((Note::E, None)));
    }

    #[test]
    fn slash_chord_with_accidental_bass() {
        let detail = pd("C/Bb");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.bass_note, Some((Note::B, Some(Accidental::Flat))));
    }

    #[test]
    fn slash_chord_with_sharp_bass() {
        let detail = pd("Am/G#");
        assert_eq!(detail.root, Note::A);
        assert_eq!(detail.quality, ChordQuality::Minor);
        assert_eq!(detail.bass_note, Some((Note::G, Some(Accidental::Sharp))));
    }

    #[test]
    fn slash_chord_extended() {
        let detail = pd("Am7/G");
        assert_eq!(detail.root, Note::A);
        assert_eq!(detail.quality, ChordQuality::Minor);
        assert_eq!(detail.extension.as_deref(), Some("7"));
        assert_eq!(detail.bass_note, Some((Note::G, None)));
    }

    #[test]
    fn slash_chord_sharp_root() {
        let detail = pd("F#m/E");
        assert_eq!(detail.root, Note::F);
        assert_eq!(detail.root_accidental, Some(Accidental::Sharp));
        assert_eq!(detail.quality, ChordQuality::Minor);
        assert_eq!(detail.bass_note, Some((Note::E, None)));
    }

    // -- Invalid / unparseable -----------------------------------------------

    #[test]
    fn empty_string() {
        assert!(parse_chord("").is_none());
    }

    #[test]
    fn lowercase_root() {
        // Chord roots must be uppercase.
        assert!(parse_chord("am").is_none());
    }

    #[test]
    fn non_note_root() {
        assert!(parse_chord("Hm").is_none());
        assert!(parse_chord("X").is_none());
    }

    #[test]
    fn numeric_only() {
        assert!(parse_chord("7").is_none());
    }

    #[test]
    fn slash_with_invalid_bass() {
        assert!(parse_chord("G/X").is_none());
        assert!(parse_chord("G/").is_none());
    }

    #[test]
    fn slash_bass_too_long() {
        // Bass should be just a note + optional accidental.
        assert!(parse_chord("G/Bm").is_none());
    }

    #[test]
    fn multi_slash_is_invalid() {
        // Multiple slashes: split on first slash so bass becomes "D/E",
        // which is not a valid note+accidental. The chord is unparseable.
        assert!(parse_chord("C/D/E").is_none());
    }

    // -- Display (round-trip) ------------------------------------------------

    #[test]
    fn display_basic_major() {
        assert_eq!(pd("C").to_string(), "C");
    }

    #[test]
    fn display_minor() {
        assert_eq!(pd("Am").to_string(), "Am");
    }

    #[test]
    fn display_sharp_minor_seventh() {
        assert_eq!(pd("C#m7").to_string(), "C#m7");
    }

    #[test]
    fn display_slash_chord() {
        assert_eq!(pd("G/B").to_string(), "G/B");
    }

    #[test]
    fn display_flat_chord() {
        assert_eq!(pd("Bb").to_string(), "Bb");
    }

    #[test]
    fn display_diminished() {
        assert_eq!(pd("Bdim").to_string(), "Bdim");
    }

    #[test]
    fn display_augmented() {
        assert_eq!(pd("Faug").to_string(), "Faug");
    }

    #[test]
    fn display_sus4() {
        assert_eq!(pd("Dsus4").to_string(), "Dsus4");
    }

    #[test]
    fn display_slash_with_accidental() {
        assert_eq!(pd("C/Bb").to_string(), "C/Bb");
    }

    #[test]
    fn display_complex_chord() {
        assert_eq!(pd("F#m7/E").to_string(), "F#m7/E");
    }

    // -- Edge cases ----------------------------------------------------------

    #[test]
    fn maj_alone() {
        // "Cmaj" — major quality with "maj" as extension
        let detail = pd("Cmaj");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("maj"));
    }

    #[test]
    fn maj9() {
        let detail = pd("Cmaj9");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.quality, ChordQuality::Major);
        assert_eq!(detail.extension.as_deref(), Some("maj9"));
    }

    #[test]
    fn sharp_augmented() {
        let detail = pd("G#+");
        assert_eq!(detail.root, Note::G);
        assert_eq!(detail.root_accidental, Some(Accidental::Sharp));
        assert_eq!(detail.quality, ChordQuality::Augmented);
    }

    #[test]
    fn flat_diminished() {
        let detail = pd("Ebdim");
        assert_eq!(detail.root, Note::E);
        assert_eq!(detail.root_accidental, Some(Accidental::Flat));
        assert_eq!(detail.quality, ChordQuality::Diminished);
    }

    #[test]
    fn minor_add9() {
        let detail = pd("Amadd9");
        assert_eq!(detail.root, Note::A);
        assert_eq!(detail.quality, ChordQuality::Minor);
        // After stripping 'm', rest is "add9"
        assert_eq!(detail.extension.as_deref(), Some("add9"));
    }
}
