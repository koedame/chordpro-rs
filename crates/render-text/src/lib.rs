//! Plain text renderer for ChordPro documents.
//!
//! This crate converts a parsed ChordPro AST (from `chordpro-core`) into
//! formatted plain text with chords aligned above their corresponding lyrics.

use chordpro_core::ast::{CommentStyle, DirectiveKind, Line, LyricsLine, Song};
use chordpro_core::config::Config;
use chordpro_core::transpose::transpose_chord;

/// Render a [`Song`] AST to plain text.
///
/// The output format:
/// - Title and subtitle are rendered as header lines.
/// - Section markers (chorus, verse, bridge, tab) render as labeled headers.
/// - Lyrics with chords produce two lines: chords above, lyrics below.
/// - Lyrics without chords produce a single lyrics line.
/// - Comments are rendered with style markers.
/// - The `{chorus}` directive recalls (re-renders) the most recently defined
///   chorus section. If no chorus has been defined yet, a `[Chorus]` marker
///   is emitted instead.
/// - Metadata directives (artist, key, capo, etc.) are silently consumed
///   (they populate [`Song::metadata`] but do not appear in the text body).
/// - Empty lines are preserved.
#[must_use]
pub fn render_song(song: &Song) -> String {
    render_song_with_transpose(song, 0, &Config::defaults())
}

/// Render a [`Song`] AST to plain text with an additional CLI transposition offset.
///
/// The `cli_transpose` parameter is added to any in-file `{transpose}` directive
/// values, allowing the CLI `--transpose` flag to combine with in-file directives.
#[must_use]
pub fn render_song_with_transpose(song: &Song, cli_transpose: i8, config: &Config) -> String {
    // Config is plumbed through for future use (e.g., suppress_empty_chords).
    let _ = config;
    let mut output = Vec::new();
    let mut transpose_offset: i8 = cli_transpose;
    // Stores the rendered text lines of the most recently defined chorus,
    // excluding the "[Chorus]" header itself. Used by `{chorus}` recall.
    let mut chorus_lines: Vec<String> = Vec::new();
    // Temporary buffer for collecting chorus content while inside a chorus section.
    let mut chorus_buf: Option<Vec<String>> = None;

    render_metadata(&song.metadata, &mut output);

    for line in &song.lines {
        match line {
            Line::Lyrics(lyrics_line) => {
                let mut target = Vec::new();
                render_lyrics(lyrics_line, transpose_offset, &mut target);
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.extend(target.iter().cloned());
                }
                output.extend(target);
            }
            Line::Directive(directive) => {
                // Metadata directives are already rendered via song.metadata;
                // skip them in the body to avoid duplicate output.
                if directive.kind.is_metadata() {
                    continue;
                }
                if directive.kind == DirectiveKind::Transpose {
                    // {transpose: N} sets the in-file transposition amount.
                    let file_offset: i8 = directive
                        .value
                        .as_deref()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    transpose_offset = file_offset.saturating_add(cli_transpose);
                    continue;
                }
                match &directive.kind {
                    DirectiveKind::StartOfChorus => {
                        render_section_header("Chorus", &directive.value, &mut output);
                        // Begin collecting chorus content lines.
                        chorus_buf = Some(Vec::new());
                    }
                    DirectiveKind::EndOfChorus => {
                        // Finish collecting: store the buffered lines as the
                        // most recent chorus for future recall.
                        if let Some(buf) = chorus_buf.take() {
                            chorus_lines = buf;
                        }
                    }
                    DirectiveKind::Chorus => {
                        render_chorus_recall(&directive.value, &chorus_lines, &mut output);
                    }
                    _ => {
                        let mut target = Vec::new();
                        render_directive(directive, &mut target);
                        if let Some(buf) = chorus_buf.as_mut() {
                            buf.extend(target.iter().cloned());
                        }
                        output.extend(target);
                    }
                }
            }
            Line::Comment(style, text) => {
                let mut target = Vec::new();
                render_comment(*style, text, &mut target);
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.extend(target.iter().cloned());
                }
                output.extend(target);
            }
            Line::Empty => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(String::new());
                }
                output.push(String::new());
            }
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

/// Render multiple [`Song`]s to plain text, separated by a blank line.
#[must_use]
pub fn render_songs(songs: &[Song]) -> String {
    render_songs_with_transpose(songs, 0, &Config::defaults())
}

/// Render multiple [`Song`]s to plain text with transposition.
#[must_use]
pub fn render_songs_with_transpose(songs: &[Song], cli_transpose: i8, config: &Config) -> String {
    songs
        .iter()
        .map(|song| render_song_with_transpose(song, cli_transpose, config))
        .collect::<Vec<_>>()
        .join("\n\n")
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
                transposed.display_name()
            } else {
                ""
            }
        } else {
            segment.chord.as_ref().map_or("", |c| c.display_name())
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
        DirectiveKind::StartOfAbc => {
            render_section_header("ABC", &directive.value, output);
        }
        DirectiveKind::StartOfLy => {
            render_section_header("Lilypond", &directive.value, output);
        }
        DirectiveKind::StartOfSvg => {
            render_section_header("SVG", &directive.value, output);
        }
        DirectiveKind::StartOfTextblock => {
            render_section_header("Textblock", &directive.value, output);
        }
        DirectiveKind::StartOfSection(section_name) => {
            // Capitalize the first letter of the section name for display.
            let label = chordpro_core::capitalize(section_name);
            render_section_header(&label, &directive.value, output);
        }
        // End-of-section, metadata, and unknown directives produce no output.
        _ => {}
    }
}

/// Render a `{chorus}` recall directive.
///
/// If a chorus has been previously defined (via `{start_of_chorus}`...`{end_of_chorus}`),
/// its content is replayed with a `[Chorus]` (or custom label) header. If no chorus
/// has been defined, only the header marker is emitted.
fn render_chorus_recall(value: &Option<String>, chorus_lines: &[String], output: &mut Vec<String>) {
    render_section_header("Chorus", value, output);
    output.extend(chorus_lines.iter().cloned());
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
mod multi_song_tests {
    use super::*;

    #[test]
    fn test_render_songs_two_songs() {
        let songs = chordpro_core::parse_multi(
            "{title: Song One}\n[Am]Hello\n{new_song}\n{title: Song Two}\n[G]World",
        )
        .unwrap();
        let output = render_songs(&songs);
        assert!(output.contains("Song One"));
        assert!(output.contains("Am"));
        assert!(output.contains("Hello"));
        assert!(output.contains("Song Two"));
        assert!(output.contains("G\nWorld"));
        // Two songs are separated by a blank line (double newline between them)
        assert!(output.contains("\n\n"));
    }

    #[test]
    fn test_render_songs_single_song() {
        let songs = chordpro_core::parse_multi("{title: Only One}\nLyrics").unwrap();
        let output = render_songs(&songs);
        assert_eq!(output, render_song(&songs[0]));
    }

    #[test]
    fn test_render_songs_with_transpose() {
        let songs =
            chordpro_core::parse_multi("{title: S1}\n[C]Do\n{new_song}\n{title: S2}\n[G]Re")
                .unwrap();
        let output = render_songs_with_transpose(&songs, 2, &Config::defaults());
        // C+2=D, G+2=A
        assert!(output.contains("D\nDo"));
        assert!(output.contains("A\nRe"));
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
        let output = render_song_with_transpose(&song, 3, &Config::defaults());
        // In-file 2 + CLI 3 = 5 total. C+5=F
        assert!(output.contains("F\nHello"));
    }

    #[test]
    fn test_cli_transpose_without_directive() {
        let input = "[G]Hello [C]world";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song_with_transpose(&song, 2, &Config::defaults());
        // CLI offset only: G+2=A, C+2=D
        assert_eq!(output, "A     D\nHello world\n");
    }

    #[test]
    fn test_transpose_directive_replaces_with_cli_additive() {
        let input = "{transpose: 2}\n[C]First\n{transpose: -1}\n[C]Second";
        let song = chordpro_core::parse(input).unwrap();
        let output = render_song_with_transpose(&song, 1, &Config::defaults());
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

    // --- Issue #109: {chorus} recall ---

    #[test]
    fn test_render_chorus_recall_basic() {
        let input = "\
{start_of_chorus}
[G]La la la
{end_of_chorus}

{start_of_verse}
Some verse
{end_of_verse}

{chorus}";
        let output = render(input);
        // The chorus content should appear twice: once in the original and once recalled
        assert_eq!(
            output,
            "[Chorus]\nG\nLa la la\n\n[Verse]\nSome verse\n\n[Chorus]\nG\nLa la la\n"
        );
    }

    #[test]
    fn test_render_chorus_recall_with_label() {
        let input = "\
{start_of_chorus}
Sing along
{end_of_chorus}

{chorus: Repeat}";
        let output = render(input);
        assert!(output.contains("[Chorus: Repeat]"));
        assert_eq!(
            output,
            "[Chorus]\nSing along\n\n[Chorus: Repeat]\nSing along\n"
        );
    }

    #[test]
    fn test_render_chorus_recall_no_chorus_defined() {
        // When {chorus} is used before any chorus is defined, just show the header
        let input = "{chorus}";
        let output = render(input);
        assert_eq!(output, "[Chorus]\n");
    }

    #[test]
    fn test_render_chorus_recall_multiple() {
        let input = "\
{start_of_chorus}
Chorus line
{end_of_chorus}
{chorus}
{chorus}";
        let output = render(input);
        // Original chorus + two recalls
        assert_eq!(
            output,
            "[Chorus]\nChorus line\n[Chorus]\nChorus line\n[Chorus]\nChorus line\n"
        );
    }

    #[test]
    fn test_render_chorus_recall_uses_latest() {
        // If there are multiple chorus definitions, recall uses the latest
        let input = "\
{start_of_chorus}
First chorus
{end_of_chorus}

{start_of_chorus}
Second chorus
{end_of_chorus}

{chorus}";
        let output = render(input);
        // The recall should use "Second chorus", not "First chorus"
        assert!(output.ends_with("[Chorus]\nSecond chorus\n"));
    }
}

#[cfg(test)]
mod delegate_tests {
    use super::*;

    #[test]
    fn test_render_abc_section() {
        let input = "{start_of_abc}\nX:1\nK:G\n{end_of_abc}";
        let output = render(input);
        assert!(output.contains("[ABC]"));
        assert!(output.contains("X:1"));
    }

    #[test]
    fn test_render_abc_section_with_label() {
        let input = "{start_of_abc: Melody}\nX:1\n{end_of_abc}";
        let output = render(input);
        assert_eq!(output, "[ABC: Melody]\nX:1\n");
    }

    #[test]
    fn test_render_ly_section() {
        let input = "{start_of_ly}\nnotes\n{end_of_ly}";
        let output = render(input);
        assert!(output.contains("[Lilypond]"));
    }

    #[test]
    fn test_render_svg_section() {
        let input = "{start_of_svg}\n<svg/>\n{end_of_svg}";
        let output = render(input);
        assert!(output.contains("[SVG]"));
    }

    #[test]
    fn test_render_textblock_section() {
        let input = "{start_of_textblock}\nPreformatted text\n{end_of_textblock}";
        let output = render(input);
        assert!(output.contains("[Textblock]"));
        assert!(output.contains("Preformatted text"));
    }

    #[test]
    fn test_delegate_verbatim_no_chords() {
        let input = "{start_of_textblock}\n[Am]Not a chord\n{end_of_textblock}";
        let output = render(input);
        assert!(output.contains("[Am]Not a chord"));
    }

    // -- inline markup rendering (plain text strips all tags) ------------------

    #[test]
    fn test_markup_stripped_in_text_output() {
        let output = render("Hello <b>bold</b> world");
        assert!(output.contains("Hello bold world"));
        assert!(!output.contains("<b>"));
        assert!(!output.contains("</b>"));
    }

    #[test]
    fn test_markup_stripped_with_chord() {
        let output = render("[Am]Hello <b>bold</b> world");
        assert!(output.contains("Am"));
        assert!(output.contains("Hello bold world"));
        assert!(!output.contains("<b>"));
    }

    #[test]
    fn test_span_markup_stripped_in_text_output() {
        let output = render(r#"<span foreground="red">red text</span>"#);
        assert!(output.contains("red text"));
        assert!(!output.contains("<span"));
        assert!(!output.contains("foreground"));
    }
}
