//! Heuristic plain-text chord+lyrics importer.
//!
//! This module detects and converts plain-text chord sheets — where chord names
//! appear on their own lines above the corresponding lyric lines — into the
//! ChordPro [`Song`] AST.
//!
//! # Format
//!
//! Plain-text chord sheets look like:
//!
//! ```text
//! [Verse]
//! Am        F         C         G
//! There's a lady who's sure all that glitters is gold
//! ```
//!
//! Each "chord line" contains only chord names (whitespace-separated), and the
//! following "lyric line" contains the sung text. The column position of each
//! chord in the chord line is preserved as an inline annotation over the
//! corresponding text in the lyric line.
//!
//! # Detection
//!
//! Use [`detect_format`] to auto-classify an input string, or
//! [`PlainTextImporter::detect_format`] to use custom thresholds.
//!
//! # Conversion
//!
//! Use [`convert_plain_text`] to convert a plain-text chord sheet into a
//! [`Song`], or [`PlainTextImporter::convert`] to use custom thresholds.

use crate::ast::{Chord, Directive, Line, LyricsLine, LyricsSegment, Song};

// ---------------------------------------------------------------------------
// InputFormat
// ---------------------------------------------------------------------------

/// Classification of an input text format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputFormat {
    /// The input is ChordPro format (directives or inline chord notation).
    ChordPro,
    /// The input is a plain chord+lyrics sheet.
    PlainChordLyrics,
    /// The format could not be determined.
    Unknown,
}

// ---------------------------------------------------------------------------
// PlainTextImporter
// ---------------------------------------------------------------------------

/// Configuration for the plain-text heuristic importer.
///
/// # Examples
///
/// ```
/// use chordsketch_core::heuristic::{PlainTextImporter, InputFormat};
///
/// let importer = PlainTextImporter::new();
/// let format = importer.detect_format("Am  G  C\nHello world here\n");
/// assert_eq!(format, InputFormat::PlainChordLyrics);
/// ```
#[derive(Debug, Clone)]
pub struct PlainTextImporter {
    /// Minimum fraction of whitespace-separated tokens that must be valid chord
    /// names for a line to be classified as a chord line. Default: `0.5`.
    pub chord_threshold: f64,
    /// Minimum number of chord tokens required to classify a line as a chord
    /// line. Default: `2`.
    pub min_chord_tokens: usize,
}

impl Default for PlainTextImporter {
    fn default() -> Self {
        Self {
            chord_threshold: 0.5,
            min_chord_tokens: 2,
        }
    }
}

impl PlainTextImporter {
    /// Creates a new importer with default threshold settings.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if `line` appears to be a chord line.
    ///
    /// A chord line satisfies all of the following conditions:
    /// - Contains at least [`min_chord_tokens`][Self::min_chord_tokens] tokens
    ///   that parse as valid chord names.
    /// - The fraction of valid chord tokens is ≥
    ///   [`chord_threshold`][Self::chord_threshold].
    /// - Does not contain sentence-ending punctuation (`.`, `?`, `!`), which
    ///   is a strong indicator that the line is lyrics.
    fn is_chord_line(&self, line: &str) -> bool {
        // Sentence-ending punctuation strongly indicates lyrics.
        if line.contains('.') || line.contains('?') || line.contains('!') {
            return false;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            return false;
        }
        let chord_count = tokens.iter().filter(|t| is_chord_token(t)).count();
        chord_count >= self.min_chord_tokens
            && chord_count as f64 / tokens.len() as f64 >= self.chord_threshold
    }

    /// Detects the input format using heuristics.
    ///
    /// Returns [`InputFormat::ChordPro`] if the input appears to be ChordPro
    /// (directive braces or inline `[chord]` notation).
    /// Returns [`InputFormat::PlainChordLyrics`] if the input contains at least
    /// two chord lines.
    /// Returns [`InputFormat::Unknown`] otherwise.
    #[must_use]
    pub fn detect_format(&self, input: &str) -> InputFormat {
        let lines: Vec<&str> = input.lines().collect();

        // ChordPro directive syntax: a line whose first non-space character is
        // `{` and last non-space character is `}`.
        let has_directives = lines.iter().any(|l| {
            let t = l.trim();
            t.starts_with('{') && t.ends_with('}')
        });
        if has_directives {
            return InputFormat::ChordPro;
        }

        // ChordPro inline chord notation: `[Am]`, `[G7]`, etc.
        // Distinguish from plain-text section labels like `[Verse]` or `[Chorus 2]`
        // by checking whether the content inside `[...]` is a valid chord name.
        let has_inline_chords = lines.iter().any(|l| {
            let mut rest: &str = l;
            while let Some(open) = rest.find('[') {
                let after = &rest[open + 1..];
                let Some(close) = after.find(']') else { break };
                let content = &after[..close];
                if is_chord_token(content) {
                    return true;
                }
                rest = &after[close + 1..];
            }
            false
        });
        if has_inline_chords {
            return InputFormat::ChordPro;
        }

        // Plain chord+lyrics: at least two chord lines.
        let chord_line_count = lines.iter().filter(|l| self.is_chord_line(l)).count();
        if chord_line_count >= 2 {
            InputFormat::PlainChordLyrics
        } else if chord_line_count == 1 && lines.len() <= 5 {
            // Very short input with one chord line is still treated as plain
            // chord+lyrics (e.g., a two-line snippet passed for testing).
            InputFormat::PlainChordLyrics
        } else {
            InputFormat::Unknown
        }
    }

    /// Converts a plain chord+lyrics text into a [`Song`] AST.
    ///
    /// # Algorithm
    ///
    /// 1. Classify each line as a chord line, section header, lyric, or blank.
    /// 2. Pair each chord line with the immediately following lyric line.
    ///    For each such pair, compute chord column offsets and produce inline
    ///    chord annotations.
    /// 3. Section headers are converted to `{start_of_*}` / `{end_of_*}`
    ///    directive pairs.
    /// 4. Lines that are neither chord lines nor section headers are emitted as
    ///    plain lyric lines.
    #[must_use]
    pub fn convert(&self, input: &str) -> Song {
        let raw_lines: Vec<&str> = input.lines().collect();
        let classes: Vec<LineKind<'_>> = raw_lines
            .iter()
            .map(|l| classify_line(l, |line| self.is_chord_line(line)))
            .collect();

        let mut song = Song::new();
        let mut i = 0;
        let mut current_section: Option<String> = None;

        while i < classes.len() {
            match &classes[i] {
                LineKind::Blank => {
                    song.lines.push(Line::Empty);
                    i += 1;
                }
                LineKind::SectionHeader(label) => {
                    let label = label.clone();
                    // Close any open section.
                    if let Some(ref sec) = current_section {
                        song.lines
                            .push(Line::Directive(end_directive_for_section(sec)));
                    }
                    // Open the new section.
                    let (start_dir, canonical) = start_directive_for_section(&label);
                    song.lines.push(Line::Directive(start_dir));
                    current_section = Some(canonical);
                    i += 1;
                }
                LineKind::ChordLine(positions) => {
                    // Peek at the next non-blank line: if it is a lyric, pair them.
                    let j = i + 1;
                    if j < classes.len() {
                        if let LineKind::Lyric(lyric) = &classes[j] {
                            let paired = pair_chords_with_lyric(positions, lyric);
                            song.lines.push(Line::Lyrics(paired));
                            i += 2;
                            continue;
                        }
                    }
                    // No following lyric — emit the chords as a chord-only line.
                    let paired = pair_chords_with_lyric(positions, "");
                    song.lines.push(Line::Lyrics(paired));
                    i += 1;
                }
                LineKind::Lyric(text) => {
                    song.lines.push(Line::Lyrics(LyricsLine {
                        segments: vec![LyricsSegment {
                            chord: None,
                            text: (*text).to_string(),
                            spans: vec![],
                        }],
                    }));
                    i += 1;
                }
            }
        }

        // Close the last open section.
        if let Some(ref sec) = current_section {
            song.lines
                .push(Line::Directive(end_directive_for_section(sec)));
        }

        song
    }
}

// ---------------------------------------------------------------------------
// Module-level convenience functions
// ---------------------------------------------------------------------------

/// Detects the format of `input` using default [`PlainTextImporter`] settings.
///
/// # Examples
///
/// ```
/// use chordsketch_core::heuristic::{detect_format, InputFormat};
///
/// assert_eq!(
///     detect_format("{title: My Song}\n[Am]Hello"),
///     InputFormat::ChordPro
/// );
/// assert_eq!(
///     detect_format("Am  G  C\nHello world\n"),
///     InputFormat::PlainChordLyrics
/// );
/// ```
#[must_use]
pub fn detect_format(input: &str) -> InputFormat {
    PlainTextImporter::default().detect_format(input)
}

/// Converts a plain chord+lyrics text into a [`Song`] AST using default
/// [`PlainTextImporter`] settings.
///
/// # Examples
///
/// ```
/// use chordsketch_core::heuristic::convert_plain_text;
///
/// let song = convert_plain_text("Am  G\nHello world\n");
/// assert!(!song.lines.is_empty());
/// ```
#[must_use]
pub fn convert_plain_text(input: &str) -> Song {
    PlainTextImporter::default().convert(input)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Classification of a single input line.
#[derive(Debug)]
enum LineKind<'a> {
    /// A blank (empty or all-whitespace) line.
    Blank,
    /// A section header like `[Verse]` or `CHORUS:`.
    /// Contains the inner label text.
    SectionHeader(String),
    /// A chord line. Contains `(byte_offset, chord_name)` pairs, sorted by
    /// byte offset.
    ChordLine(Vec<(usize, String)>),
    /// A lyric (or unrecognised) line.
    Lyric(&'a str),
}

/// Classifies a single line using the supplied chord-line predicate.
fn classify_line<'a, F>(line: &'a str, is_chord_line: F) -> LineKind<'a>
where
    F: Fn(&str) -> bool,
{
    if line.trim().is_empty() {
        return LineKind::Blank;
    }
    if let Some(label) = parse_section_header(line) {
        return LineKind::SectionHeader(label);
    }
    if is_chord_line(line) {
        return LineKind::ChordLine(chord_positions(line));
    }
    LineKind::Lyric(line)
}

/// Returns `true` if `token` is a well-formed chord name.
///
/// This is a stricter check than [`parse_chord`]: it rejects tokens whose
/// extension part contains unexpected alphabetic characters (e.g., `"Chorus"`
/// would be parsed as `C + "horus"` by `parse_chord`, but is rejected here).
///
/// Accepted patterns:
/// - Root `A–G` with optional `#` / `b`
/// - Zero or more quality+extension atoms: a quality keyword (`m`, `maj`,
///   `min`, `dim`, `aug`, `sus`, `add`, `+`, `°`) optionally followed by a
///   numeric extension (e.g. `m7add11`, `maj7sus4`, `7b5`)
/// - Optional bass note: `/A–G[#b]`
fn is_chord_token(token: &str) -> bool {
    // Chord names are short; reject anything suspiciously long.
    if token.is_empty() || token.len() > 12 {
        return false;
    }
    let bytes = token.as_bytes();

    // Root note: must be A–G (uppercase).
    if !matches!(bytes[0], b'A'..=b'G') {
        return false;
    }

    // Split off optional bass note (/X[#b]).
    let (body, bass) = match token.find('/') {
        Some(i) => (&token[..i], Some(&token[i + 1..])),
        None => (token, None),
    };

    // Validate bass note.
    if let Some(bass) = bass {
        if bass.is_empty() {
            return false;
        }
        let b = bass.as_bytes();
        if !matches!(b[0], b'A'..=b'G') {
            return false;
        }
        if b.len() > 1 && b[1] != b'#' && b[1] != b'b' {
            return false;
        }
        if b.len() > 2 {
            return false;
        }
    }

    // Validate body: root [accidental] [quality] [extension]
    let body_bytes = body.as_bytes();
    let mut pos = 1usize; // skip root letter

    // Optional accidental.
    if pos < body_bytes.len() && (body_bytes[pos] == b'#' || body_bytes[pos] == b'b') {
        pos += 1;
    }

    let quality_ext = &body[pos..];
    is_valid_quality_ext(quality_ext)
}

/// Consumes an optional accidental (`#` or `b`, only when immediately
/// followed by a digit) and then any run of ASCII digits.  Returns the
/// unconsumed suffix.
fn consume_numeric(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut i = 0;
    // Accidental only counts when a digit follows it.
    if bytes.len() >= 2 && (bytes[0] == b'#' || bytes[0] == b'b') && bytes[1].is_ascii_digit() {
        i = 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    &s[i..]
}

/// Returns `true` if `s` is a valid chord quality+extension suffix.
///
/// The suffix is consumed iteratively: each step strips one quality keyword
/// (`maj`, `min`, `dim`, `aug`, `sus`, `add`, `m`, `+`, `°`) optionally
/// followed by a numeric extension (optional `#`/`b` accidental then digits).
/// This allows compound forms such as `m7add11`, `maj7sus4`, or `7b5`.
///
/// Acceptable atoms (zero or more, in any order):
/// - Quality keyword: `maj`, `min`, `dim`, `aug`, `sus`, `add`, `m`, `+`, `°`
/// - Numeric extension: optional accidental (`#`/`b`, only if a digit follows)
///   then one or more digits
fn is_valid_quality_ext(s: &str) -> bool {
    let mut rest = s;
    loop {
        if rest.is_empty() {
            return true;
        }

        // Try to strip a quality keyword.
        let after_kw: Option<&str> = None
            .or_else(|| rest.strip_prefix("maj"))
            .or_else(|| rest.strip_prefix("min"))
            .or_else(|| rest.strip_prefix("dim"))
            .or_else(|| rest.strip_prefix("aug"))
            .or_else(|| rest.strip_prefix("sus"))
            .or_else(|| rest.strip_prefix("add"))
            .or_else(|| rest.strip_prefix('m'))
            .or_else(|| rest.strip_prefix('+'))
            .or_else(|| rest.strip_prefix('°'));

        if let Some(after) = after_kw {
            // Keyword consumed; optionally consume a following numeric part.
            rest = consume_numeric(after);
        } else {
            // No keyword — try a bare numeric extension.
            let next = consume_numeric(rest);
            if next.len() == rest.len() {
                // Nothing consumed: unrecognized character.
                return false;
            }
            rest = next;
        }
    }
}

/// Extracts `(byte_offset, chord_name)` pairs from a chord line, preserving
/// the column position of each chord token.
fn chord_positions(line: &str) -> Vec<(usize, String)> {
    let mut result = Vec::new();
    let mut search_start = 0usize;

    for token in line.split_whitespace() {
        // Locate this token inside the remaining slice.
        if let Some(rel) = line[search_start..].find(token) {
            let abs = search_start + rel;
            if is_chord_token(token) {
                result.push((abs, token.to_string()));
            }
            // Advance past this token.
            search_start = abs + token.len();
        }
    }
    result
}

/// Attempts to parse `line` as a section header.
///
/// Recognised patterns:
/// - `[Verse]`, `[Chorus 2]` — square brackets
/// - `(Bridge)` — parentheses
/// - `VERSE:`, `CHORUS:` — uppercase label followed by a colon
/// - `-- Chorus --`, `== Verse ==` — dash or equals decoration
///
/// Returns the inner label on success, or `None` if the line is not a section
/// header.
fn parse_section_header(line: &str) -> Option<String> {
    let trimmed = line.trim();

    /// Returns `true` if `label` is safe to embed in a ChordPro directive
    /// value.  Rejects labels containing `{` or `}` because ChordPro has no
    /// escape mechanism inside directive values and those characters would
    /// produce malformed output.
    fn is_safe_label(label: &str) -> bool {
        !label.is_empty() && !label.contains('{') && !label.contains('}')
    }

    // [Label] form
    if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() >= 3 {
        let inner = trimmed[1..trimmed.len() - 1].trim();
        if !inner.contains('[') && is_safe_label(inner) {
            return Some(inner.to_string());
        }
    }

    // (Label) form
    if trimmed.starts_with('(') && trimmed.ends_with(')') && trimmed.len() >= 3 {
        let inner = trimmed[1..trimmed.len() - 1].trim();
        if !inner.contains('(') && is_safe_label(inner) {
            return Some(inner.to_string());
        }
    }

    // LABEL: form — alphabetic label followed by exactly one colon
    if let Some(label) = trimmed.strip_suffix(':') {
        if label
            .chars()
            .all(|c| c.is_alphabetic() || c == ' ' || c == '-')
            && is_safe_label(label.trim())
        {
            return Some(label.trim().to_string());
        }
    }

    // -- Label -- and == Label == forms
    for delim in &["--", "==", "**", "##"] {
        if trimmed.starts_with(delim) && trimmed.ends_with(delim) && trimmed.len() > 2 * delim.len()
        {
            let inner = trimmed[delim.len()..trimmed.len() - delim.len()].trim();
            if is_safe_label(inner) {
                return Some(inner.to_string());
            }
        }
    }

    None
}

/// Maps a section label to its canonical ChordPro directive base name.
///
/// For example, `"verse"` → `"verse"`, `"chorus 2"` → `"chorus"`.
/// Unknown labels fall back to `"verse"` with a label attribute.
fn canonical_section(label: &str) -> &'static str {
    let lower = label.to_lowercase();
    let lower = lower.trim();
    if lower.starts_with("chorus") || lower.starts_with("refrain") {
        "chorus"
    } else if lower.starts_with("bridge") {
        "bridge"
    } else {
        // verse, intro, outro, pre-chorus, and unknown labels all map to "verse".
        "verse"
    }
}

/// Returns the `{start_of_*}` directive for the given label, plus the
/// canonical section name used to match the closing directive.
fn start_directive_for_section(label: &str) -> (Directive, String) {
    let canonical = canonical_section(label);
    let dir_name = format!("start_of_{canonical}");
    // Include the label as a `label` attribute when the label text differs from
    // the canonical section name (e.g., "Verse 2", "Intro").
    let lower_label = label.trim().to_lowercase();
    let dir = if lower_label == canonical {
        Directive::name_only(dir_name)
    } else {
        Directive::with_value(dir_name, label.trim().to_string())
    };
    (dir, canonical.to_string())
}

/// Returns the `{end_of_*}` directive for the given canonical section name.
fn end_directive_for_section(canonical: &str) -> Directive {
    let dir_name = format!("end_of_{canonical}");
    Directive::name_only(dir_name)
}

/// Builds a [`LyricsLine`] by pairing chord column positions with the
/// corresponding lyric text.
///
/// Each chord annotates the lyric text starting at its column position in the
/// chord line. Text that precedes the first chord is emitted in a leading
/// chord-free segment.
fn pair_chords_with_lyric(positions: &[(usize, String)], lyric: &str) -> LyricsLine {
    // Work in terms of char indices for correctness with multi-byte text, but
    // chord positions from `chord_positions` are byte offsets into the ASCII
    // chord line. Because chord lines are expected to be ASCII (chord names),
    // byte == char for the chord line. We must, however, map those byte offsets
    // to char offsets in the lyric line for correct slicing.
    let lyric_char_offsets: Vec<usize> = lyric.char_indices().map(|(b, _)| b).collect();
    let lyric_len = lyric.len();

    // Maps a byte offset from the (ASCII) chord line to a valid byte offset in
    // the lyric string, clamped to lyric_len.  When col is beyond the end of
    // the lyric we return lyric_len (no text to annotate).  For shorter cols
    // we snap to the nearest char boundary so we never slice in the middle of
    // a multi-byte codepoint.
    let clamp_to_lyric = |col: usize| -> usize {
        if col >= lyric_len {
            return lyric_len;
        }
        // Snap down to the nearest char-boundary offset that does not exceed col.
        lyric_char_offsets
            .iter()
            .copied()
            .rfind(|&b| b <= col)
            .unwrap_or(0)
    };

    let mut segments: Vec<LyricsSegment> = Vec::new();
    let mut cursor = 0usize; // byte position in lyric

    for (i, (col, chord_name)) in positions.iter().enumerate() {
        let text_start = clamp_to_lyric(*col);
        let text_end = if let Some((next_col, _)) = positions.get(i + 1) {
            clamp_to_lyric(*next_col)
        } else {
            lyric_len
        };

        // Any lyric text before this chord position (after the cursor) without
        // a chord annotation.
        if text_start > cursor {
            segments.push(LyricsSegment {
                chord: None,
                text: lyric[cursor..text_start].to_string(),
                spans: vec![],
            });
        }

        // text_start is always <= lyric_len by construction of clamp_to_lyric.
        let text = lyric[text_start..text_end.min(lyric_len)].to_string();

        segments.push(LyricsSegment {
            chord: Some(Chord::new(chord_name.as_str())),
            text,
            spans: vec![],
        });
        cursor = text_end.min(lyric_len);
    }

    // Remaining lyric text after all chord positions.
    if cursor < lyric_len {
        if let Some(last) = segments.last_mut() {
            last.text.push_str(&lyric[cursor..]);
        } else {
            segments.push(LyricsSegment {
                chord: None,
                text: lyric[cursor..].to_string(),
                spans: vec![],
            });
        }
    }

    if segments.is_empty() {
        segments.push(LyricsSegment {
            chord: None,
            text: lyric.to_string(),
            spans: vec![],
        });
    }

    LyricsLine { segments }
}

// ---------------------------------------------------------------------------
// ChordPro serializer for plain-text-imported songs
// ---------------------------------------------------------------------------

/// Serializes a [`Song`] to ChordPro format.
///
/// This serializer is intended for songs produced by [`convert_plain_text`].
/// It handles the subset of [`Line`] and [`Directive`] variants that the
/// heuristic importer emits. Complex AST features (image directives, delegate
/// environments, etc.) are rendered as a best-effort comment.
///
/// # Examples
///
/// ```
/// use chordsketch_core::heuristic::{convert_plain_text, song_to_chordpro};
///
/// let song = convert_plain_text("[Verse]\nAm  G\nHello world\n");
/// let chordpro = song_to_chordpro(&song);
/// assert!(chordpro.contains("{start_of_verse}"));
/// assert!(chordpro.contains("[Am]"));
/// ```
#[must_use]
pub fn song_to_chordpro(song: &Song) -> String {
    use crate::ast::{CommentStyle, Line};

    let mut out = String::new();

    // Emit metadata directives first if populated.
    if let Some(ref title) = song.metadata.title {
        out.push_str(&format!("{{title: {title}}}\n"));
    }
    if let Some(ref artist) = song.metadata.artists.first() {
        out.push_str(&format!("{{artist: {artist}}}\n"));
    }

    for line in &song.lines {
        match line {
            Line::Empty => out.push('\n'),
            Line::Comment(style, text) => match style {
                CommentStyle::Normal => out.push_str(&format!("{{comment: {text}}}\n")),
                CommentStyle::Italic => out.push_str(&format!("{{comment_italic: {text}}}\n")),
                CommentStyle::Boxed => out.push_str(&format!("{{comment_box: {text}}}\n")),
            },
            Line::Directive(dir) => {
                if let Some(ref value) = dir.value {
                    out.push_str(&format!("{{{}: {value}}}\n", dir.name));
                } else {
                    out.push_str(&format!("{{{}}}\n", dir.name));
                }
            }
            Line::Lyrics(lyrics) => {
                for seg in &lyrics.segments {
                    if let Some(ref chord) = seg.chord {
                        out.push('[');
                        out.push_str(&chord.name);
                        out.push(']');
                    }
                    out.push_str(&seg.text);
                }
                out.push('\n');
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn chord_names(line: &LyricsLine) -> Vec<Option<String>> {
        line.segments
            .iter()
            .map(|s| s.chord.as_ref().map(|c| c.name.clone()))
            .collect()
    }

    // --- detect_format ---

    #[test]
    fn detects_chordpro_from_directives() {
        assert_eq!(
            detect_format("{title: Hello}\n{soc}\n[Am]Hello\n{eoc}"),
            InputFormat::ChordPro
        );
    }

    #[test]
    fn detects_chordpro_from_inline_chords() {
        assert_eq!(detect_format("[Am]Hello [G]world"), InputFormat::ChordPro);
    }

    #[test]
    fn detects_plain_chord_lyrics() {
        let input = "Am  G  C  Em\nHello beautiful world tonight\nG  C\nOnce more";
        assert_eq!(detect_format(input), InputFormat::PlainChordLyrics);
    }

    #[test]
    fn detects_plain_chord_lyrics_with_section_labels() {
        // Section labels like [Verse] and [Chorus] should NOT be misidentified
        // as ChordPro inline chord notation.
        let input = "[Verse]\nG  D\nHere I am\nEm  C\nWondering\n\n[Chorus]\nC  G\nLala\n";
        assert_eq!(detect_format(input), InputFormat::PlainChordLyrics);
    }

    #[test]
    fn detects_unknown_for_pure_lyrics() {
        let input = "Hello beautiful world\nOnce upon a time\nSomething happened here";
        assert_eq!(detect_format(input), InputFormat::Unknown);
    }

    #[test]
    fn detects_unknown_for_empty() {
        assert_eq!(detect_format(""), InputFormat::Unknown);
    }

    // --- is_chord_token ---

    #[test]
    fn chord_token_rejects_section_labels() {
        // Section labels that start with A-G should NOT be recognized as chords.
        assert!(!is_chord_token("Chorus"));
        assert!(!is_chord_token("Bridge"));
        assert!(!is_chord_token("Em7add9sus2extended"));
    }

    #[test]
    fn chord_token_accepts_valid_chords() {
        assert!(is_chord_token("Am"));
        assert!(is_chord_token("C"));
        assert!(is_chord_token("G7"));
        assert!(is_chord_token("Cmaj7"));
        assert!(is_chord_token("D/F#"));
        assert!(is_chord_token("Bb"));
        assert!(is_chord_token("F#m7"));
        assert!(is_chord_token("Gsus4"));
        assert!(is_chord_token("Em"));
    }

    #[test]
    fn chord_token_accepts_multicomponent_extensions() {
        // Compound quality+extension sequences (issue #1279).
        assert!(is_chord_token("Am7add11"));
        assert!(is_chord_token("Cmaj7sus4"));
        assert!(is_chord_token("G7b5"));
        assert!(is_chord_token("Fmaj7add9"));
        assert!(is_chord_token("Dm7add11"));
        assert!(is_chord_token("G7#9"));
        assert!(is_chord_token("Cmaj9"));
        // Words that happen to start with A-G must still be rejected.
        assert!(!is_chord_token("Chorus"));
        assert!(!is_chord_token("Bridge"));
        assert!(!is_chord_token("Cmaj7extended")); // 'e' after digits is not a keyword
    }

    // --- is_chord_line ---

    #[test]
    fn chord_line_typical() {
        let imp = PlainTextImporter::new();
        assert!(imp.is_chord_line("Am  F  C  G"));
    }

    #[test]
    fn chord_line_with_slash_chords() {
        let imp = PlainTextImporter::new();
        assert!(imp.is_chord_line("G  D/F#  Em  C"));
    }

    #[test]
    fn not_chord_line_all_lyrics() {
        let imp = PlainTextImporter::new();
        assert!(!imp.is_chord_line("There's a lady who's sure."));
    }

    #[test]
    fn not_chord_line_sentence_punctuation() {
        let imp = PlainTextImporter::new();
        // Even though "A" and "G" are valid chords, the period disqualifies it.
        assert!(!imp.is_chord_line("A song for G."));
    }

    #[test]
    fn not_chord_line_too_few_chords() {
        let imp = PlainTextImporter::new();
        // Only one chord token — below min_chord_tokens=2.
        assert!(!imp.is_chord_line("Am something else here now"));
    }

    // --- parse_section_header ---

    #[test]
    fn section_square_brackets() {
        assert_eq!(parse_section_header("[Verse]"), Some("Verse".to_string()));
        assert_eq!(
            parse_section_header("[Chorus 2]"),
            Some("Chorus 2".to_string())
        );
    }

    #[test]
    fn section_parens() {
        assert_eq!(parse_section_header("(Bridge)"), Some("Bridge".to_string()));
    }

    #[test]
    fn section_colon() {
        assert_eq!(parse_section_header("VERSE:"), Some("VERSE".to_string()));
        assert_eq!(parse_section_header("Chorus:"), Some("Chorus".to_string()));
    }

    #[test]
    fn section_dash_decorated() {
        assert_eq!(
            parse_section_header("-- Chorus --"),
            Some("Chorus".to_string())
        );
    }

    #[test]
    fn section_not_matched() {
        assert_eq!(parse_section_header("Hello world"), None);
        assert_eq!(parse_section_header("Am G C Em"), None);
    }

    #[test]
    fn section_rejects_brace_in_label() {
        // Labels containing { or } must be rejected to prevent emitting
        // malformed ChordPro directive values (M-1 from delta review).
        assert_eq!(parse_section_header("[Verse}]"), None);
        assert_eq!(parse_section_header("[{Chorus}]"), None);
        assert_eq!(parse_section_header("Verse}:"), None);
    }

    // --- pair_chords_with_lyric ---

    #[test]
    fn pair_basic() {
        // "Am  F" at cols 0 and 4 over "Hello world"
        let positions = vec![(0, "Am".to_string()), (4, "F".to_string())];
        let line = pair_chords_with_lyric(&positions, "Hello world");
        let chords = chord_names(&line);
        assert_eq!(chords, vec![Some("Am".to_string()), Some("F".to_string())]);
        assert_eq!(line.segments[0].text, "Hell");
        assert_eq!(line.segments[1].text, "o world");
    }

    #[test]
    fn pair_lyric_shorter_than_chord_line() {
        // Chord at column 10 but lyric is only 5 chars.
        let positions = vec![(0, "Am".to_string()), (10, "G".to_string())];
        let line = pair_chords_with_lyric(&positions, "Hi");
        // Am gets "Hi", G gets ""
        assert_eq!(
            line.segments[0].chord.as_ref().map(|c| c.name.as_str()),
            Some("Am")
        );
        assert_eq!(line.segments[0].text, "Hi");
        assert_eq!(
            line.segments[1].chord.as_ref().map(|c| c.name.as_str()),
            Some("G")
        );
        assert_eq!(line.segments[1].text, "");
    }

    #[test]
    fn pair_no_chords_returns_plain_lyric() {
        let positions: Vec<(usize, String)> = vec![];
        let line = pair_chords_with_lyric(&positions, "Hello world");
        assert_eq!(line.segments.len(), 1);
        assert!(line.segments[0].chord.is_none());
        assert_eq!(line.segments[0].text, "Hello world");
    }

    // --- convert ---

    #[test]
    fn convert_simple_verse() {
        let input = "[Verse]\nAm  G  C\nHello world today\n";
        let song = convert_plain_text(input);
        // Should have: start_of_verse, lyrics, end_of_verse
        assert!(song.lines.iter().any(|l| matches!(l, Line::Directive(_))));
        assert!(song.lines.iter().any(|l| matches!(l, Line::Lyrics(_))));
    }

    #[test]
    fn convert_chordless_lyric_passthrough() {
        let input = "Am  G  C  Em\nThere is a song\nThis line has no preceding chord line\n";
        let song = convert_plain_text(input);
        // The first line pair produces a lyrics line with chords.
        // "This line has no preceding chord line" is a lyric after the pair.
        let has_plain_lyric = song.lines.iter().any(|l| {
            if let Line::Lyrics(ll) = l {
                ll.segments.len() == 1
                    && ll.segments[0].chord.is_none()
                    && ll.segments[0].text.contains("no preceding")
            } else {
                false
            }
        });
        assert!(has_plain_lyric);
    }

    #[test]
    fn convert_section_labels_to_directives() {
        let input = "[Chorus]\nG  C  G  D\nLala lala lala\n[Verse]\nAm  F  C  G\nHello world\n";
        let song = convert_plain_text(input);
        // At least one StartOfChorus and one StartOfVerse directive.
        use crate::ast::DirectiveKind;
        let kinds: Vec<&DirectiveKind> = song
            .lines
            .iter()
            .filter_map(|l| {
                if let Line::Directive(d) = l {
                    Some(&d.kind)
                } else {
                    None
                }
            })
            .collect();
        assert!(kinds.iter().any(|k| **k == DirectiveKind::StartOfChorus));
        assert!(kinds.iter().any(|k| **k == DirectiveKind::StartOfVerse));
    }

    #[test]
    fn convert_multiple_sections_close_properly() {
        let input = "[Verse]\nAm  G\nHello world\n[Chorus]\nC  G\nYeah yeah\n";
        let song = convert_plain_text(input);
        use crate::ast::DirectiveKind;
        // Should have both end_of_verse and end_of_chorus.
        let kinds: Vec<&DirectiveKind> = song
            .lines
            .iter()
            .filter_map(|l| {
                if let Line::Directive(d) = l {
                    Some(&d.kind)
                } else {
                    None
                }
            })
            .collect();
        assert!(kinds.iter().any(|k| **k == DirectiveKind::EndOfVerse));
        assert!(kinds.iter().any(|k| **k == DirectiveKind::EndOfChorus));
    }
}
