//! ABC notation → ChordPro converter.
//!
//! Converts ABC notation files to ChordPro format by extracting:
//! - Chord symbols (`"Am"`, `"Cmaj7"` etc.) → `[Am]` inline chord markers
//! - Lyrics (`w:` lines) aligned with the preceding music line
//! - Song metadata (`T:`, `C:`, `K:`, `Q:`) → `{title}`, `{composer}`,
//!   `{key}`, `{tempo}` directives
//! - Part labels (`P:` in the body) → `{start_of_verse: label}` sections
//!
//! # What is out of scope
//!
//! - Staff notation rendering (continues to rely on external `abc2svg`)
//! - Note pitch, rhythm, and duration data
//! - Multi-voice files (only the first voice is used)
//!
//! # ABC format overview (relevant subset)
//!
//! An ABC file consists of one or more *tunes*, each identified by an `X:`
//! (reference number) field.  A tune begins with a *header* (field lines
//! before the body) and a *body* (music lines and `w:` lyric lines).  The
//! `K:` field marks the end of the header.
//!
//! In the music body, double-quoted strings are chord annotations: `"Am"`.
//! Each `w:` line provides lyrics for the notes in the immediately preceding
//! music line.  Syllable–note alignment uses the following rules:
//!
//! - Space or `-` between syllables: new syllable maps to the next note.
//! - `_` (underscore): hold current syllable over the next note (no new text).
//! - `*` (asterisk): skip one note (insert whitespace).
//! - `|` in a `w:` line: bar synchronisation marker (ignored for alignment).
//!
//! # Examples
//!
//! ```
//! use chordsketch_core::abc_importer::convert_abc;
//!
//! let input = "X:1\nT:Test Song\nK:Am\n\"Am\" CDEF \"G\" GABC|\nw:Hel-lo world how are you\n";
//! let out = convert_abc(input);
//! assert!(out.contains("{title: Test Song}"));
//! assert!(out.contains("[Am]"));
//! ```

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Converts an ABC notation document to ChordPro format.
///
/// Handles single-tune and multi-tune (multiple `X:` header fields) ABC files.
/// Returns the full ChordPro text as a `String`.
///
/// Staff notation, note pitches, rhythms, and durations are discarded; only
/// chord symbols, lyrics, and metadata are extracted.
///
/// # Panics
///
/// Never panics; unknown fields and malformed lines are silently skipped.
#[must_use]
pub fn convert_abc(input: &str) -> String {
    let tunes = split_tunes(input);
    let mut out = String::new();
    for (i, tune) in tunes.iter().enumerate() {
        if i > 0 {
            out.push_str("{new_song}\n");
        }
        out.push_str(&convert_tune(tune));
    }
    out
}

// ---------------------------------------------------------------------------
// Tune splitting
// ---------------------------------------------------------------------------

/// Split a multi-tune ABC file into individual tune strings.
fn split_tunes(input: &str) -> Vec<String> {
    let mut tunes: Vec<String> = Vec::new();
    let mut current = String::new();

    for line in input.lines() {
        if line.starts_with("X:") && !current.trim().is_empty() {
            tunes.push(std::mem::take(&mut current));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        tunes.push(current);
    }
    if tunes.is_empty() && !input.trim().is_empty() {
        tunes.push(input.to_string());
    }
    tunes
}

// ---------------------------------------------------------------------------
// Tune conversion
// ---------------------------------------------------------------------------

/// Convert a single ABC tune to ChordPro text.
fn convert_tune(input: &str) -> String {
    let mut out = String::new();

    // Header metadata
    let mut title: Option<String> = None;
    let mut composer: Option<String> = None;
    let mut tempo: Option<String> = None;

    // Body state
    let mut in_header = true;
    let mut open_section = false;

    // Accumulate a "music block": one or more consecutive music lines before a
    // set of w: lyric lines.  When we see the first w: line (or an empty line
    // or a P: directive), we flush the accumulated block.
    let mut music_lines: Vec<String> = Vec::new();
    let mut pending_w_lines: Vec<String> = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();

        // Skip % comments
        if trimmed.starts_with('%') {
            continue;
        }

        // Empty line in body: flush current block
        if trimmed.is_empty() {
            if !in_header {
                flush_block(&music_lines, &pending_w_lines, &mut out);
                music_lines.clear();
                pending_w_lines.clear();
            }
            continue;
        }

        // Lyric lines (w: / W:) must be checked before the generic field-line
        // parser because `parse_field_line` would also match them (since 'w'
        // and 'W' are alphabetic).  Only collect them in the body.
        if !in_header && (trimmed.starts_with("w:") || trimmed.starts_with("W:")) {
            let lyrics = trimmed[2..].trim_start().to_string();
            pending_w_lines.push(lyrics);
            continue;
        }

        // Detect and dispatch header / mid-body field lines
        if let Some((field, value)) = parse_field_line(trimmed) {
            match field {
                'X' => {
                    // Reference number: marks (re)start of header
                    in_header = true;
                }
                'T' => title = Some(value.to_string()),
                'C' => composer = Some(value.to_string()),
                'Q' => tempo = extract_tempo_bpm(value),
                'K' => {
                    if in_header {
                        // K: is the last required header field; emit directives now.
                        emit_header_directives(
                            title.as_deref(),
                            composer.as_deref(),
                            tempo.as_deref(),
                            Some(value),
                            &mut out,
                        );
                        out.push('\n');
                        in_header = false;
                    }
                }
                'P' if !in_header => {
                    // Part label in body: close previous section, open new one.
                    flush_block(&music_lines, &pending_w_lines, &mut out);
                    music_lines.clear();
                    pending_w_lines.clear();
                    if open_section {
                        out.push_str("{end_of_verse}\n");
                    }
                    out.push_str(&format!(
                        "{{start_of_verse: {}}}\n",
                        escape_directive_value(value)
                    ));
                    open_section = true;
                }
                _ => {}
            }
            continue;
        }

        // Not a field line: body music line
        if in_header {
            // Content before the K: field — skip (malformed ABC or preamble)
            continue;
        }

        // Music line: if w: lines have already been collected for the previous
        // music block, flush that block now before starting a new one.
        if !pending_w_lines.is_empty() {
            flush_block(&music_lines, &pending_w_lines, &mut out);
            music_lines.clear();
            pending_w_lines.clear();
        }

        music_lines.push(trimmed.to_string());
    }

    // Flush any remaining accumulated content
    if !music_lines.is_empty() || !pending_w_lines.is_empty() {
        flush_block(&music_lines, &pending_w_lines, &mut out);
    }

    if open_section {
        out.push_str("{end_of_verse}\n");
    }

    out
}

// ---------------------------------------------------------------------------
// Header helpers
// ---------------------------------------------------------------------------

/// Parse an ABC field line `"F: value"` → `Some(('F', "value"))`.
///
/// Returns `None` if the line does not match the pattern.
fn parse_field_line(line: &str) -> Option<(char, &str)> {
    let mut chars = line.chars();
    let field = chars.next()?;
    if !field.is_ascii_alphabetic() {
        return None;
    }
    let rest = chars.as_str();
    if let Some(after_colon) = rest.strip_prefix(':') {
        Some((field, after_colon.trim()))
    } else {
        None
    }
}

/// Extract a numeric BPM value from a `Q:` field.
///
/// Handles bare numbers (`"120"`), note-length annotations (`"1/4=120"`),
/// and tempos with text labels (`"1/4=120 Allegro"`).
///
/// Returns `None` if no numeric BPM can be extracted (e.g. `"Q:Allegro"` or
/// `"Q:1/4=Allegro"`), so the caller can omit the `{tempo}` directive rather
/// than emitting a non-numeric value that would be invalid ChordPro.
fn extract_tempo_bpm(value: &str) -> Option<String> {
    let candidate = if let Some(pos) = value.find('=') {
        value[pos + 1..].split_whitespace().next().unwrap_or("")
    } else {
        value.split_whitespace().next().unwrap_or("")
    };
    if !candidate.is_empty() && candidate.bytes().all(|b| b.is_ascii_digit()) {
        Some(candidate.to_string())
    } else {
        None
    }
}

/// Emit ChordPro header directives from collected metadata.
fn emit_header_directives(
    title: Option<&str>,
    composer: Option<&str>,
    tempo: Option<&str>,
    key: Option<&str>,
    out: &mut String,
) {
    if let Some(t) = title {
        out.push_str(&format!("{{title: {}}}\n", escape_directive_value(t)));
    }
    if let Some(c) = composer {
        out.push_str(&format!("{{composer: {}}}\n", escape_directive_value(c)));
    }
    if let Some(t) = tempo {
        out.push_str(&format!("{{tempo: {}}}\n", escape_directive_value(t)));
    }
    if let Some(k) = key {
        out.push_str(&format!("{{key: {}}}\n", escape_directive_value(k)));
    }
}

/// Strip `{` and `}` from a directive value (sanitize for ChordPro output).
///
/// ChordPro has no escape mechanism for literal braces inside directive
/// values, so brace characters are removed.  Only the brace characters are
/// stripped; the rest of the value is preserved.
fn escape_directive_value(s: &str) -> String {
    let s = s.trim();
    if s.contains('{') || s.contains('}') {
        s.replace(['{', '}'], "")
    } else {
        s.to_string()
    }
}

/// Strip `{` and `}` from a lyric syllable before embedding it in ChordPro
/// output.
///
/// Unlike [`escape_directive_value`], this function does **not** trim
/// whitespace so that trailing hyphens (syllable connectors) are preserved.
/// ChordPro has no escape mechanism for literal braces, so they are removed.
fn sanitize_lyric_text(s: &str) -> String {
    if s.contains('{') || s.contains('}') {
        s.replace(['{', '}'], "")
    } else {
        s.to_string()
    }
}

/// Strip `[`, `]`, `{`, and `}` from a chord name before embedding it in
/// ChordPro `[chord]` output notation.
///
/// The `]` character would terminate the bracket group prematurely;
/// `{` and `}` could inject directive syntax into the output stream.
fn sanitize_chord_name(chord: &str) -> String {
    if chord.contains(['[', ']', '{', '}']) {
        chord
            .chars()
            .filter(|&c| !matches!(c, '[' | ']' | '{' | '}'))
            .collect()
    } else {
        chord.to_string()
    }
}

// ---------------------------------------------------------------------------
// Music block flushing
// ---------------------------------------------------------------------------

/// Flush a music block (one or more music lines + optional lyric lines) to
/// ChordPro output.
fn flush_block(music_lines: &[String], w_lines: &[String], out: &mut String) {
    if music_lines.is_empty() {
        // Orphaned lyric lines with no preceding music: output as plain text.
        for w in w_lines {
            let safe = sanitize_lyric_text(w);
            if !safe.is_empty() {
                out.push_str(&safe);
                out.push('\n');
            }
        }
        return;
    }

    // Combine all music lines into one token stream (multi-line music is treated
    // as one continuous sequence of notes).
    let combined_music: String = music_lines.join(" ");
    let (chord_at_note, note_count) = extract_chords_and_notes(&combined_music);

    if w_lines.is_empty() {
        // Chord-only block (no lyrics): emit a line of chord markers.
        let chord_line = build_chord_only_line(&chord_at_note, note_count);
        if !chord_line.trim().is_empty() {
            out.push_str(&chord_line);
            out.push('\n');
        }
        return;
    }

    // Emit all w: lines as separate ChordPro lyric lines.  Each line
    // represents one verse sung over the same melody.
    for w_line in w_lines {
        let syllables = parse_abc_lyrics(w_line);
        let line = build_chordpro_line(&chord_at_note, &syllables);
        if !line.trim().is_empty() {
            out.push_str(&line);
            out.push('\n');
        }
    }
}

// ---------------------------------------------------------------------------
// Music line tokenizer
// ---------------------------------------------------------------------------

/// Extract chord annotations and count notes from a combined music line.
///
/// Returns:
/// - A map from note index → chord name (the chord that appears immediately
///   before that note in the music notation).
/// - The total number of notes (including rests) encountered.
fn extract_chords_and_notes(line: &str) -> (HashMap<usize, String>, usize) {
    let mut chord_at: HashMap<usize, String> = HashMap::new();
    let mut note_idx: usize = 0;
    let mut pending_chord: Option<String> = None;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            // Comment: ignore rest of line
            '%' => break,

            // Chord annotation: "..."
            '"' => {
                i += 1; // skip opening "
                let start = i;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                let chord: String = chars[start..i].iter().collect();
                let chord = chord.trim().to_string();
                // Ignore empty chords and explicit "no chord" markers
                if !chord.is_empty()
                    && !matches!(chord.as_str(), "N.C." | "N.C" | "NC" | "n.c." | "n.c")
                {
                    pending_chord = Some(chord);
                }
                if i < chars.len() {
                    i += 1; // skip closing "
                }
            }

            // Rest: z (regular), Z (multi-measure), x (invisible)
            'z' | 'Z' | 'x' => {
                assign_chord(&mut chord_at, &mut pending_chord, note_idx);
                note_idx += 1;
                i += 1;
                skip_duration(&chars, &mut i);
            }

            // Accidental prefix (^, _, =) before a note letter
            '^' | '_' | '=' => {
                let mut j = i + 1;
                // Double accidental (^^ or __)
                if j < chars.len() && (chars[j] == '^' || chars[j] == '_') {
                    j += 1;
                }
                if j < chars.len() && is_note_letter(chars[j]) {
                    assign_chord(&mut chord_at, &mut pending_chord, note_idx);
                    note_idx += 1;
                    i = j + 1;
                    skip_note_suffix(&chars, &mut i);
                } else {
                    i += 1;
                }
            }

            // Note letter A-G (upper or lower case)
            c if is_note_letter(c) => {
                assign_chord(&mut chord_at, &mut pending_chord, note_idx);
                note_idx += 1;
                i += 1;
                skip_note_suffix(&chars, &mut i);
            }

            // Chord group [CEG] — simultaneous notes count as one syllable.
            // Also handles repeat markers like [1 or [|
            '[' => {
                i += 1;
                if i < chars.len()
                    && (chars[i].is_ascii_digit() || chars[i] == ':' || chars[i] == '|')
                {
                    // Repeat / volta marker — not a note
                    while i < chars.len() && chars[i] != ']' {
                        i += 1;
                    }
                    if i < chars.len() {
                        i += 1;
                    }
                } else {
                    // Simultaneous note chord — counts as one lyric syllable
                    assign_chord(&mut chord_at, &mut pending_chord, note_idx);
                    note_idx += 1;
                    while i < chars.len() && chars[i] != ']' {
                        i += 1;
                    }
                    if i < chars.len() {
                        i += 1;
                    }
                    skip_duration(&chars, &mut i);
                }
            }

            // Grace note group {cde} — ornament, does not advance lyric counter
            '{' => {
                i += 1;
                while i < chars.len() && chars[i] != '}' {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }

            // Decoration !...! or +...+
            '!' | '+' => {
                let closing = chars[i];
                i += 1;
                while i < chars.len() && chars[i] != closing {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
            }

            // Barlines | : (including complex forms like |: :| ::)
            '|' | ':' => {
                i += 1;
                while i < chars.len() && (chars[i] == '|' || chars[i] == ':') {
                    i += 1;
                }
            }

            // Tuplet marker (3, (2, etc. — doesn't add notes
            '(' => {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }

            // Everything else (spaces, broken-rhythm markers > <, ties -, etc.)
            _ => {
                i += 1;
            }
        }
    }

    // Any leftover pending chord (at the very end of the line) gets dropped;
    // there's no note to attach it to.

    (chord_at, note_idx)
}

/// Assign `pending_chord` to note `idx` if set, then clear `pending_chord`.
fn assign_chord(chord_at: &mut HashMap<usize, String>, pending: &mut Option<String>, idx: usize) {
    if let Some(chord) = pending.take() {
        chord_at.insert(idx, chord);
    }
}

fn is_note_letter(c: char) -> bool {
    matches!(c, 'A'..='G' | 'a'..='g')
}

/// Skip octave modifiers (`'`, `,`) and duration (`digits`, `/`).
fn skip_note_suffix(chars: &[char], i: &mut usize) {
    // Octave modifiers
    while *i < chars.len() && (chars[*i] == '\'' || chars[*i] == ',') {
        *i += 1;
    }
    skip_duration(chars, i);
}

/// Skip a duration suffix: digits and `/`.
fn skip_duration(chars: &[char], i: &mut usize) {
    while *i < chars.len() && (chars[*i].is_ascii_digit() || chars[*i] == '/') {
        *i += 1;
    }
}

// ---------------------------------------------------------------------------
// Lyric line parser
// ---------------------------------------------------------------------------

/// A tokenised ABC lyric element.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LyricToken {
    /// A syllable (text) that maps to the next note.
    Syllable(String),
    /// `_` — hold/extend: the previous syllable continues over this note.
    Hold,
    /// `*` — skip: advance the note counter without outputting text.
    Skip,
    /// `|` — bar synchronisation marker (ignored for note counting).
    Bar,
}

/// Parse an ABC `w:` lyric line (with the `w:` prefix already removed) into
/// a sequence of [`LyricToken`]s.
fn parse_abc_lyrics(w_line: &str) -> Vec<LyricToken> {
    let mut tokens: Vec<LyricToken> = Vec::new();
    let mut syllable = String::new();
    let chars: Vec<char> = w_line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' => {
                // Whitespace: end of current syllable
                if !syllable.is_empty() {
                    tokens.push(LyricToken::Syllable(syllable.clone()));
                    syllable.clear();
                }
                i += 1;
            }
            '-' => {
                // Hyphen: syllable boundary within a word.  Preserve the
                // hyphen so the output reads naturally (e.g., "Hel-lo").
                syllable.push('-');
                tokens.push(LyricToken::Syllable(syllable.clone()));
                syllable.clear();
                i += 1;
            }
            '_' => {
                // Hold: flush current syllable, then push a Hold token.
                if !syllable.is_empty() {
                    tokens.push(LyricToken::Syllable(syllable.clone()));
                    syllable.clear();
                }
                tokens.push(LyricToken::Hold);
                i += 1;
            }
            '*' => {
                // Skip: flush current syllable, then push a Skip token.
                if !syllable.is_empty() {
                    tokens.push(LyricToken::Syllable(syllable.clone()));
                    syllable.clear();
                }
                tokens.push(LyricToken::Skip);
                i += 1;
            }
            '|' => {
                // Bar synchronisation: flush syllable, push Bar token.
                if !syllable.is_empty() {
                    tokens.push(LyricToken::Syllable(syllable.clone()));
                    syllable.clear();
                }
                tokens.push(LyricToken::Bar);
                i += 1;
            }
            '~' => {
                // Tilde: word connector / extender (treated as a space in output)
                syllable.push(' ');
                i += 1;
            }
            '\\' if i + 1 < chars.len() && chars[i + 1] == 'n' => {
                // ABC line continuation `\n` embedded in w: line
                if !syllable.is_empty() {
                    tokens.push(LyricToken::Syllable(syllable.clone()));
                    syllable.clear();
                }
                i += 2;
            }
            c => {
                syllable.push(c);
                i += 1;
            }
        }
    }

    if !syllable.is_empty() {
        tokens.push(LyricToken::Syllable(syllable));
    }

    tokens
}

// ---------------------------------------------------------------------------
// ChordPro line builder
// ---------------------------------------------------------------------------

/// Build a ChordPro lyric line from a note→chord map and lyric tokens.
///
/// `note_count` is the total number of notes in the music block (used to
/// ensure any trailing chords beyond the lyrics are still emitted).
fn build_chordpro_line(chord_at: &HashMap<usize, String>, syllables: &[LyricToken]) -> String {
    let mut out = String::new();
    let mut note_idx: usize = 0;
    // Track whether a space is needed before the next syllable.  A space is
    // needed when the previous syllable did not end with a hyphen (i.e., the
    // next syllable starts a new word rather than continuing the same word).
    let mut need_space = false;

    for token in syllables {
        match token {
            LyricToken::Bar => {
                // Bar markers don't advance the note counter or emit text.
            }
            LyricToken::Hold => {
                // Hold: advance note counter; space state carries forward.
                note_idx += 1;
            }
            LyricToken::Skip => {
                // Skip: advance note counter, insert a space.
                out.push(' ');
                need_space = false;
                note_idx += 1;
            }
            LyricToken::Syllable(text) => {
                if need_space {
                    out.push(' ');
                }
                if let Some(chord) = chord_at.get(&note_idx) {
                    out.push('[');
                    out.push_str(&sanitize_chord_name(chord));
                    out.push(']');
                }
                let safe = sanitize_lyric_text(text);
                out.push_str(&safe);
                // A hyphen at the end means the next syllable continues the
                // same word — no inter-word space needed.
                need_space = !safe.ends_with('-');
                note_idx += 1;
            }
        }
    }

    // Emit any chords that fall beyond the last syllable (trailing chords).
    if let Some(&last_chord_note) = chord_at.keys().max() {
        while note_idx <= last_chord_note {
            if let Some(chord) = chord_at.get(&note_idx) {
                let safe = sanitize_chord_name(chord);
                out.push_str(&format!("[{safe}]"));
            }
            note_idx += 1;
        }
    }

    out.trim_end().to_string()
}

/// Build a chord-only line for music blocks that have no lyric lines.
fn build_chord_only_line(chord_at: &HashMap<usize, String>, note_count: usize) -> String {
    if chord_at.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = Vec::new();
    for i in 0..note_count.max(chord_at.keys().max().copied().unwrap_or(0) + 1) {
        if let Some(chord) = chord_at.get(&i) {
            parts.push(format!("[{}]", sanitize_chord_name(chord)));
        }
    }
    parts.join(" ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_field_line ---

    #[test]
    fn parse_field_title() {
        assert_eq!(parse_field_line("T:My Song"), Some(('T', "My Song")));
    }

    #[test]
    fn parse_field_with_spaces() {
        assert_eq!(
            parse_field_line("T: My Song Title "),
            Some(('T', "My Song Title"))
        );
    }

    #[test]
    fn parse_field_not_a_field() {
        assert_eq!(parse_field_line("Hello world"), None);
        assert_eq!(parse_field_line("|: CDEF |"), None);
    }

    // --- extract_tempo_bpm ---

    #[test]
    fn tempo_bare_number() {
        assert_eq!(extract_tempo_bpm("120"), Some("120".to_string()));
    }

    #[test]
    fn tempo_note_length() {
        assert_eq!(extract_tempo_bpm("1/4=120"), Some("120".to_string()));
    }

    #[test]
    fn tempo_with_label() {
        assert_eq!(
            extract_tempo_bpm("1/4=140 Allegro"),
            Some("140".to_string())
        );
    }

    #[test]
    fn tempo_text_only_returns_none() {
        // "Q:Allegro" has no numeric BPM — must not emit {tempo: Allegro}.
        assert_eq!(extract_tempo_bpm("Allegro"), None);
    }

    #[test]
    fn tempo_note_length_text_only_returns_none() {
        // "Q:1/4=Allegro" — after '=' the token is non-numeric.
        assert_eq!(extract_tempo_bpm("1/4=Allegro"), None);
    }

    // --- extract_chords_and_notes ---

    #[test]
    fn extract_chords_simple() {
        let (map, count) = extract_chords_and_notes("\"Am\" CDEF \"G\" GABC");
        assert_eq!(count, 8);
        assert_eq!(map.get(&0), Some(&"Am".to_string()));
        assert_eq!(map.get(&4), Some(&"G".to_string()));
        assert_eq!(map.get(&1), None);
    }

    #[test]
    fn extract_chords_with_rests() {
        let (map, count) = extract_chords_and_notes("\"Am\" C z E");
        assert_eq!(count, 3);
        assert_eq!(map.get(&0), Some(&"Am".to_string()));
        // z is a rest at note 1, E is at note 2
        assert_eq!(map.get(&2), None);
    }

    #[test]
    fn extract_chords_no_chord_nc() {
        // "N.C." should not produce a chord entry
        let (map, count) = extract_chords_and_notes("\"N.C.\" C D E");
        assert_eq!(count, 3);
        assert!(map.is_empty(), "N.C. should not produce a chord");
    }

    #[test]
    fn extract_chords_lowercase_notes() {
        let (map, count) = extract_chords_and_notes("\"C\" cdef");
        assert_eq!(count, 4);
        assert_eq!(map.get(&0), Some(&"C".to_string()));
    }

    #[test]
    fn extract_chords_accidentals() {
        let (map, count) = extract_chords_and_notes("\"F#m\" ^F G ^A B");
        assert_eq!(count, 4);
        assert_eq!(map.get(&0), Some(&"F#m".to_string()));
    }

    #[test]
    fn extract_chords_grace_notes_not_counted() {
        // Grace notes in {} should not advance the lyric counter
        let (map, count) = extract_chords_and_notes("\"Am\" {cde} C D");
        assert_eq!(count, 2);
        assert_eq!(map.get(&0), Some(&"Am".to_string()));
    }

    // --- parse_abc_lyrics ---

    #[test]
    fn lyrics_basic() {
        let tokens = parse_abc_lyrics("Hello world");
        assert_eq!(
            tokens,
            vec![
                LyricToken::Syllable("Hello".to_string()),
                LyricToken::Syllable("world".to_string()),
            ]
        );
    }

    #[test]
    fn lyrics_hyphen_continuation() {
        let tokens = parse_abc_lyrics("Hel-lo world");
        assert_eq!(
            tokens,
            vec![
                LyricToken::Syllable("Hel-".to_string()),
                LyricToken::Syllable("lo".to_string()),
                LyricToken::Syllable("world".to_string()),
            ]
        );
    }

    #[test]
    fn lyrics_hold_skip() {
        let tokens = parse_abc_lyrics("Hello _ world * end");
        assert_eq!(
            tokens,
            vec![
                LyricToken::Syllable("Hello".to_string()),
                LyricToken::Hold,
                LyricToken::Syllable("world".to_string()),
                LyricToken::Skip,
                LyricToken::Syllable("end".to_string()),
            ]
        );
    }

    #[test]
    fn lyrics_bar_marker() {
        let tokens = parse_abc_lyrics("Hello | world");
        assert_eq!(
            tokens,
            vec![
                LyricToken::Syllable("Hello".to_string()),
                LyricToken::Bar,
                LyricToken::Syllable("world".to_string()),
            ]
        );
    }

    // --- build_chordpro_line ---

    #[test]
    fn build_line_basic() {
        let mut map = HashMap::new();
        map.insert(0, "Am".to_string());
        map.insert(2, "G".to_string());
        let tokens = vec![
            LyricToken::Syllable("Hel-".to_string()),
            LyricToken::Syllable("lo".to_string()),
            LyricToken::Syllable("world".to_string()),
        ];
        let result = build_chordpro_line(&map, &tokens);
        assert_eq!(result, "[Am]Hel-lo [G]world");
    }

    #[test]
    fn build_line_hold_advances_counter() {
        let mut map = HashMap::new();
        map.insert(2, "G".to_string());
        let tokens = vec![
            LyricToken::Syllable("Hello".to_string()),
            LyricToken::Hold,
            LyricToken::Syllable("world".to_string()),
        ];
        let result = build_chordpro_line(&map, &tokens);
        assert_eq!(result, "Hello [G]world");
    }

    // --- convert_abc ---

    #[test]
    fn convert_basic_tune() {
        let input = "X:1\nT:Test Song\nK:C\n\"C\" CDEF \"G\" GABC|\nw:Hel-lo world how are you\n";
        let out = convert_abc(input);
        assert!(out.contains("{title: Test Song}"), "should have title");
        assert!(out.contains("{key: C}"), "should have key");
        assert!(out.contains("[C]"), "should have C chord");
        assert!(out.contains("[G]"), "should have G chord");
        assert!(out.contains("Hel-"), "should have lyrics");
    }

    #[test]
    fn convert_with_composer_and_tempo() {
        let input = "X:1\nT:My Song\nC:John Doe\nQ:1/4=120\nK:Am\n\"Am\" CDEF|\nw:Hello world\n";
        let out = convert_abc(input);
        assert!(out.contains("{title: My Song}"));
        assert!(out.contains("{composer: John Doe}"));
        assert!(out.contains("{tempo: 120}"));
        assert!(out.contains("{key: Am}"));
    }

    #[test]
    fn convert_multi_tune() {
        let input =
            "X:1\nT:First\nK:C\n\"C\" CDEF|\nw:Hello\n\nX:2\nT:Second\nK:G\n\"G\" GABC|\nw:World\n";
        let out = convert_abc(input);
        assert!(
            out.contains("{new_song}"),
            "should separate tunes with new_song"
        );
        assert!(out.contains("{title: First}"));
        assert!(out.contains("{title: Second}"));
    }

    #[test]
    fn convert_no_lyrics_emits_chord_line() {
        let input = "X:1\nT:Instrumental\nK:C\n\"C\" CDEF \"G\" GABC|\n";
        let out = convert_abc(input);
        assert!(
            out.contains("[C]"),
            "chord-only line should contain C chord"
        );
        assert!(
            out.contains("[G]"),
            "chord-only line should contain G chord"
        );
    }

    #[test]
    fn convert_part_labels() {
        let input =
            "X:1\nT:Song\nK:C\nP:A\n\"C\" CDEF|\nw:Hello world\nP:B\n\"G\" GABC|\nw:Bye world\n";
        let out = convert_abc(input);
        assert!(out.contains("{start_of_verse: A}"));
        assert!(out.contains("{end_of_verse}"));
        assert!(out.contains("{start_of_verse: B}"));
    }

    #[test]
    fn convert_empty_abc_returns_empty() {
        assert_eq!(convert_abc(""), "");
        assert_eq!(convert_abc("   \n  \n  "), "");
    }

    #[test]
    fn convert_abc_no_chord_nc_not_emitted() {
        let input = "X:1\nT:T\nK:C\n\"N.C.\" C D E F|\nw:Hello world how are\n";
        let out = convert_abc(input);
        assert!(
            !out.contains("[N.C.]"),
            "N.C. should not appear as a chord marker"
        );
        assert!(out.contains("Hello"), "lyrics should still be present");
    }

    #[test]
    fn convert_more_notes_than_syllables() {
        // When notes outnumber syllables, extra notes just have no text.
        let input = "X:1\nT:T\nK:C\n\"C\" C D E F G A B c|\nw:Hel-lo\n";
        let out = convert_abc(input);
        // Should not panic, should produce some output
        assert!(out.contains("[C]Hel-"));
    }

    #[test]
    fn convert_slash_chord() {
        let input = "X:1\nT:T\nK:C\n\"Am/E\" CDEF|\nw:Hello world\n";
        let out = convert_abc(input);
        assert!(out.contains("[Am/E]"), "slash chord should be preserved");
    }

    // --- sanitization ---

    #[test]
    fn escape_directive_value_strips_braces_not_whole_value() {
        // A title containing braces should have the braces stripped, not the
        // whole value discarded.
        assert_eq!(escape_directive_value("Song {live}"), "Song live");
        assert_eq!(escape_directive_value("{evil}"), "evil");
        assert_eq!(escape_directive_value("Normal"), "Normal");
    }

    #[test]
    fn convert_title_with_braces_strips_not_empties() {
        // Regression: title containing `{` or `}` must not become empty.
        let input = "X:1\nT:Song {live}\nK:C\n";
        let out = convert_abc(input);
        assert!(
            out.contains("{title: Song live}"),
            "braces in title should be stripped, got: {out}"
        );
    }

    #[test]
    fn sanitize_chord_name_strips_injection_chars() {
        assert_eq!(sanitize_chord_name("Am"), "Am");
        assert_eq!(sanitize_chord_name("Am][{title: evil}]["), "Amtitle: evil");
        assert_eq!(sanitize_chord_name("G{7}"), "G7");
    }

    #[test]
    fn convert_chord_with_injection_chars_sanitized() {
        // A chord annotation containing `]` and `{` must not inject directives
        // into the ChordPro output.
        let input = "X:1\nT:T\nK:C\n\"Am][{title: evil}][\" C D|\nw:Hello world\n";
        let out = convert_abc(input);
        assert!(
            !out.contains("{title: evil}"),
            "injected directive must not appear in output, got: {out}"
        );
        // The sanitized chord (braces and brackets stripped) should appear.
        assert!(
            out.contains("[Amtitle: evil]"),
            "sanitized chord name should be present, got: {out}"
        );
    }

    // --- non-numeric tempo (issue #1339) ---

    #[test]
    fn convert_tempo_text_only_omitted() {
        // Q:Allegro — no numeric BPM, so {tempo} must not be emitted.
        let input = "X:1\nT:T\nQ:Allegro\nK:C\n";
        let out = convert_abc(input);
        assert!(
            !out.contains("{tempo:"),
            "non-numeric tempo must not produce a {{tempo}} directive, got: {out}"
        );
    }

    #[test]
    fn convert_tempo_note_length_text_omitted() {
        // Q:1/4=Allegro — after '=' the token is non-numeric.
        let input = "X:1\nT:T\nQ:1/4=Allegro\nK:C\n";
        let out = convert_abc(input);
        assert!(
            !out.contains("{tempo:"),
            "non-numeric tempo must not produce a {{tempo}} directive, got: {out}"
        );
    }

    // --- multiple w: verses (issue #1340) ---

    #[test]
    fn convert_multi_verse_emits_all_verses() {
        // Two w: lines for the same music block — both verses must appear in output
        // and each must carry the shared chord annotation.
        let input = "X:1\nT:T\nK:C\n\"C\" C D E F|\nw:First verse here\nw:Second verse here\n";
        let out = convert_abc(input);
        assert!(
            out.contains("First"),
            "first verse must be present, got: {out}"
        );
        assert!(
            out.contains("Second"),
            "second verse must be present, got: {out}"
        );
        // The [C] chord annotation must appear once per verse line (shared chord map).
        let chord_count = out.matches("[C]").count();
        assert_eq!(
            chord_count, 2,
            "[C] chord must appear once per verse line (2 total), got {chord_count} in: {out}"
        );
    }

    // --- lyric sanitization (issue #1346) ---

    #[test]
    fn convert_lyric_braces_sanitized() {
        // w: lines containing { or } must not inject directive syntax into output.
        let input = "X:1\nT:T\nK:C\n\"C\" C D|\nw:Hello {world}\n";
        let out = convert_abc(input);
        assert!(
            !out.contains("{world}"),
            "brace injection must be stripped from lyrics, got: {out}"
        );
        assert!(
            out.contains("Hello"),
            "lyric text before braces must be preserved, got: {out}"
        );
        assert!(
            out.contains("world"),
            "lyric text inside braces must be preserved (braces stripped), got: {out}"
        );
    }

    // --- orphaned lyric sanitization (issue #1348) ---

    #[test]
    fn orphaned_lyric_braces_sanitized() {
        // w: lines with no preceding music line are output as plain text.
        // They must still have { and } stripped to prevent directive injection.
        let input = "X:1\nT:T\nK:C\nw:Orphaned {inject} line\n";
        let out = convert_abc(input);
        assert!(
            !out.contains("{inject}"),
            "brace injection must be stripped from orphaned lyrics, got: {out}"
        );
        assert!(
            out.contains("inject"),
            "text inside braces must be preserved (braces stripped), got: {out}"
        );
    }

    #[test]
    fn orphaned_lyric_brace_only_emits_no_blank_line() {
        // A w: line containing only braces sanitizes to an empty string and
        // must not produce a blank line in the output.
        let input = "X:1\nT:T\nK:C\nw:{}\n";
        let out = convert_abc(input);
        // The header block ends with "\n\n"; the brace-only lyric must not
        // append an additional "\n" that would produce "\n\n\n".
        assert!(
            !out.ends_with("\n\n\n"),
            "brace-only orphaned lyric must not emit an extra blank line, got: {out:?}"
        );
    }
}
