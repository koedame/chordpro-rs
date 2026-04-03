# Known Deviations from Perl ChordPro

This document lists intentional differences between chordpro-rs and the Perl
ChordPro reference implementation. These are not bugs — they represent design
decisions where chordpro-rs chose a different (or simpler) text rendering format.

The ChordPro specification defines the `.cho` file format but does **not**
prescribe a specific text output format. Both implementations produce valid
renderings; they differ in presentation style.

## Text Renderer Differences

### Title Display

| Perl | Rust |
|------|------|
| `-- Title: My Song` | `My Song` |

Perl prefixes the title with `-- Title:`. Rust renders it as a plain heading.

### Section Markers

| Perl | Rust |
|------|------|
| `-- Start of verse` | `[Verse]` |
| `-- End of verse` | *(not displayed)* |

Perl uses `-- Start of ...` / `-- End of ...` markers. Rust uses `[Section]`
headers at the start only, matching common lead-sheet convention.

### Comments

| Perl | Rust |
|------|------|
| `-- comment text` | `(comment text)` |
| *(italic)* `-- Play softly` | `(*Play softly*)` |
| *(boxed)* `-- Note` | `[Note]` |

Perl uses `--` prefix for all comment styles. Rust uses parentheses/brackets
with style indicators.

### Smart Quotes

Perl ChordPro performs typographic ("smart") quote conversion on output
(e.g., `'` → `'`, `"` → `"`). Rust preserves the original characters from
the `.cho` file without modification.

### Blank Line Spacing

Perl inserts extra blank lines between lyrics lines and around sections. Rust
uses single blank lines for section boundaries only, producing a more compact
output.

## Structural Equivalence

Despite the text formatting differences above, the following aspects are
verified to be equivalent:

- Chord names and positions above lyrics
- Transposition results
- Section structure (verse, chorus, bridge, tab, grid)
- Metadata parsing (title, artist, key, capo, tempo, etc.)
- Multi-song file splitting at `{new_song}` boundaries
- Unicode character handling
- Directive parsing and classification

## Perl Errors on Test Corpus

The following test corpus files cause Perl ChordPro to error (likely due to
markup features not supported by the Text output backend):

- `basic/02-title-only.cho` — empty song body
- `edge-cases/10-mixed-everything.cho` — complex markup combinations
- `formatting/01-bold.cho` through `formatting/09-markup-with-chords.cho` — inline markup in Text mode
