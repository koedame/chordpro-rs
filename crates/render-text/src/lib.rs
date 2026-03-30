//! Plain text renderer for ChordPro documents.
//!
//! This crate converts a parsed ChordPro AST (from `chordpro-core`) into
//! formatted plain text with chords aligned above their corresponding lyrics.

use chordpro_core::ast::{CommentStyle, DirectiveKind, Line, LyricsLine, Song};
use chordpro_core::transpose::transpose_chord;

/// Render a [`Song`] AST to plain text.
///
/// The output format:
/// - Title and subtitle are rendered as header lines.
/// - Section markers (chorus, verse, bridge, tab) render as labeled headers.
/// - Lyrics with chords produce two lines: chords above, lyrics below.
/// - Lyrics without chords produce a single lyrics line.
/// - Comments are rendered with style markers.
/// - Metadata directives (artist, key, capo, etc.) are silently consumed
///   (they populate [`Song::metadata`] but do not appear in the text body).
/// - Empty lines are preserved.
#[must_use]
pub fn render_song(song: &Song) -> String {
    render_song_with_transpose(song, 0)
}

/// Render a [`Song`] AST to plain text with an additional CLI transposition offset.
///
/// The `cli_transpose` parameter is added to any in-file `{transpose}` directive
/// values, allowing the CLI `--transpose` flag to combine with in-file directives.
#[must_use]
pub fn render_song_with_transpose(song: &Song, cli_transpose: i8) -> String {
    let mut output = Vec::new();
    let mut transpose_offset: i8 = cli_transpose;

    render_metadata(&song.metadata, &mut output);

    for line in &song.lines {
        match line {
            Line::Lyrics(lyrics_line) => {
                render_lyrics(lyrics_line, transpose_offset, &mut output);
            }
            Line::Directive(directive) => {
                if directive.kind == DirectiveKind::Transpose {
                    // {transpose: N} sets the in-file transposition amount.
                    // A subsequent {transpose} replaces (not adds to) the
                    // previous in-file value. CLI offset is always added.
                    let file_offset: i8 = directive
                        .value
                        .as_deref()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    transpose_offset = file_offset.saturating_add(cli_transpose);
                } else if !directive.kind.is_metadata() {
                    render_directive(directive, &mut output);
                }
            }
            Line::Comment(style, text) => render_comment(*style, text, &mut output),
            Line::Empty => output.push(String::new()),
        }
    }

    // Remove trailing empty lines, then add a final newline.
    while output.last().is_some_and(|l| l.is_empty()) {
        output.pop();
    }

    if output.is_empty() {
        return String::new();
    }

    let mut result = output.join("\n");
    result.push('\n');
    result
}

/// Parse a ChordPro source string and render it to plain text.
///
/// Returns `Ok(text)` on success, or the [`chordpro_core::ParseError`] if
/// the input cannot be parsed.
///
/// For pre-parsed input, use [`render_song`] directly.
pub fn try_render(input: &str) -> Result<String, chordpro_core::ParseError> {
    let song = chordpro_core::parse(input)?;
    Ok(render_song(&song))
}

/// Parse a ChordPro source string and render it to plain text.
///
/// This is a convenience wrapper around [`try_render`] that converts parse
/// errors into a human-readable error string. Because success and failure
/// both return a `String`, callers **cannot** distinguish between them
/// programmatically — use [`try_render`] if you need error handling.
#[must_use]
pub fn render(input: &str) -> String {
    match try_render(input) {
        Ok(text) => text,
        Err(e) => format!(
            "Parse error at line {} column {}: {}\n",
            e.line(),
            e.column(),
            e.message
        ),
    }
}

// ---------------------------------------------------------------------------
// Metadata header
// ---------------------------------------------------------------------------

/// Render the song metadata (title, subtitle) as a header block.
///
/// No trailing blank line is added — the document's own empty lines
/// provide spacing between the metadata header and the song body.
fn render_metadata(metadata: &chordpro_core::ast::Metadata, output: &mut Vec<String>) {
    if let Some(title) = &metadata.title {
        output.push(title.clone());
    }
    for subtitle in &metadata.subtitles {
        output.push(subtitle.clone());
    }
}

// ---------------------------------------------------------------------------
// Lyrics rendering (chord-over-lyrics alignment)
// ---------------------------------------------------------------------------

/// Render a lyrics line with chords aligned above the lyrics.
///
/// If the line has chords, two lines are produced:
///   1. A chord line with each chord positioned above its lyrics segment.
///   2. The lyrics text.
///
/// If the line has no chords, only the lyrics text is emitted.
///
/// Alignment is based on character count (`chars().count()`), which correctly
/// handles multi-byte UTF-8 sequences in lyrics text.
fn render_lyrics(lyrics_line: &LyricsLine, transpose_offset: i8, output: &mut Vec<String>) {
    if !lyrics_line.has_chords() {
        output.push(lyrics_line.text());
        return;
    }

    let mut chord_line = String::new();
    let mut lyric_line = String::new();

    for segment in &lyrics_line.segments {
        let transposed;
        let chord_name = if transpose_offset != 0 {
            if let Some(chord) = &segment.chord {
                transposed = transpose_chord(chord, transpose_offset);
                transposed.name.as_str()
            } else {
                ""
            }
        } else {
            segment.chord.as_ref().map_or("", |c| c.name.as_str())
        };
        let text = &segment.text;

        let chord_len = chord_name.chars().count();
        let text_len = text.chars().count();

        // Write the chord (or equivalent spacing) on the chord line.
        chord_line.push_str(chord_name);

        // Write the text on the lyric line.
        lyric_line.push_str(text);

        // Ensure alignment: both lines must advance to at least the same column.
        // If the chord is longer than the text, pad the lyric line.
        // If the text is longer than the chord, pad the chord line.
        // Add 1 space of padding after chord when chord >= text length,
        // so chords don't run together.
        if chord_len > 0 && chord_len >= text_len {
            let padding = chord_len - text_len + 1;
            lyric_line.extend(std::iter::repeat_n(' ', padding));
            chord_line.push(' ');
        } else if chord_len > 0 && text_len > chord_len {
            let padding = text_len - chord_len;
            chord_line.extend(std::iter::repeat_n(' ', padding));
        }
        // If chord_len == 0 (no chord), text just advances lyric_line naturally
        // and chord_line needs to keep up.
        if chord_len == 0 && text_len > 0 {
            chord_line.extend(std::iter::repeat_n(' ', text_len));
        }
    }

    output.push(chord_line.trim_end().to_string());
    output.push(lyric_line.trim_end().to_string());
}

// ---------------------------------------------------------------------------
// Directive rendering
// ---------------------------------------------------------------------------

/// Render a directive to text output.
///
/// - Section start directives produce a labeled header (e.g., "Chorus").
/// - Section end directives are not rendered (they are structural markers).
/// - Metadata directives are not rendered here (handled by `render_metadata`).
/// - Unknown directives are silently ignored.
fn render_directive(directive: &chordpro_core::ast::Directive, output: &mut Vec<String>) {
    match &directive.kind {
        DirectiveKind::StartOfChorus => {
            render_section_header("Chorus", &directive.value, output);
        }
        DirectiveKind::StartOfVerse => {
            render_section_header("Verse", &directive.value, output);
        }
        DirectiveKind::StartOfBridge => {
            render_section_header("Bridge", &directive.value, output);
        }
        DirectiveKind::StartOfTab => {
            render_section_header("Tab", &directive.value, output);
        }
        DirectiveKind::StartOfGrid => {
            render_section_header("Grid", &directive.value, output);
        }
        DirectiveKind::StartOfSection(section_name) => {
            // Capitalize the first letter of the section name for display.
            let label = capitalize(section_name);
            render_section_header(&label, &directive.value, output);
        }
        // End-of-section, metadata, and unknown directives produce no output.
        _ => {}
    }
}

/// Capitalize the first character of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Render a section header like "Chorus" or "Verse 1".
fn render_section_header(label: &str, value: &Option<String>, output: &mut Vec<String>) {
    match value {
        Some(v) if !v.is_empty() => output.push(format!("[{label}: {v}]")),
        _ => output.push(format!("[{label}]")),
    }
}

// ---------------------------------------------------------------------------
// Comment rendering
// ---------------------------------------------------------------------------

/// Render a comment with its style marker.
///
/// - Normal comments: `(comment text)`
/// - Italic comments: `(*comment text*)`
/// - Boxed comments:  `[comment text]`
fn render_comment(style: CommentStyle, text: &str, output: &mut Vec<String>) {
    match style {
        CommentStyle::Normal => output.push(format!("({text})")),
        CommentStyle::Italic => output.push(format!("(*{text}*)")),
        CommentStyle::Boxed => output.push(format!("[{text}]")),
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_empty() {
        assert_eq!(render(""), "");
    }

    #[test]
    fn test_render_title_only() {
        let input = "{title: Amazing Grace}";
        let output = render(input);
        assert_eq!(output, "Amazing Grace\n");
    }

    #[test]
    fn test_render_title_and_subtitle() {
        let input = "{title: Amazing Grace}\n{subtitle: Traditional}";
        let output = render(input);
        assert_eq!(output, "Amazing Grace\nTraditional\n");
    }

    #[test]
    fn test_render_plain_lyrics() {
        let input = "Hello world\nSecond line";
        let output = render(input);
        assert_eq!(output, "Hello world\nSecond line\n");
    }

    #[test]
    fn test_render_lyrics_with_chords() {
        let input = "[Am]Hello [G]world";
        let output = render(input);
        assert_eq!(output, "Am    G\nHello world\n");
    }

    #[test]
    fn test_render_chord_longer_than_text() {
        // Chord "Cmaj7" is 5 chars, text "I" is 1 char
        let input = "[Cmaj7]I [G]see";
        let output = render(input);
        assert_eq!(output, "Cmaj7 G\nI     see\n");
    }

    #[test]
    fn test_render_chorus_section() {
        let input = "{start_of_chorus}\n[G]La la la\n{end_of_chorus}";
        let output = render(input);
        assert_eq!(output, "[Chorus]\nG\nLa la la\n");
    }

    #[test]
    fn test_render_verse_with_label() {
        let input = "{start_of_verse: Verse 1}\nSome lyrics\n{end_of_verse}";
        let output = render(input);
        assert_eq!(output, "[Verse: Verse 1]\nSome lyrics\n");
    }

    #[test]
    fn test_render_comment_normal() {
        let input = "{comment: This is a comment}";
        let output = render(input);
        assert_eq!(output, "(This is a comment)\n");
    }

    #[test]
    fn test_render_comment_italic() {
        let input = "{comment_italic: Softly}";
        let output = render(input);
        assert_eq!(output, "(*Softly*)\n");
    }

    #[test]
    fn test_render_comment_box() {
        let input = "{comment_box: Important}";
        let output = render(input);
        assert_eq!(output, "[Important]\n");
    }

    #[test]
    fn test_render_empty_lines_preserved() {
        let input = "Line one\n\nLine two";
        let output = render(input);
        assert_eq!(output, "Line one\n\nLine two\n");
    }

    #[test]
    fn test_render_metadata_not_duplicated() {
        // Metadata directives like {artist} should NOT appear in body text
        let input = "{title: Test}\n{artist: Someone}\n{key: G}\nLyrics here";
        let output = render(input);
        assert_eq!(output, "Test\nLyrics here\n");
    }

    #[test]
    fn test_render_full_song() {
        let input = "\
{title: Amazing Grace}
{subtitle: Traditional}
{key: G}

{start_of_verse}
[G]Amazing [G7]grace, how [C]sweet the [G]sound
[G]That saved a [Em]wretch like [D]me
{end_of_verse}

{start_of_chorus}
[G]I once was [G7]lost, but [C]now am [G]found
{end_of_chorus}";
        let output = render(input);
        // Just verify it doesn't panic and produces non-empty output
        assert!(!output.is_empty());
        assert!(output.contains("Amazing Grace"));
        assert!(output.contains("[Verse]"));
        assert!(output.contains("[Chorus]"));
    }

    #[test]
    fn test_render_song_api() {
        let song = chordpro_core::parse("{title: Test}\n[Am]Hello").unwrap();
        let output = render_song(&song);
        assert!(output.contains("Test"));
        assert!(output.contains("Am"));
        assert!(output.contains("Hello"));
    }

    #[test]
    fn test_render_chord_only_segment() {
        // A chord at end of line with no following text
        let input = "[Am]Hello [G]";
        let output = render(input);
        assert!(output.contains("Am"));
        assert!(output.contains("G"));
        assert!(output.contains("Hello"));
    }

    #[test]
    fn test_render_bridge_section() {
        let input = "{start_of_bridge}\nBridge lyrics\n{end_of_bridge}";
        let output = render(input);
        assert_eq!(output, "[Bridge]\nBridge lyrics\n");
    }

    #[test]
    fn test_render_tab_section() {
        let input = "{start_of_tab}\ne|---0---|\n{end_of_tab}";
        let output = render(input);
        assert_eq!(output, "[Tab]\ne|---0---|\n");
    }

    // --- Issue #65: Unicode alignment ---

    #[test]
    fn test_render_multibyte_lyrics_alignment() {
        // Japanese text: each char is 3 bytes but 1 char
        let input = "[Am]こんにちは [G]世界";
        let output = render(input);
        // "こんにちは " = 6 chars, "Am" = 2 chars → pad chord line by 4
        // "世界" = 2 chars, "G" = 1 char → pad chord line by 1
        assert_eq!(output, "Am    G\nこんにちは 世界\n");
    }

    #[test]
    fn test_render_accented_lyrics_alignment() {
        let input = "[Em]café [D]résumé";
        let output = render(input);
        assert_eq!(output, "Em   D\ncafé résumé\n");
    }

    // --- Issue #66: Text before first chord ---

    #[test]
    fn test_render_text_before_first_chord() {
        let input = "Hello [Am]world";
        let output = render(input);
        assert_eq!(output, "      Am\nHello world\n");
    }

    #[test]
    fn test_render_text_before_first_chord_multiple() {
        let input = "I say [Am]hello [G]world";
        let output = render(input);
        assert_eq!(output, "      Am    G\nI say hello world\n");
    }

    // --- Issue #67: try_render API ---

    #[test]
    fn test_try_render_success() {
        let result = try_render("{title: Test}\n[Am]Hello");
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("Test"));
        assert!(text.contains("Am"));
    }

    #[test]
    fn test_try_render_parse_error() {
        let result = try_render("{title: unclosed");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.line(), 1);
    }

    #[test]
    fn test_render_grid_section() {
        let input = "{start_of_grid}\n| Am . | C . |\n{end_of_grid}";
        let output = render(input);
        assert_eq!(output, "[Grid]\n| Am . | C . |\n");
    }

    #[test]
    fn test_render_grid_section_with_label() {
        let input = "{start_of_grid: Intro}\n| Am . | C . |\n{end_of_grid}";
        let output = render(input);
        assert_eq!(output, "[Grid: Intro]\n| Am . | C . |\n");
    }

    #[test]
    fn test_render_grid_short_alias() {
        let input = "{sog}\n| G . | D . |\n{eog}";
        let output = render(input);
        assert_eq!(output, "[Grid]\n| G . | D . |\n");
    }

    // --- Custom sections (#108) ---

    #[test]
    fn test_render_custom_section_intro() {
        let input = "{start_of_intro}\n[Am]Da da da\n{end_of_intro}";
        let output = render(input);
        assert!(output.contains("[Intro]"));
        assert!(output.contains("Am"));
        assert!(output.contains("Da da da"));
    }

    #[test]
    fn test_render_custom_section_with_label() {
        let input = "{start_of_intro: Guitar}\nSome notes\n{end_of_intro}";
        let output = render(input);
        assert_eq!(output, "[Intro: Guitar]\nSome notes\n");
    }

    #[test]
    fn test_render_custom_section_outro() {
        let input = "{start_of_outro}\nFinal notes\n{end_of_outro}";
        let output = render(input);
        assert!(output.contains("[Outro]"));
    }

    #[test]
    fn test_render_custom_section_solo() {
        let input = "{start_of_solo}\n[Em]Solo line\n{end_of_solo}";
        let output = render(input);
        assert!(output.contains("[Solo]"));
    }
}

#[cfg(test)]
mod transpose_tests {
    use super::*;

    #[test]
    fn test_transpose_directive_up_2() {
        let input = "{transpose: 2}\n[G]Hello [C]world";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song(&song);
        // G+2=A, C+2=D
        assert_eq!(output, "A     D\nHello world\n");
    }

    #[test]
    fn test_transpose_directive_down_3() {
        let input = "{transpose: -3}\n[Am]Hello [Em]world";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song(&song);
        // Am-3=F#m, Em-3=C#m
        assert_eq!(output, "F#m   C#m\nHello world\n");
    }

    #[test]
    fn test_transpose_directive_replaces_previous() {
        let input = "{transpose: 2}\n[G]First\n{transpose: -1}\n[G]Second";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song(&song);
        // First line: G+2=A, Second line: G-1=F#
        assert!(output.contains("A\nFirst"));
        assert!(output.contains("F#\nSecond"));
    }

    #[test]
    fn test_transpose_directive_zero_resets() {
        let input = "{transpose: 5}\n[C]Up\n{transpose: 0}\n[C]Normal";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song(&song);
        // First line: C+5=F, Second line: C+0=C
        assert!(output.contains("F\nUp"));
        assert!(output.contains("C\nNormal"));
    }

    #[test]
    fn test_transpose_directive_with_cli_offset() {
        let input = "{transpose: 2}\n[C]Hello";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song_with_transpose(&song, 3);
        // In-file 2 + CLI 3 = 5 total. C+5=F
        assert!(output.contains("F\nHello"));
    }

    #[test]
    fn test_cli_transpose_without_directive() {
        let input = "[G]Hello [C]world";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song_with_transpose(&song, 2);
        // CLI offset only: G+2=A, C+2=D
        assert_eq!(output, "A     D\nHello world\n");
    }

    #[test]
    fn test_transpose_directive_replaces_with_cli_additive() {
        let input = "{transpose: 2}\n[C]First\n{transpose: -1}\n[C]Second";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song_with_transpose(&song, 1);
        // First: 2+1=3, C+3=D#. Second: -1+1=0, C+0=C
        assert!(output.contains("D#\nFirst"));
        assert!(output.contains("C\nSecond"));
    }

    #[test]
    fn test_transpose_no_chord_lyrics_unaffected() {
        let input = "{transpose: 5}\nPlain lyrics no chords";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song(&song);
        assert_eq!(output, "Plain lyrics no chords\n");
    }

    #[test]
    fn test_transpose_invalid_value_treated_as_zero() {
        let input = "{transpose: abc}\n[G]Hello";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song(&song);
        // Invalid value -> treated as 0
        assert!(output.contains("G\nHello"));
    }

    #[test]
    fn test_transpose_no_value_treated_as_zero() {
        let input = "{transpose}\n[G]Hello";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song(&song);
        // No value -> treated as 0
        assert!(output.contains("G\nHello"));
    }
}
