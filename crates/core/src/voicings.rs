//! Built-in chord voicing database for guitar and ukulele.
//!
//! Provides ~200 pre-defined chord voicings stored as compile-time static
//! data — no external files, no runtime I/O. The lookup priority is:
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
    /// Number of strings for this instrument (6 = guitar, 4 = ukulele).
    strings: usize,
}

impl StaticVoicing {
    fn to_diagram(&self) -> DiagramData {
        DiagramData {
            name: self.name.to_string(),
            display_name: None,
            strings: self.strings,
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
        strings: 6,
    },
    StaticVoicing {
        name: "A",
        base_fret: 1,
        frets: &[-1, 0, 2, 2, 2, 0],
        strings: 6,
    },
    StaticVoicing {
        name: "G",
        base_fret: 1,
        frets: &[3, 2, 0, 0, 0, 3],
        strings: 6,
    },
    StaticVoicing {
        name: "C",
        base_fret: 1,
        frets: &[-1, 3, 2, 0, 1, 0],
        strings: 6,
    },
    StaticVoicing {
        name: "D",
        base_fret: 1,
        frets: &[-1, -1, 0, 2, 3, 2],
        strings: 6,
    },
    // E-shape barre voicings
    StaticVoicing {
        name: "F",
        base_fret: 1,
        frets: &[1, 3, 3, 2, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "F#",
        base_fret: 2,
        frets: &[1, 3, 3, 2, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "G#",
        base_fret: 4,
        frets: &[1, 3, 3, 2, 1, 1],
        strings: 6,
    },
    // A-shape barre voicings
    StaticVoicing {
        name: "A#",
        base_fret: 1,
        frets: &[-1, 1, 3, 3, 3, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "B",
        base_fret: 2,
        frets: &[-1, 1, 3, 3, 3, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "C#",
        base_fret: 4,
        frets: &[-1, 1, 3, 3, 3, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "D#",
        base_fret: 6,
        frets: &[-1, 1, 3, 3, 3, 1],
        strings: 6,
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
        strings: 6,
    },
    StaticVoicing {
        name: "Am",
        base_fret: 1,
        frets: &[-1, 0, 2, 2, 1, 0],
        strings: 6,
    },
    StaticVoicing {
        name: "Dm",
        base_fret: 1,
        frets: &[-1, -1, 0, 2, 3, 1],
        strings: 6,
    },
    // Em-shape barre voicings
    StaticVoicing {
        name: "Fm",
        base_fret: 1,
        frets: &[1, 3, 3, 1, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "F#m",
        base_fret: 2,
        frets: &[1, 3, 3, 1, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "Gm",
        base_fret: 3,
        frets: &[1, 3, 3, 1, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "G#m",
        base_fret: 4,
        frets: &[1, 3, 3, 1, 1, 1],
        strings: 6,
    },
    // Am-shape barre voicings
    StaticVoicing {
        name: "A#m",
        base_fret: 1,
        frets: &[-1, 1, 3, 3, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "Bm",
        base_fret: 2,
        frets: &[-1, 1, 3, 3, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "Cm",
        base_fret: 3,
        frets: &[-1, 1, 3, 3, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "C#m",
        base_fret: 4,
        frets: &[-1, 1, 3, 3, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "D#m",
        base_fret: 6,
        frets: &[-1, 1, 3, 3, 2, 1],
        strings: 6,
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
        strings: 6,
    },
    StaticVoicing {
        name: "A7",
        base_fret: 1,
        frets: &[-1, 0, 2, 0, 2, 0],
        strings: 6,
    },
    StaticVoicing {
        name: "G7",
        base_fret: 1,
        frets: &[3, 2, 0, 0, 0, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "C7",
        base_fret: 1,
        frets: &[-1, 3, 2, 3, 1, 0],
        strings: 6,
    },
    StaticVoicing {
        name: "D7",
        base_fret: 1,
        frets: &[-1, -1, 0, 2, 1, 2],
        strings: 6,
    },
    StaticVoicing {
        name: "B7",
        base_fret: 1,
        frets: &[-1, 2, 1, 2, 0, 2],
        strings: 6,
    },
    // E7-shape barre voicings
    StaticVoicing {
        name: "F7",
        base_fret: 1,
        frets: &[1, 3, 1, 2, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "F#7",
        base_fret: 2,
        frets: &[1, 3, 1, 2, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "G#7",
        base_fret: 4,
        frets: &[1, 3, 1, 2, 1, 1],
        strings: 6,
    },
    // A7-shape barre voicings
    StaticVoicing {
        name: "A#7",
        base_fret: 1,
        frets: &[-1, 1, 3, 1, 3, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "C#7",
        base_fret: 4,
        frets: &[-1, 1, 3, 1, 3, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "D#7",
        base_fret: 6,
        frets: &[-1, 1, 3, 1, 3, 1],
        strings: 6,
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
        strings: 6,
    },
    StaticVoicing {
        name: "Amaj7",
        base_fret: 1,
        frets: &[-1, 0, 2, 1, 2, 0],
        strings: 6,
    },
    StaticVoicing {
        name: "Gmaj7",
        base_fret: 1,
        frets: &[3, 2, 0, 0, 0, 2],
        strings: 6,
    },
    StaticVoicing {
        name: "Cmaj7",
        base_fret: 1,
        frets: &[-1, 3, 2, 0, 0, 0],
        strings: 6,
    },
    StaticVoicing {
        name: "Dmaj7",
        base_fret: 1,
        frets: &[-1, -1, 0, 2, 2, 2],
        strings: 6,
    },
    // Emaj7-shape barre voicings
    StaticVoicing {
        name: "Fmaj7",
        base_fret: 1,
        frets: &[1, 3, 2, 2, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "F#maj7",
        base_fret: 2,
        frets: &[1, 3, 2, 2, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "G#maj7",
        base_fret: 4,
        frets: &[1, 3, 2, 2, 1, 1],
        strings: 6,
    },
    // Amaj7-shape barre voicings
    StaticVoicing {
        name: "A#maj7",
        base_fret: 1,
        frets: &[-1, 1, 3, 2, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "Bmaj7",
        base_fret: 2,
        frets: &[-1, 1, 3, 2, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "C#maj7",
        base_fret: 4,
        frets: &[-1, 1, 3, 2, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "D#maj7",
        base_fret: 6,
        frets: &[-1, 1, 3, 2, 2, 1],
        strings: 6,
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
        strings: 6,
    },
    StaticVoicing {
        name: "Am7",
        base_fret: 1,
        frets: &[-1, 0, 2, 0, 1, 0],
        strings: 6,
    },
    StaticVoicing {
        name: "Dm7",
        base_fret: 1,
        frets: &[-1, -1, 0, 2, 1, 1],
        strings: 6,
    },
    // Em7-shape barre voicings
    StaticVoicing {
        name: "Fm7",
        base_fret: 1,
        frets: &[1, 3, 1, 1, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "F#m7",
        base_fret: 2,
        frets: &[1, 3, 1, 1, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "Gm7",
        base_fret: 3,
        frets: &[1, 3, 1, 1, 1, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "G#m7",
        base_fret: 4,
        frets: &[1, 3, 1, 1, 1, 1],
        strings: 6,
    },
    // Am7-shape barre voicings
    StaticVoicing {
        name: "A#m7",
        base_fret: 1,
        frets: &[-1, 1, 3, 1, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "Bm7",
        base_fret: 2,
        frets: &[-1, 1, 3, 1, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "Cm7",
        base_fret: 3,
        frets: &[-1, 1, 3, 1, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "C#m7",
        base_fret: 4,
        frets: &[-1, 1, 3, 1, 2, 1],
        strings: 6,
    },
    StaticVoicing {
        name: "D#m7",
        base_fret: 6,
        frets: &[-1, 1, 3, 1, 2, 1],
        strings: 6,
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
        strings: 4,
    },
    StaticVoicing {
        name: "A#",
        base_fret: 1,
        frets: &[3, 2, 1, 1],
        strings: 4,
    },
    StaticVoicing {
        name: "B",
        base_fret: 1,
        frets: &[4, 3, 2, 2],
        strings: 4,
    },
    StaticVoicing {
        name: "C",
        base_fret: 1,
        frets: &[0, 0, 0, 3],
        strings: 4,
    },
    StaticVoicing {
        name: "C#",
        base_fret: 1,
        frets: &[1, 1, 1, 4],
        strings: 4,
    },
    StaticVoicing {
        name: "D",
        base_fret: 1,
        frets: &[2, 2, 2, 0],
        strings: 4,
    },
    StaticVoicing {
        name: "D#",
        base_fret: 1,
        frets: &[3, 3, 3, 1],
        strings: 4,
    },
    StaticVoicing {
        name: "E",
        base_fret: 1,
        frets: &[4, 4, 4, 2],
        strings: 4,
    },
    StaticVoicing {
        name: "F",
        base_fret: 1,
        frets: &[2, 0, 1, 0],
        strings: 4,
    },
    StaticVoicing {
        name: "F#",
        base_fret: 1,
        frets: &[3, 1, 2, 1],
        strings: 4,
    },
    StaticVoicing {
        name: "G",
        base_fret: 1,
        frets: &[0, 2, 3, 2],
        strings: 4,
    },
    StaticVoicing {
        name: "G#",
        base_fret: 1,
        frets: &[1, 3, 4, 3],
        strings: 4,
    },
];

/// Ukulele minor chord voicings.
const UKULELE_MINOR: &[StaticVoicing] = &[
    StaticVoicing {
        name: "Am",
        base_fret: 1,
        frets: &[2, 0, 0, 0],
        strings: 4,
    },
    StaticVoicing {
        name: "A#m",
        base_fret: 1,
        frets: &[3, 1, 1, 1],
        strings: 4,
    },
    StaticVoicing {
        name: "Bm",
        base_fret: 1,
        frets: &[4, 2, 2, 2],
        strings: 4,
    },
    StaticVoicing {
        name: "Cm",
        base_fret: 1,
        frets: &[0, 3, 3, 3],
        strings: 4,
    },
    StaticVoicing {
        name: "C#m",
        base_fret: 1,
        frets: &[1, 1, 0, 4],
        strings: 4,
    },
    StaticVoicing {
        name: "Dm",
        base_fret: 1,
        frets: &[2, 2, 1, 0],
        strings: 4,
    },
    StaticVoicing {
        name: "D#m",
        base_fret: 1,
        frets: &[3, 3, 2, 1],
        strings: 4,
    },
    StaticVoicing {
        name: "Em",
        base_fret: 1,
        frets: &[0, 4, 3, 2],
        strings: 4,
    },
    StaticVoicing {
        name: "Fm",
        base_fret: 1,
        frets: &[1, 0, 1, 3],
        strings: 4,
    },
    StaticVoicing {
        name: "F#m",
        base_fret: 1,
        frets: &[2, 1, 2, 0],
        strings: 4,
    },
    StaticVoicing {
        name: "Gm",
        base_fret: 1,
        frets: &[0, 2, 3, 1],
        strings: 4,
    },
    StaticVoicing {
        name: "G#m",
        base_fret: 1,
        frets: &[1, 3, 4, 2],
        strings: 4,
    },
];

/// Ukulele dominant-7th chord voicings.
const UKULELE_DOM7: &[StaticVoicing] = &[
    StaticVoicing {
        name: "A7",
        base_fret: 1,
        frets: &[0, 1, 0, 0],
        strings: 4,
    },
    StaticVoicing {
        name: "A#7",
        base_fret: 1,
        frets: &[1, 2, 1, 1],
        strings: 4,
    },
    StaticVoicing {
        name: "B7",
        base_fret: 1,
        frets: &[2, 3, 2, 2],
        strings: 4,
    },
    StaticVoicing {
        name: "C7",
        base_fret: 1,
        frets: &[0, 0, 0, 1],
        strings: 4,
    },
    StaticVoicing {
        name: "C#7",
        base_fret: 1,
        frets: &[1, 1, 1, 2],
        strings: 4,
    },
    StaticVoicing {
        name: "D7",
        base_fret: 1,
        frets: &[2, 2, 2, 3],
        strings: 4,
    },
    StaticVoicing {
        name: "D#7",
        base_fret: 1,
        frets: &[3, 3, 3, 4],
        strings: 4,
    },
    StaticVoicing {
        name: "E7",
        base_fret: 1,
        frets: &[1, 2, 0, 2],
        strings: 4,
    },
    StaticVoicing {
        name: "F7",
        base_fret: 1,
        frets: &[2, 3, 1, 0],
        strings: 4,
    },
    StaticVoicing {
        name: "F#7",
        base_fret: 1,
        frets: &[3, 4, 2, 4],
        strings: 4,
    },
    StaticVoicing {
        name: "G7",
        base_fret: 1,
        frets: &[0, 2, 1, 2],
        strings: 4,
    },
    StaticVoicing {
        name: "G#7",
        base_fret: 1,
        frets: &[1, 3, 2, 3],
        strings: 4,
    },
];

// ---------------------------------------------------------------------------
// Enharmonic alias table
//
// Maps flat-spelled root+suffix → sharp-spelled equivalent stored above.
// Only the root changes; the suffix is passed through unchanged.
// ---------------------------------------------------------------------------

/// Converts a flat-spelled chord name to its sharp-spelled canonical form.
///
/// Returns `None` if no alias applies (the name is already in canonical form
/// or is unrecognised).
fn flat_to_sharp(name: &str) -> Option<&'static str> {
    // Map of complete chord name: flat → sharp equivalent.
    // Covers the most common flat-root spellings.
    const ALIASES: &[(&str, &str)] = &[
        // Major
        ("Bb", "A#"),
        ("Db", "C#"),
        ("Eb", "D#"),
        ("Gb", "F#"),
        ("Ab", "G#"),
        // Minor
        ("Bbm", "A#m"),
        ("Dbm", "C#m"),
        ("Ebm", "D#m"),
        ("Gbm", "F#m"),
        ("Abm", "G#m"),
        // Dominant 7th
        ("Bb7", "A#7"),
        ("Db7", "C#7"),
        ("Eb7", "D#7"),
        ("Gb7", "F#7"),
        ("Ab7", "G#7"),
        // Major 7th
        ("Bbmaj7", "A#maj7"),
        ("Dbmaj7", "C#maj7"),
        ("Ebmaj7", "D#maj7"),
        ("Gbmaj7", "F#maj7"),
        ("Abmaj7", "G#maj7"),
        // Minor 7th
        ("Bbm7", "A#m7"),
        ("Dbm7", "C#m7"),
        ("Ebm7", "D#m7"),
        ("Gbm7", "F#m7"),
        ("Abm7", "G#m7"),
        // Ukulele minor (same roots, but listed for completeness — covered above)
    ];
    ALIASES
        .iter()
        .find(|(flat, _)| *flat == name)
        .map(|(_, sharp)| *sharp)
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
    let name = flat_to_sharp(chord_name).unwrap_or(chord_name);
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
    let name = flat_to_sharp(chord_name).unwrap_or(chord_name);
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
///   falls back to guitar.
/// - `frets_shown` — number of fret rows to display in the diagram.
#[must_use]
pub fn lookup_diagram(
    chord_name: &str,
    defines: &[(String, String)],
    instrument: &str,
    frets_shown: usize,
) -> Option<crate::chord_diagram::DiagramData> {
    // 1. Song-level {define} directives take priority.
    // Normalize both the lookup key and the define names to their canonical sharp
    // form so that e.g. a {define: A# …} entry is found when looking up "Bb",
    // and vice-versa.
    let canonical_chord = flat_to_sharp(chord_name).unwrap_or(chord_name);
    if let Some((_, raw)) = defines.iter().find(|(n, _)| {
        let canonical_n = flat_to_sharp(n.as_str()).unwrap_or(n.as_str());
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
                assert_eq!(v.strings, 6, "voicing '{}' strings mismatch", v.name);
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
                assert_eq!(v.strings, 4, "voicing '{}' strings mismatch", v.name);
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
}
