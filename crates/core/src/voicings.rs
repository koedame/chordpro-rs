//! Built-in chord voicing database for guitar, ukulele, and piano/keyboard.
//!
//! Provides 156 pre-defined chord voicings:
//! - 60 guitar voicings (5 families × 12 roots)
//! - 36 ukulele voicings (3 families × 12 roots)
//! - 60 keyboard/piano voicings (5 families × 12 roots: major, minor, dom7, maj7, min7)
//!
//! All data is stored as compile-time static data — no external files, no runtime I/O.
//! The lookup priority is:
//!
//! 1. `{define}` in the song file (handled by the parser/AST)
//! 2. User `chordsketch.json` (future — not yet implemented)
//! 3. This built-in database (this module)
//!
//! # Fret encoding
//!
//! `frets` arrays are ordered from the **lowest-pitched string to the
//! highest-pitched string** (guitar: strings 6→1, ukulele: strings 4→1).
//! Values are:
//! - `-1` — muted (×)
//! - `0`  — open string (○)
//! - `1+` — fret number **relative to `base_fret`** (1 = first visible row)
//!
//! # Enharmonic equivalents
//!
//! Each sharp-root chord (A#, C#, D#, F#, G#) is stored once; the lookup
//! function accepts both spellings (e.g., "Bb" resolves to the same entry
//! as "A#").

use crate::chord_diagram::{DEFAULT_FRETS_SHOWN, DiagramData};

// ---------------------------------------------------------------------------
// Internal data type
// ---------------------------------------------------------------------------

struct StaticVoicing {
    name: &'static str,
    base_fret: u32,
    frets: &'static [i32],
}

impl StaticVoicing {
    fn to_diagram(&self) -> DiagramData {
        DiagramData {
            name: self.name.to_string(),
            display_name: None,
            strings: self.frets.len(),
            frets_shown: DEFAULT_FRETS_SHOWN,
            base_fret: self.base_fret,
            frets: self.frets.to_vec(),
            fingers: vec![],
        }
    }
}

// ---------------------------------------------------------------------------
// Guitar voicings (6 strings, standard EADGBE tuning)
//
// Fret order: [string6=low-E, string5=A, string4=D, string3=G, string2=B, string1=high-e]
// ---------------------------------------------------------------------------

/// Guitar major chord voicings (open position where possible, barre otherwise).
///
/// E-shape barre pattern  (relative): [1,3,3,2,1,1]
/// A-shape barre pattern  (relative): [-1,1,3,3,3,1]
const GUITAR_MAJOR: &[StaticVoicing] = &[
    // Open voicings
    StaticVoicing {
        name: "E",
        base_fret: 1,
        frets: &[0, 2, 2, 1, 0, 0],
    },
    StaticVoicing {
        name: "A",
        base_fret: 1,
        frets: &[-1, 0, 2, 2, 2, 0],
    },
    StaticVoicing {
        name: "G",
        base_fret: 1,
        frets: &[3, 2, 0, 0, 0, 3],
    },
    StaticVoicing {
        name: "C",
        base_fret: 1,
        frets: &[-1, 3, 2, 0, 1, 0],
    },
    StaticVoicing {
        name: "D",
        base_fret: 1,
        frets: &[-1, -1, 0, 2, 3, 2],
    },
    // E-shape barre voicings
    StaticVoicing {
        name: "F",
        base_fret: 1,
        frets: &[1, 3, 3, 2, 1, 1],
    },
    StaticVoicing {
        name: "F#",
        base_fret: 2,
        frets: &[1, 3, 3, 2, 1, 1],
    },
    StaticVoicing {
        name: "G#",
        base_fret: 4,
        frets: &[1, 3, 3, 2, 1, 1],
    },
    // A-shape barre voicings
    StaticVoicing {
        name: "A#",
        base_fret: 1,
        frets: &[-1, 1, 3, 3, 3, 1],
    },
    StaticVoicing {
        name: "B",
        base_fret: 2,
        frets: &[-1, 1, 3, 3, 3, 1],
    },
    StaticVoicing {
        name: "C#",
        base_fret: 4,
        frets: &[-1, 1, 3, 3, 3, 1],
    },
    StaticVoicing {
        name: "D#",
        base_fret: 6,
        frets: &[-1, 1, 3, 3, 3, 1],
    },
];

/// Guitar minor chord voicings.
///
/// Em-shape barre pattern (relative): [1,3,3,1,1,1]
/// Am-shape barre pattern (relative): [-1,1,3,3,2,1]
const GUITAR_MINOR: &[StaticVoicing] = &[
    // Open voicings
    StaticVoicing {
        name: "Em",
        base_fret: 1,
        frets: &[0, 2, 2, 0, 0, 0],
    },
    StaticVoicing {
        name: "Am",
        base_fret: 1,
        frets: &[-1, 0, 2, 2, 1, 0],
    },
    StaticVoicing {
        name: "Dm",
        base_fret: 1,
        frets: &[-1, -1, 0, 2, 3, 1],
    },
    // Em-shape barre voicings
    StaticVoicing {
        name: "Fm",
        base_fret: 1,
        frets: &[1, 3, 3, 1, 1, 1],
    },
    StaticVoicing {
        name: "F#m",
        base_fret: 2,
        frets: &[1, 3, 3, 1, 1, 1],
    },
    StaticVoicing {
        name: "Gm",
        base_fret: 3,
        frets: &[1, 3, 3, 1, 1, 1],
    },
    StaticVoicing {
        name: "G#m",
        base_fret: 4,
        frets: &[1, 3, 3, 1, 1, 1],
    },
    // Am-shape barre voicings
    StaticVoicing {
        name: "A#m",
        base_fret: 1,
        frets: &[-1, 1, 3, 3, 2, 1],
    },
    StaticVoicing {
        name: "Bm",
        base_fret: 2,
        frets: &[-1, 1, 3, 3, 2, 1],
    },
    StaticVoicing {
        name: "Cm",
        base_fret: 3,
        frets: &[-1, 1, 3, 3, 2, 1],
    },
    StaticVoicing {
        name: "C#m",
        base_fret: 4,
        frets: &[-1, 1, 3, 3, 2, 1],
    },
    StaticVoicing {
        name: "D#m",
        base_fret: 6,
        frets: &[-1, 1, 3, 3, 2, 1],
    },
];

/// Guitar dominant-7th chord voicings.
///
/// E7-shape barre pattern (relative): [1,3,1,2,1,1]
/// A7-shape barre pattern (relative): [-1,1,3,1,3,1]
const GUITAR_DOM7: &[StaticVoicing] = &[
    // Open voicings
    StaticVoicing {
        name: "E7",
        base_fret: 1,
        frets: &[0, 2, 0, 1, 0, 0],
    },
    StaticVoicing {
        name: "A7",
        base_fret: 1,
        frets: &[-1, 0, 2, 0, 2, 0],
    },
    StaticVoicing {
        name: "G7",
        base_fret: 1,
        frets: &[3, 2, 0, 0, 0, 1],
    },
    StaticVoicing {
        name: "C7",
        base_fret: 1,
        frets: &[-1, 3, 2, 3, 1, 0],
    },
    StaticVoicing {
        name: "D7",
        base_fret: 1,
        frets: &[-1, -1, 0, 2, 1, 2],
    },
    StaticVoicing {
        name: "B7",
        base_fret: 1,
        frets: &[-1, 2, 1, 2, 0, 2],
    },
    // E7-shape barre voicings
    StaticVoicing {
        name: "F7",
        base_fret: 1,
        frets: &[1, 3, 1, 2, 1, 1],
    },
    StaticVoicing {
        name: "F#7",
        base_fret: 2,
        frets: &[1, 3, 1, 2, 1, 1],
    },
    StaticVoicing {
        name: "G#7",
        base_fret: 4,
        frets: &[1, 3, 1, 2, 1, 1],
    },
    // A7-shape barre voicings
    StaticVoicing {
        name: "A#7",
        base_fret: 1,
        frets: &[-1, 1, 3, 1, 3, 1],
    },
    StaticVoicing {
        name: "C#7",
        base_fret: 4,
        frets: &[-1, 1, 3, 1, 3, 1],
    },
    StaticVoicing {
        name: "D#7",
        base_fret: 6,
        frets: &[-1, 1, 3, 1, 3, 1],
    },
];

/// Guitar major-7th chord voicings.
///
/// Emaj7-shape barre pattern (relative): [1,3,2,2,1,1]
/// Amaj7-shape barre pattern (relative): [-1,1,3,2,2,1]
const GUITAR_MAJ7: &[StaticVoicing] = &[
    // Open voicings
    StaticVoicing {
        name: "Emaj7",
        base_fret: 1,
        frets: &[0, 2, 1, 1, 0, 0],
    },
    StaticVoicing {
        name: "Amaj7",
        base_fret: 1,
        frets: &[-1, 0, 2, 1, 2, 0],
    },
    StaticVoicing {
        name: "Gmaj7",
        base_fret: 1,
        frets: &[3, 2, 0, 0, 0, 2],
    },
    StaticVoicing {
        name: "Cmaj7",
        base_fret: 1,
        frets: &[-1, 3, 2, 0, 0, 0],
    },
    StaticVoicing {
        name: "Dmaj7",
        base_fret: 1,
        frets: &[-1, -1, 0, 2, 2, 2],
    },
    // Emaj7-shape barre voicings
    StaticVoicing {
        name: "Fmaj7",
        base_fret: 1,
        frets: &[1, 3, 2, 2, 1, 1],
    },
    StaticVoicing {
        name: "F#maj7",
        base_fret: 2,
        frets: &[1, 3, 2, 2, 1, 1],
    },
    StaticVoicing {
        name: "G#maj7",
        base_fret: 4,
        frets: &[1, 3, 2, 2, 1, 1],
    },
    // Amaj7-shape barre voicings
    StaticVoicing {
        name: "A#maj7",
        base_fret: 1,
        frets: &[-1, 1, 3, 2, 2, 1],
    },
    StaticVoicing {
        name: "Bmaj7",
        base_fret: 2,
        frets: &[-1, 1, 3, 2, 2, 1],
    },
    StaticVoicing {
        name: "C#maj7",
        base_fret: 4,
        frets: &[-1, 1, 3, 2, 2, 1],
    },
    StaticVoicing {
        name: "D#maj7",
        base_fret: 6,
        frets: &[-1, 1, 3, 2, 2, 1],
    },
];

/// Guitar minor-7th chord voicings.
///
/// Em7-shape barre pattern (relative): [1,3,1,1,1,1]
/// Am7-shape barre pattern (relative): [-1,1,3,1,2,1]
const GUITAR_MIN7: &[StaticVoicing] = &[
    // Open voicings
    StaticVoicing {
        name: "Em7",
        base_fret: 1,
        frets: &[0, 2, 0, 0, 0, 0],
    },
    StaticVoicing {
        name: "Am7",
        base_fret: 1,
        frets: &[-1, 0, 2, 0, 1, 0],
    },
    StaticVoicing {
        name: "Dm7",
        base_fret: 1,
        frets: &[-1, -1, 0, 2, 1, 1],
    },
    // Em7-shape barre voicings
    StaticVoicing {
        name: "Fm7",
        base_fret: 1,
        frets: &[1, 3, 1, 1, 1, 1],
    },
    StaticVoicing {
        name: "F#m7",
        base_fret: 2,
        frets: &[1, 3, 1, 1, 1, 1],
    },
    StaticVoicing {
        name: "Gm7",
        base_fret: 3,
        frets: &[1, 3, 1, 1, 1, 1],
    },
    StaticVoicing {
        name: "G#m7",
        base_fret: 4,
        frets: &[1, 3, 1, 1, 1, 1],
    },
    // Am7-shape barre voicings
    StaticVoicing {
        name: "A#m7",
        base_fret: 1,
        frets: &[-1, 1, 3, 1, 2, 1],
    },
    StaticVoicing {
        name: "Bm7",
        base_fret: 2,
        frets: &[-1, 1, 3, 1, 2, 1],
    },
    StaticVoicing {
        name: "Cm7",
        base_fret: 3,
        frets: &[-1, 1, 3, 1, 2, 1],
    },
    StaticVoicing {
        name: "C#m7",
        base_fret: 4,
        frets: &[-1, 1, 3, 1, 2, 1],
    },
    StaticVoicing {
        name: "D#m7",
        base_fret: 6,
        frets: &[-1, 1, 3, 1, 2, 1],
    },
];

// ---------------------------------------------------------------------------
// Ukulele voicings (4 strings, standard GCEA tuning, high-G)
//
// Fret order: [string4=G, string3=C, string2=E, string1=A]
// Open strings: G(3), C(4), E(0), A(0) in semitones above C3
// ---------------------------------------------------------------------------

/// Ukulele major chord voicings.
///
/// Semitone offsets from open: G=0, C=0, E=0, A=0 (string open pitches: G4, C4, E4, A4)
const UKULELE_MAJOR: &[StaticVoicing] = &[
    StaticVoicing {
        name: "A",
        base_fret: 1,
        frets: &[2, 1, 0, 0],
    },
    StaticVoicing {
        name: "A#",
        base_fret: 1,
        frets: &[3, 2, 1, 1],
    },
    StaticVoicing {
        name: "B",
        base_fret: 1,
        frets: &[4, 3, 2, 2],
    },
    StaticVoicing {
        name: "C",
        base_fret: 1,
        frets: &[0, 0, 0, 3],
    },
    StaticVoicing {
        name: "C#",
        base_fret: 1,
        frets: &[1, 1, 1, 4],
    },
    StaticVoicing {
        name: "D",
        base_fret: 1,
        frets: &[2, 2, 2, 0],
    },
    StaticVoicing {
        name: "D#",
        base_fret: 1,
        frets: &[3, 3, 3, 1],
    },
    StaticVoicing {
        name: "E",
        base_fret: 1,
        frets: &[4, 4, 4, 2],
    },
    StaticVoicing {
        name: "F",
        base_fret: 1,
        frets: &[2, 0, 1, 0],
    },
    StaticVoicing {
        name: "F#",
        base_fret: 1,
        frets: &[3, 1, 2, 1],
    },
    StaticVoicing {
        name: "G",
        base_fret: 1,
        frets: &[0, 2, 3, 2],
    },
    StaticVoicing {
        name: "G#",
        base_fret: 1,
        frets: &[1, 3, 4, 3],
    },
];

/// Ukulele minor chord voicings.
const UKULELE_MINOR: &[StaticVoicing] = &[
    StaticVoicing {
        name: "Am",
        base_fret: 1,
        frets: &[2, 0, 0, 0],
    },
    StaticVoicing {
        name: "A#m",
        base_fret: 1,
        frets: &[3, 1, 1, 1],
    },
    StaticVoicing {
        name: "Bm",
        base_fret: 1,
        frets: &[4, 2, 2, 2],
    },
    StaticVoicing {
        name: "Cm",
        base_fret: 1,
        frets: &[0, 3, 3, 3],
    },
    StaticVoicing {
        name: "C#m",
        base_fret: 1,
        frets: &[1, 1, 0, 4],
    },
    StaticVoicing {
        name: "Dm",
        base_fret: 1,
        frets: &[2, 2, 1, 0],
    },
    StaticVoicing {
        name: "D#m",
        base_fret: 1,
        frets: &[3, 3, 2, 1],
    },
    StaticVoicing {
        name: "Em",
        base_fret: 1,
        frets: &[0, 4, 3, 2],
    },
    StaticVoicing {
        name: "Fm",
        base_fret: 1,
        frets: &[1, 0, 1, 3],
    },
    StaticVoicing {
        name: "F#m",
        base_fret: 1,
        frets: &[2, 1, 2, 0],
    },
    StaticVoicing {
        name: "Gm",
        base_fret: 1,
        frets: &[0, 2, 3, 1],
    },
    StaticVoicing {
        name: "G#m",
        base_fret: 1,
        frets: &[1, 3, 4, 2],
    },
];

/// Ukulele dominant-7th chord voicings.
const UKULELE_DOM7: &[StaticVoicing] = &[
    StaticVoicing {
        name: "A7",
        base_fret: 1,
        frets: &[0, 1, 0, 0],
    },
    StaticVoicing {
        name: "A#7",
        base_fret: 1,
        frets: &[1, 2, 1, 1],
    },
    StaticVoicing {
        name: "B7",
        base_fret: 1,
        frets: &[2, 3, 2, 2],
    },
    StaticVoicing {
        name: "C7",
        base_fret: 1,
        frets: &[0, 0, 0, 1],
    },
    StaticVoicing {
        name: "C#7",
        base_fret: 1,
        frets: &[1, 1, 1, 2],
    },
    StaticVoicing {
        name: "D7",
        base_fret: 1,
        frets: &[2, 2, 2, 3],
    },
    StaticVoicing {
        name: "D#7",
        base_fret: 1,
        frets: &[3, 3, 3, 4],
    },
    StaticVoicing {
        name: "E7",
        base_fret: 1,
        frets: &[1, 2, 0, 2],
    },
    StaticVoicing {
        name: "F7",
        base_fret: 1,
        frets: &[2, 3, 1, 0],
    },
    StaticVoicing {
        name: "F#7",
        base_fret: 1,
        frets: &[3, 4, 2, 4],
    },
    StaticVoicing {
        name: "G7",
        base_fret: 1,
        frets: &[0, 2, 1, 2],
    },
    StaticVoicing {
        name: "G#7",
        base_fret: 1,
        frets: &[1, 3, 2, 3],
    },
];

// ---------------------------------------------------------------------------
// Enharmonic root normalisation
//
// Splits the chord name into a flat root + suffix, normalises the root to
// its sharp equivalent, and reassembles.  Only the root changes; the suffix
// is passed through unchanged, so this works for any chord family without
// manual expansion.
// ---------------------------------------------------------------------------

/// Converts a flat-spelled chord name to its sharp-spelled canonical form.
///
/// Splits `name` into a flat root (Bb, Db, Eb, Gb, Ab) and an arbitrary
/// suffix, normalises the root to its sharp equivalent, and returns the
/// reassembled string.  Returns `None` when the root is not a recognised
/// flat enharmonic (already in canonical form or unrecognised).
///
/// Because only the root is mapped, this handles any chord suffix — dom9,
/// sus4, add9, dim7, etc. — without requiring additions to this function.
pub(crate) fn flat_to_sharp(name: &str) -> Option<String> {
    const ROOTS: &[(&str, &str)] = &[
        ("Bb", "A#"),
        ("Db", "C#"),
        ("Eb", "D#"),
        ("Gb", "F#"),
        ("Ab", "G#"),
    ];
    for (flat, sharp) in ROOTS {
        if let Some(suffix) = name.strip_prefix(flat) {
            return Some(format!("{sharp}{suffix}"));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Public lookup API
// ---------------------------------------------------------------------------

/// Looks up a guitar voicing for `chord_name`.
///
/// Returns `None` if no voicing is available for the requested chord.
/// Accepts both sharp and flat spellings (e.g., `"Bb"` → same as `"A#"`).
#[must_use]
pub fn guitar_voicing(chord_name: &str) -> Option<DiagramData> {
    let canonical = flat_to_sharp(chord_name);
    let name = canonical.as_deref().unwrap_or(chord_name);
    let tables: &[&[StaticVoicing]] = &[
        GUITAR_MAJOR,
        GUITAR_MINOR,
        GUITAR_DOM7,
        GUITAR_MAJ7,
        GUITAR_MIN7,
    ];
    for table in tables {
        if let Some(v) = table.iter().find(|v| v.name == name) {
            return Some(v.to_diagram());
        }
    }
    None
}

/// Looks up a ukulele voicing for `chord_name`.
///
/// Returns `None` if no voicing is available for the requested chord.
/// Accepts both sharp and flat spellings.
#[must_use]
pub fn ukulele_voicing(chord_name: &str) -> Option<DiagramData> {
    let canonical = flat_to_sharp(chord_name);
    let name = canonical.as_deref().unwrap_or(chord_name);
    let tables: &[&[StaticVoicing]] = &[UKULELE_MAJOR, UKULELE_MINOR, UKULELE_DOM7];
    for table in tables {
        if let Some(v) = table.iter().find(|v| v.name == name) {
            return Some(v.to_diagram());
        }
    }
    None
}

/// Looks up a chord diagram by name using a prioritised lookup chain.
///
/// # Lookup order
///
/// 1. `defines` — fretted `{define}` entries from the song file (highest priority).
/// 2. Built-in voicing database — `guitar_voicing` or `ukulele_voicing` based on
///    the `instrument` parameter.
///
/// Returns `None` when no diagram is available (unknown chord / keyboard-only
/// definition).
///
/// # Parameters
///
/// - `chord_name` — chord name as it appears in the lyrics (e.g., `"Am"`, `"C#m7"`).
/// - `defines` — list of `(name, raw)` pairs from `{define}` directives.
///   Obtain via [`Song::fretted_defines`](crate::ast::Song::fretted_defines).
/// - `instrument` — `"guitar"` or `"ukulele"` (case-insensitive). Anything else
///   falls back to guitar. Returns `None` immediately for keyboard-family
///   instruments (`"piano"`, `"keyboard"`, `"keys"`); use
///   [`lookup_keyboard_voicing`] for those.
/// - `frets_shown` — number of fret rows to display in the diagram.
#[must_use]
pub fn lookup_diagram(
    chord_name: &str,
    defines: &[(String, String)],
    instrument: &str,
    frets_shown: usize,
) -> Option<crate::chord_diagram::DiagramData> {
    // Keyboard instruments have no fretted diagrams.
    if matches!(
        instrument.to_ascii_lowercase().as_str(),
        "piano" | "keyboard" | "keys"
    ) {
        return None;
    }

    // 1. Song-level {define} directives take priority.
    // Normalize both the lookup key and the define names to their canonical sharp
    // form so that e.g. a {define: A# …} entry is found when looking up "Bb",
    // and vice-versa.
    let canonical_chord_owned = flat_to_sharp(chord_name);
    let canonical_chord = canonical_chord_owned.as_deref().unwrap_or(chord_name);
    if let Some((_, raw)) = defines.iter().find(|(n, _)| {
        let canonical_n_owned = flat_to_sharp(n.as_str());
        let canonical_n = canonical_n_owned.as_deref().unwrap_or(n.as_str());
        canonical_n == canonical_chord
    }) {
        return crate::chord_diagram::DiagramData::from_raw_infer_frets(
            chord_name,
            raw,
            frets_shown,
        );
    }
    // 2. Built-in database.
    match instrument.to_ascii_lowercase().as_str() {
        "ukulele" | "uke" => ukulele_voicing(chord_name),
        _ => guitar_voicing(chord_name),
    }
}

// ---------------------------------------------------------------------------
// Piano / keyboard voicings
// ---------------------------------------------------------------------------

use crate::chord_diagram::KeyboardVoicing;

/// Internal static piano voicing record.
struct StaticKeyVoicing {
    /// Chord name (sharp spelling).
    name: &'static str,
    /// MIDI note numbers for the chord tones (absolute, octave 4 unless noted).
    keys: &'static [u8],
    /// MIDI note number of the root key.
    root_key: u8,
}

impl StaticKeyVoicing {
    fn to_voicing(&self, requested_name: &str) -> KeyboardVoicing {
        KeyboardVoicing {
            name: requested_name.to_string(),
            display_name: None,
            keys: self.keys.to_vec(),
            root_key: self.root_key,
        }
    }
}

// Piano major voicings – root position, octave 4 (C4 = MIDI 60).
// Intervals: root (R), major third (+4), perfect fifth (+7).
const PIANO_MAJOR: &[StaticKeyVoicing] = &[
    StaticKeyVoicing {
        name: "C",
        keys: &[60, 64, 67],
        root_key: 60,
    },
    StaticKeyVoicing {
        name: "C#",
        keys: &[61, 65, 68],
        root_key: 61,
    },
    StaticKeyVoicing {
        name: "D",
        keys: &[62, 66, 69],
        root_key: 62,
    },
    StaticKeyVoicing {
        name: "D#",
        keys: &[63, 67, 70],
        root_key: 63,
    },
    StaticKeyVoicing {
        name: "E",
        keys: &[64, 68, 71],
        root_key: 64,
    },
    StaticKeyVoicing {
        name: "F",
        keys: &[65, 69, 72],
        root_key: 65,
    },
    StaticKeyVoicing {
        name: "F#",
        keys: &[66, 70, 73],
        root_key: 66,
    },
    StaticKeyVoicing {
        name: "G",
        keys: &[67, 71, 74],
        root_key: 67,
    },
    StaticKeyVoicing {
        name: "G#",
        keys: &[68, 72, 75],
        root_key: 68,
    },
    StaticKeyVoicing {
        name: "A",
        keys: &[69, 73, 76],
        root_key: 69,
    },
    StaticKeyVoicing {
        name: "A#",
        keys: &[70, 74, 77],
        root_key: 70,
    },
    StaticKeyVoicing {
        name: "B",
        keys: &[71, 75, 78],
        root_key: 71,
    },
];

// Piano minor voicings.
// Intervals: root, minor third (+3), perfect fifth (+7).
const PIANO_MINOR: &[StaticKeyVoicing] = &[
    StaticKeyVoicing {
        name: "Cm",
        keys: &[60, 63, 67],
        root_key: 60,
    },
    StaticKeyVoicing {
        name: "C#m",
        keys: &[61, 64, 68],
        root_key: 61,
    },
    StaticKeyVoicing {
        name: "Dm",
        keys: &[62, 65, 69],
        root_key: 62,
    },
    StaticKeyVoicing {
        name: "D#m",
        keys: &[63, 66, 70],
        root_key: 63,
    },
    StaticKeyVoicing {
        name: "Em",
        keys: &[64, 67, 71],
        root_key: 64,
    },
    StaticKeyVoicing {
        name: "Fm",
        keys: &[65, 68, 72],
        root_key: 65,
    },
    StaticKeyVoicing {
        name: "F#m",
        keys: &[66, 69, 73],
        root_key: 66,
    },
    StaticKeyVoicing {
        name: "Gm",
        keys: &[67, 70, 74],
        root_key: 67,
    },
    StaticKeyVoicing {
        name: "G#m",
        keys: &[68, 71, 75],
        root_key: 68,
    },
    StaticKeyVoicing {
        name: "Am",
        keys: &[69, 72, 76],
        root_key: 69,
    },
    StaticKeyVoicing {
        name: "A#m",
        keys: &[70, 73, 77],
        root_key: 70,
    },
    StaticKeyVoicing {
        name: "Bm",
        keys: &[71, 74, 78],
        root_key: 71,
    },
];

// Piano dominant-seventh voicings.
// Intervals: root, major third (+4), perfect fifth (+7), minor seventh (+10).
const PIANO_DOM7: &[StaticKeyVoicing] = &[
    StaticKeyVoicing {
        name: "C7",
        keys: &[60, 64, 67, 70],
        root_key: 60,
    },
    StaticKeyVoicing {
        name: "C#7",
        keys: &[61, 65, 68, 71],
        root_key: 61,
    },
    StaticKeyVoicing {
        name: "D7",
        keys: &[62, 66, 69, 72],
        root_key: 62,
    },
    StaticKeyVoicing {
        name: "D#7",
        keys: &[63, 67, 70, 73],
        root_key: 63,
    },
    StaticKeyVoicing {
        name: "E7",
        keys: &[64, 68, 71, 74],
        root_key: 64,
    },
    StaticKeyVoicing {
        name: "F7",
        keys: &[65, 69, 72, 75],
        root_key: 65,
    },
    StaticKeyVoicing {
        name: "F#7",
        keys: &[66, 70, 73, 76],
        root_key: 66,
    },
    StaticKeyVoicing {
        name: "G7",
        keys: &[67, 71, 74, 77],
        root_key: 67,
    },
    StaticKeyVoicing {
        name: "G#7",
        keys: &[68, 72, 75, 78],
        root_key: 68,
    },
    StaticKeyVoicing {
        name: "A7",
        keys: &[69, 73, 76, 79],
        root_key: 69,
    },
    StaticKeyVoicing {
        name: "A#7",
        keys: &[70, 74, 77, 80],
        root_key: 70,
    },
    StaticKeyVoicing {
        name: "B7",
        keys: &[71, 75, 78, 81],
        root_key: 71,
    },
];

// Piano major-seventh voicings.
// Intervals: root, major third (+4), perfect fifth (+7), major seventh (+11).
const PIANO_MAJ7: &[StaticKeyVoicing] = &[
    StaticKeyVoicing {
        name: "Cmaj7",
        keys: &[60, 64, 67, 71],
        root_key: 60,
    },
    StaticKeyVoicing {
        name: "C#maj7",
        keys: &[61, 65, 68, 72],
        root_key: 61,
    },
    StaticKeyVoicing {
        name: "Dmaj7",
        keys: &[62, 66, 69, 73],
        root_key: 62,
    },
    StaticKeyVoicing {
        name: "D#maj7",
        keys: &[63, 67, 70, 74],
        root_key: 63,
    },
    StaticKeyVoicing {
        name: "Emaj7",
        keys: &[64, 68, 71, 75],
        root_key: 64,
    },
    StaticKeyVoicing {
        name: "Fmaj7",
        keys: &[65, 69, 72, 76],
        root_key: 65,
    },
    StaticKeyVoicing {
        name: "F#maj7",
        keys: &[66, 70, 73, 77],
        root_key: 66,
    },
    StaticKeyVoicing {
        name: "Gmaj7",
        keys: &[67, 71, 74, 78],
        root_key: 67,
    },
    StaticKeyVoicing {
        name: "G#maj7",
        keys: &[68, 72, 75, 79],
        root_key: 68,
    },
    StaticKeyVoicing {
        name: "Amaj7",
        keys: &[69, 73, 76, 80],
        root_key: 69,
    },
    StaticKeyVoicing {
        name: "A#maj7",
        keys: &[70, 74, 77, 81],
        root_key: 70,
    },
    StaticKeyVoicing {
        name: "Bmaj7",
        keys: &[71, 75, 78, 82],
        root_key: 71,
    },
];

// Piano minor-seventh voicings.
// Intervals: root, minor third (+3), perfect fifth (+7), minor seventh (+10).
const PIANO_MIN7: &[StaticKeyVoicing] = &[
    StaticKeyVoicing {
        name: "Cm7",
        keys: &[60, 63, 67, 70],
        root_key: 60,
    },
    StaticKeyVoicing {
        name: "C#m7",
        keys: &[61, 64, 68, 71],
        root_key: 61,
    },
    StaticKeyVoicing {
        name: "Dm7",
        keys: &[62, 65, 69, 72],
        root_key: 62,
    },
    StaticKeyVoicing {
        name: "D#m7",
        keys: &[63, 66, 70, 73],
        root_key: 63,
    },
    StaticKeyVoicing {
        name: "Em7",
        keys: &[64, 67, 71, 74],
        root_key: 64,
    },
    StaticKeyVoicing {
        name: "Fm7",
        keys: &[65, 68, 72, 75],
        root_key: 65,
    },
    StaticKeyVoicing {
        name: "F#m7",
        keys: &[66, 69, 73, 76],
        root_key: 66,
    },
    StaticKeyVoicing {
        name: "Gm7",
        keys: &[67, 70, 74, 77],
        root_key: 67,
    },
    StaticKeyVoicing {
        name: "G#m7",
        keys: &[68, 71, 75, 78],
        root_key: 68,
    },
    StaticKeyVoicing {
        name: "Am7",
        keys: &[69, 72, 76, 79],
        root_key: 69,
    },
    StaticKeyVoicing {
        name: "A#m7",
        keys: &[70, 73, 77, 80],
        root_key: 70,
    },
    StaticKeyVoicing {
        name: "Bm7",
        keys: &[71, 74, 78, 81],
        root_key: 71,
    },
];

/// Looks up a built-in piano/keyboard voicing for `chord_name`.
///
/// Returns `None` if no voicing is available for the requested chord.
/// Accepts both sharp and flat spellings (e.g., `"Bb"` → same as `"A#"`).
///
/// The built-in database covers major, minor, dominant-seventh, major-seventh,
/// and minor-seventh chord families for all twelve roots.
#[must_use]
pub fn keyboard_voicing(chord_name: &str) -> Option<KeyboardVoicing> {
    let canonical = flat_to_sharp(chord_name);
    let name = canonical.as_deref().unwrap_or(chord_name);
    let tables: &[&[StaticKeyVoicing]] =
        &[PIANO_MAJOR, PIANO_MINOR, PIANO_DOM7, PIANO_MAJ7, PIANO_MIN7];
    for table in tables {
        if let Some(v) = table.iter().find(|v| v.name == name) {
            return Some(v.to_voicing(chord_name));
        }
    }
    None
}

/// Looks up a keyboard voicing by chord name using a prioritised chain.
///
/// # Lookup order
///
/// 1. `keyboard_defines` — `{define: Name keys n1 n2 ...}` entries from the
///    song file (highest priority).
/// 2. Built-in piano voicing database ([`keyboard_voicing`]).
///
/// Returns `None` when no voicing is found.
///
/// # Parameters
///
/// - `chord_name` — chord name as it appears in the lyrics (e.g., `"Am"`, `"Cmaj7"`).
/// - `keyboard_defines` — list of `(name, keys)` pairs from keyboard `{define}`
///   directives. Obtain via [`Song::keyboard_defines`](crate::ast::Song::keyboard_defines).
///
/// # Root key convention
///
/// When a voicing is constructed from a song-level `{define}` entry, the
/// **first key in the `keys` list** is treated as the root key. Song authors
/// should list the root note first (e.g., `{define: Am keys 57 60 64}` where
/// 57 = A3 is the root). Built-in voicings always have the root first.
#[must_use]
pub fn lookup_keyboard_voicing(
    chord_name: &str,
    keyboard_defines: &[(String, Vec<i32>)],
) -> Option<KeyboardVoicing> {
    // 1. Song-level {define: name keys ...} directives take priority.
    let canonical_owned = flat_to_sharp(chord_name);
    let canonical = canonical_owned.as_deref().unwrap_or(chord_name);
    if let Some((_, keys)) = keyboard_defines.iter().find(|(n, _)| {
        let cn_owned = flat_to_sharp(n.as_str());
        let cn = cn_owned.as_deref().unwrap_or(n.as_str());
        cn == canonical
    }) {
        let keys_u8: Vec<u8> = keys
            .iter()
            .filter_map(|&k| {
                if (0i32..=127).contains(&k) {
                    Some(k as u8)
                } else {
                    None
                }
            })
            .collect();
        if !keys_u8.is_empty() {
            let root = keys_u8[0];
            return Some(KeyboardVoicing {
                name: chord_name.to_string(),
                display_name: None,
                keys: keys_u8,
                root_key: root,
            });
        }
    }
    // 2. Built-in database.
    keyboard_voicing(chord_name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- guitar lookups ---

    #[test]
    fn guitar_major_open_positions() {
        // Spot-check canonical open-position major chords.
        let e = guitar_voicing("E").unwrap();
        assert_eq!(e.frets, vec![0, 2, 2, 1, 0, 0]);
        assert_eq!(e.base_fret, 1);
        assert_eq!(e.strings, 6);

        let a = guitar_voicing("A").unwrap();
        assert_eq!(a.frets, vec![-1, 0, 2, 2, 2, 0]);

        let g = guitar_voicing("G").unwrap();
        assert_eq!(g.frets, vec![3, 2, 0, 0, 0, 3]);

        let c = guitar_voicing("C").unwrap();
        assert_eq!(c.frets, vec![-1, 3, 2, 0, 1, 0]);

        let d = guitar_voicing("D").unwrap();
        assert_eq!(d.frets, vec![-1, -1, 0, 2, 3, 2]);
    }

    #[test]
    fn guitar_barre_major() {
        // F uses E-shape barre at fret 1.
        let f = guitar_voicing("F").unwrap();
        assert_eq!(f.frets, vec![1, 3, 3, 2, 1, 1]);
        assert_eq!(f.base_fret, 1);

        // B uses A-shape barre at fret 2.
        let b = guitar_voicing("B").unwrap();
        assert_eq!(b.frets, vec![-1, 1, 3, 3, 3, 1]);
        assert_eq!(b.base_fret, 2);
    }

    #[test]
    fn guitar_minor_open_positions() {
        let em = guitar_voicing("Em").unwrap();
        assert_eq!(em.frets, vec![0, 2, 2, 0, 0, 0]);

        let am = guitar_voicing("Am").unwrap();
        assert_eq!(am.frets, vec![-1, 0, 2, 2, 1, 0]);

        let dm = guitar_voicing("Dm").unwrap();
        assert_eq!(dm.frets, vec![-1, -1, 0, 2, 3, 1]);
    }

    #[test]
    fn guitar_dom7_open_positions() {
        let e7 = guitar_voicing("E7").unwrap();
        assert_eq!(e7.frets, vec![0, 2, 0, 1, 0, 0]);

        let g7 = guitar_voicing("G7").unwrap();
        assert_eq!(g7.frets, vec![3, 2, 0, 0, 0, 1]);

        let d7 = guitar_voicing("D7").unwrap();
        assert_eq!(d7.frets, vec![-1, -1, 0, 2, 1, 2]);
    }

    #[test]
    fn guitar_maj7_and_min7() {
        let emaj7 = guitar_voicing("Emaj7").unwrap();
        assert_eq!(emaj7.frets, vec![0, 2, 1, 1, 0, 0]);

        let em7 = guitar_voicing("Em7").unwrap();
        assert_eq!(em7.frets, vec![0, 2, 0, 0, 0, 0]);

        let am7 = guitar_voicing("Am7").unwrap();
        assert_eq!(am7.frets, vec![-1, 0, 2, 0, 1, 0]);
    }

    #[test]
    fn guitar_flat_spelling_aliases() {
        // Bb should resolve to the same voicing as A#.
        let bb = guitar_voicing("Bb").unwrap();
        let as_ = guitar_voicing("A#").unwrap();
        assert_eq!(bb.frets, as_.frets);
        assert_eq!(bb.base_fret, as_.base_fret);

        assert_eq!(
            guitar_voicing("Db").unwrap().frets,
            guitar_voicing("C#").unwrap().frets
        );
        assert_eq!(
            guitar_voicing("Eb").unwrap().frets,
            guitar_voicing("D#").unwrap().frets
        );
        assert_eq!(
            guitar_voicing("Gb").unwrap().frets,
            guitar_voicing("F#").unwrap().frets
        );
        assert_eq!(
            guitar_voicing("Ab").unwrap().frets,
            guitar_voicing("G#").unwrap().frets
        );

        // Minor flat aliases
        assert_eq!(
            guitar_voicing("Bbm").unwrap().frets,
            guitar_voicing("A#m").unwrap().frets
        );
        assert_eq!(
            guitar_voicing("Ebm").unwrap().frets,
            guitar_voicing("D#m").unwrap().frets
        );

        // 7th flat aliases
        assert_eq!(
            guitar_voicing("Bb7").unwrap().frets,
            guitar_voicing("A#7").unwrap().frets
        );
    }

    #[test]
    fn flat_to_sharp_extended_chord_types() {
        // Root-based normalisation handles any suffix without manual expansion.
        assert_eq!(flat_to_sharp("Bb9").as_deref(), Some("A#9"));
        assert_eq!(flat_to_sharp("Bbsus4").as_deref(), Some("A#sus4"));
        assert_eq!(flat_to_sharp("Bbadd9").as_deref(), Some("A#add9"));
        assert_eq!(flat_to_sharp("Bbdim").as_deref(), Some("A#dim"));
        assert_eq!(flat_to_sharp("Bbdim7").as_deref(), Some("A#dim7"));
        assert_eq!(flat_to_sharp("Bbaug").as_deref(), Some("A#aug"));
        assert_eq!(flat_to_sharp("Bb6").as_deref(), Some("A#6"));
        assert_eq!(flat_to_sharp("Bbm6").as_deref(), Some("A#m6"));
        assert_eq!(flat_to_sharp("Bb11").as_deref(), Some("A#11"));
        assert_eq!(flat_to_sharp("Bb13").as_deref(), Some("A#13"));

        // All five flat roots
        assert_eq!(flat_to_sharp("Dbsus2").as_deref(), Some("C#sus2"));
        assert_eq!(flat_to_sharp("Ebadd9").as_deref(), Some("D#add9"));
        assert_eq!(flat_to_sharp("Gbdim7").as_deref(), Some("F#dim7"));
        assert_eq!(flat_to_sharp("Absus2").as_deref(), Some("G#sus2"));

        // Sharp spellings must not be altered.
        assert_eq!(flat_to_sharp("A#9").as_deref(), None);
        assert_eq!(flat_to_sharp("C#sus4").as_deref(), None);

        // Unknown root returns None.
        assert_eq!(flat_to_sharp("Xyzzy").as_deref(), None);
        assert_eq!(flat_to_sharp("").as_deref(), None);
    }

    #[test]
    fn lookup_diagram_define_extended_chord_flat_sharp() {
        // {define: Bb9 …} in the song, [A#9] in the lyrics → define must be found.
        let defines = vec![(
            "Bb9".to_string(),
            "base-fret 1 frets 1 2 3 4 5 1".to_string(),
        )];
        let d = lookup_diagram("A#9", &defines, "guitar", 5).unwrap();
        assert_eq!(d.frets, vec![1, 2, 3, 4, 5, 1]);
    }

    #[test]
    fn lookup_diagram_define_extended_chord_sharp_flat() {
        // {define: A#sus4 …} in the song, [Bbsus4] in the lyrics → define must be found.
        let defines = vec![(
            "A#sus4".to_string(),
            "base-fret 1 frets 1 2 3 4 5 1".to_string(),
        )];
        let d = lookup_diagram("Bbsus4", &defines, "guitar", 5).unwrap();
        assert_eq!(d.frets, vec![1, 2, 3, 4, 5, 1]);
    }

    #[test]
    fn guitar_unknown_chord_returns_none() {
        assert!(guitar_voicing("Xmaj13").is_none());
        assert!(guitar_voicing("").is_none());
    }

    // --- ukulele lookups ---

    #[test]
    fn ukulele_major_basic() {
        let c = ukulele_voicing("C").unwrap();
        assert_eq!(c.frets, vec![0, 0, 0, 3]);
        assert_eq!(c.strings, 4);

        let f = ukulele_voicing("F").unwrap();
        assert_eq!(f.frets, vec![2, 0, 1, 0]);

        let g = ukulele_voicing("G").unwrap();
        assert_eq!(g.frets, vec![0, 2, 3, 2]);
    }

    #[test]
    fn ukulele_minor_and_dom7() {
        let am = ukulele_voicing("Am").unwrap();
        assert_eq!(am.frets, vec![2, 0, 0, 0]);

        let g7 = ukulele_voicing("G7").unwrap();
        assert_eq!(g7.frets, vec![0, 2, 1, 2]);

        let c7 = ukulele_voicing("C7").unwrap();
        assert_eq!(c7.frets, vec![0, 0, 0, 1]);
    }

    #[test]
    fn ukulele_flat_aliases() {
        let bb = ukulele_voicing("Bb").unwrap();
        let as_ = ukulele_voicing("A#").unwrap();
        assert_eq!(bb.frets, as_.frets);
    }

    #[test]
    fn ukulele_unknown_chord_returns_none() {
        assert!(ukulele_voicing("Xsus13").is_none());
    }

    #[test]
    fn all_guitar_voicings_have_six_strings() {
        let tables: &[&[StaticVoicing]] = &[
            GUITAR_MAJOR,
            GUITAR_MINOR,
            GUITAR_DOM7,
            GUITAR_MAJ7,
            GUITAR_MIN7,
        ];
        for table in tables {
            for v in *table {
                assert_eq!(
                    v.frets.len(),
                    6,
                    "voicing '{}' has {} frets, expected 6",
                    v.name,
                    v.frets.len()
                );
            }
        }
    }

    #[test]
    fn all_ukulele_voicings_have_four_strings() {
        let tables: &[&[StaticVoicing]] = &[UKULELE_MAJOR, UKULELE_MINOR, UKULELE_DOM7];
        for table in tables {
            for v in *table {
                assert_eq!(
                    v.frets.len(),
                    4,
                    "voicing '{}' has {} frets, expected 4",
                    v.name,
                    v.frets.len()
                );
            }
        }
    }

    // --- lookup_diagram ---

    #[test]
    fn lookup_diagram_builtin_guitar() {
        let d = lookup_diagram("Am", &[], "guitar", 5).unwrap();
        assert_eq!(d.name, "Am");
        assert_eq!(d.strings, 6);
    }

    #[test]
    fn lookup_diagram_builtin_ukulele() {
        let d = lookup_diagram("Am", &[], "ukulele", 5).unwrap();
        assert_eq!(d.name, "Am");
        assert_eq!(d.strings, 4);
    }

    #[test]
    fn lookup_diagram_unknown_returns_none() {
        assert!(lookup_diagram("Xyzzy", &[], "guitar", 5).is_none());
    }

    #[test]
    fn lookup_diagram_define_overrides_builtin() {
        // Override Am with a different voicing (3 strings, frets 1 2 3).
        let defines = vec![("Am".to_string(), "base-fret 1 frets 1 2 3".to_string())];
        let d = lookup_diagram("Am", &defines, "guitar", 5).unwrap();
        assert_eq!(d.frets, vec![1, 2, 3]);
        assert_eq!(d.strings, 3); // inferred from fret count
    }

    #[test]
    fn lookup_diagram_flat_alias_resolved() {
        // Bb is stored internally as A# in the guitar table; lookup_diagram
        // accepts flat input and returns a diagram (name comes from the DB entry).
        let d = lookup_diagram("Bb", &[], "guitar", 5).unwrap();
        // The DB stores "A#" internally; the returned name reflects that.
        assert_eq!(d.name, "A#");
    }

    #[test]
    fn lookup_diagram_define_flat_sharp_alias() {
        // User writes {define: A# …} but lyrics use [Bb]; the define must be found.
        // Use frets 1 2 3 4 5 1 (all ≤ frets_shown=5 to avoid clamping).
        let defines = vec![(
            "A#".to_string(),
            "base-fret 1 frets 1 2 3 4 5 1".to_string(),
        )];
        let d = lookup_diagram("Bb", &defines, "guitar", 5).unwrap();
        assert_eq!(d.frets, vec![1, 2, 3, 4, 5, 1]);
    }

    #[test]
    fn lookup_diagram_define_sharp_flat_alias() {
        // User writes {define: Bb …} but lyrics use [A#]; the define must be found.
        let defines = vec![(
            "Bb".to_string(),
            "base-fret 1 frets 1 2 3 4 5 1".to_string(),
        )];
        let d = lookup_diagram("A#", &defines, "guitar", 5).unwrap();
        assert_eq!(d.frets, vec![1, 2, 3, 4, 5, 1]);
    }

    // --- piano / keyboard voicing lookups ---

    #[test]
    fn piano_major_c_voicing() {
        let v = keyboard_voicing("C").unwrap();
        assert_eq!(v.keys, vec![60, 64, 67]);
        assert_eq!(v.root_key, 60);
        assert_eq!(v.name, "C");
    }

    #[test]
    fn piano_minor_am_voicing() {
        let v = keyboard_voicing("Am").unwrap();
        assert_eq!(v.keys, vec![69, 72, 76]);
        assert_eq!(v.root_key, 69);
    }

    #[test]
    fn piano_maj7_cmaj7_voicing() {
        let v = keyboard_voicing("Cmaj7").unwrap();
        assert_eq!(v.keys, vec![60, 64, 67, 71]);
        assert_eq!(v.root_key, 60);
    }

    #[test]
    fn piano_flat_alias_bb() {
        // Bb is stored as A#; lookup_keyboard_voicing with flat name should work.
        let v = keyboard_voicing("Bb").unwrap();
        // name preserved as requested
        assert_eq!(v.name, "Bb");
        // keys are for A# major (same as Bb major)
        assert_eq!(v.keys, vec![70, 74, 77]);
    }

    #[test]
    fn piano_unknown_chord_returns_none() {
        assert!(keyboard_voicing("Xyzzy").is_none());
    }

    #[test]
    fn lookup_keyboard_voicing_define_overrides_builtin() {
        let defines = vec![("Am".to_string(), vec![57i32, 60, 64])];
        let v = lookup_keyboard_voicing("Am", &defines).unwrap();
        assert_eq!(v.keys, vec![57, 60, 64]);
        assert_eq!(v.root_key, 57);
    }

    #[test]
    fn lookup_keyboard_voicing_falls_back_to_builtin() {
        let v = lookup_keyboard_voicing("G7", &[]).unwrap();
        assert_eq!(v.keys, vec![67, 71, 74, 77]);
    }

    #[test]
    fn lookup_keyboard_voicing_flat_sharp_alias_in_define() {
        // Define uses A# but lookup uses Bb
        let defines = vec![("A#".to_string(), vec![58i32, 62, 65])];
        let v = lookup_keyboard_voicing("Bb", &defines).unwrap();
        assert_eq!(v.keys, vec![58, 62, 65]);
    }

    // --- lookup_diagram piano rejection ---

    #[test]
    fn lookup_diagram_rejects_piano_instrument() {
        // Passing "piano" to lookup_diagram must return None; it has no fretted
        // diagram database. Callers should use lookup_keyboard_voicing instead.
        assert!(
            lookup_diagram("Am", &[], "piano", 5).is_none(),
            "piano should return None from lookup_diagram"
        );
        assert!(
            lookup_diagram("Am", &[], "keyboard", 5).is_none(),
            "keyboard should return None from lookup_diagram"
        );
        assert!(
            lookup_diagram("Am", &[], "keys", 5).is_none(),
            "keys should return None from lookup_diagram"
        );
    }
}
