//! Plain text renderer for ChordPro documents.
//!
//! This crate converts a parsed ChordPro AST (from `chordsketch-core`) into
//! formatted plain text with chords aligned above their corresponding lyrics.

use chordsketch_core::ast::{CommentStyle, DirectiveKind, Line, LyricsLine, Song};
use chordsketch_core::config::Config;
use chordsketch_core::render_result::RenderResult;
use chordsketch_core::resolve_diagrams_instrument;
use chordsketch_core::transpose::transpose_chord;
use unicode_width::UnicodeWidthStr;

/// Maximum number of chorus recall directives allowed per song.
/// Prevents output amplification from malicious inputs with many `{chorus}` lines.
const MAX_CHORUS_RECALLS: usize = 1000;

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
///
/// Warnings are printed to stderr via `eprintln!`. Use
/// [`render_song_with_warnings`] to capture them programmatically.
#[must_use]
pub fn render_song_with_transpose(song: &Song, cli_transpose: i8, config: &Config) -> String {
    let result = render_song_with_warnings(song, cli_transpose, config);
    for w in &result.warnings {
        eprintln!("warning: {w}");
    }
    result.output
}

/// Render a [`Song`] AST to plain text, returning warnings programmatically.
///
/// This is the structured variant of [`render_song_with_transpose`]. Instead
/// of printing warnings to stderr, they are collected into
/// [`RenderResult::warnings`].
#[must_use = "caller must check warnings in the returned RenderResult"]
pub fn render_song_with_warnings(
    song: &Song,
    cli_transpose: i8,
    config: &Config,
) -> RenderResult<String> {
    let mut warnings = Vec::new();
    let output = render_song_impl(song, cli_transpose, config, &mut warnings);
    RenderResult::with_warnings(output, warnings)
}

/// Internal implementation that renders a song and collects warnings.
fn render_song_impl(
    song: &Song,
    cli_transpose: i8,
    config: &Config,
    warnings: &mut Vec<String>,
) -> String {
    // Apply song-level config overrides ({+config.KEY: VALUE} directives).
    // The effective config is not yet consumed by the text renderer but is
    // computed here for consistency with the HTML and PDF renderers, and will
    // be used when text-specific config settings are added.
    let song_overrides = song.config_overrides();
    let song_config;
    let _config = if song_overrides.is_empty() {
        config
    } else {
        song_config = config
            .clone()
            .with_song_overrides(&song_overrides, warnings);
        &song_config
    };
    // Extract song-level transpose delta from {+config.settings.transpose}.
    // The base config transpose is already folded into cli_transpose by the caller.
    let song_transpose_delta = Config::song_transpose_delta(&song_overrides);
    let mut output = Vec::new();
    let (combined_transpose, _) =
        chordsketch_core::transpose::combine_transpose(cli_transpose, song_transpose_delta);
    let mut transpose_offset: i8 = combined_transpose;
    // Stores the AST lines of the most recently defined chorus body.
    // Re-rendered at recall time so the current transpose offset is applied.
    let mut chorus_body: Vec<Line> = Vec::new();
    // Temporary buffer for collecting chorus AST lines.
    let mut chorus_buf: Option<Vec<Line>> = None;
    let mut chorus_recall_count: usize = 0;

    // Instrument for the auto-inject ASCII diagram block.
    // Set by {diagrams: guitar/ukulele/on}; cleared by {diagrams: off} / {no_diagrams}.
    let default_instrument = _config
        .get_path("diagrams.instrument")
        .as_str()
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "guitar".to_string());
    let mut auto_diagrams_instrument: Option<String> = None;

    render_metadata(&song.metadata, &mut output);

    for line in &song.lines {
        match line {
            Line::Lyrics(lyrics_line) => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                render_lyrics(lyrics_line, transpose_offset, &mut output);
            }
            Line::Directive(directive) => {
                // Metadata directives are already rendered via song.metadata;
                // skip them in the body to avoid duplicate output.
                if directive.kind.is_metadata() {
                    continue;
                }
                if directive.kind == DirectiveKind::Diagrams {
                    auto_diagrams_instrument = resolve_diagrams_instrument(
                        directive.value.as_deref(),
                        &default_instrument,
                    );
                    continue;
                }
                if directive.kind == DirectiveKind::NoDiagrams {
                    auto_diagrams_instrument = None;
                    continue;
                }
                if directive.kind == DirectiveKind::Transpose {
                    // {transpose: N} sets the in-file transposition amount.
                    // A missing or empty value silently resets to 0; only a
                    // non-empty value that cannot be parsed as i8 emits a warning.
                    let file_offset: i8 = match directive.value.as_deref() {
                        None | Some("") => 0,
                        Some(raw) => match raw.parse() {
                            Ok(v) => v,
                            Err(_) => {
                                warnings.push(format!(
                                    "{{transpose}} value {raw:?} cannot be \
                                     parsed as i8, ignored (using 0)"
                                ));
                                0
                            }
                        },
                    };
                    let (combined, saturated) =
                        chordsketch_core::transpose::combine_transpose(file_offset, cli_transpose);
                    if saturated {
                        warnings.push(format!(
                            "transpose offset {file_offset} + {cli_transpose} \
                             exceeds i8 range, clamped to {combined}"
                        ));
                    }
                    transpose_offset = combined;
                    continue;
                }
                match &directive.kind {
                    DirectiveKind::StartOfChorus => {
                        render_section_header("Chorus", &directive.value, &mut output);
                        // Begin collecting chorus content lines.
                        chorus_buf = Some(Vec::new());
                    }
                    DirectiveKind::EndOfChorus => {
                        if let Some(buf) = chorus_buf.take() {
                            chorus_body = buf;
                        }
                    }
                    DirectiveKind::Chorus => {
                        if chorus_recall_count < MAX_CHORUS_RECALLS {
                            render_chorus_recall(
                                &directive.value,
                                &chorus_body,
                                transpose_offset,
                                &mut output,
                            );
                            chorus_recall_count += 1;
                        } else if chorus_recall_count == MAX_CHORUS_RECALLS {
                            warnings.push(format!(
                                "chorus recall limit ({MAX_CHORUS_RECALLS}) reached, \
                                 further recalls suppressed"
                            ));
                            chorus_recall_count += 1;
                        }
                    }
                    // All page-layout directives are intentionally excluded from the
                    // chorus buffer — they must not be replayed on chorus recall.
                    // Plain-text rendering produces no output for these directives
                    // (parity with HTML and PDF renderers which do the same exclusion).
                    DirectiveKind::NewPage
                    | DirectiveKind::NewPhysicalPage
                    | DirectiveKind::ColumnBreak
                    | DirectiveKind::Columns => {}
                    _ => {
                        if let Some(buf) = chorus_buf.as_mut() {
                            buf.push(line.clone());
                        }
                        let mut target = Vec::new();
                        render_directive(directive, &mut target);
                        output.extend(target);
                    }
                }
            }
            Line::Comment(style, text) => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                render_comment(*style, text, &mut output);
            }
            Line::Empty => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                output.push(String::new());
            }
        }
    }

    // Auto-inject ASCII diagram block when {diagrams} (or {diagrams: guitar/ukulele/piano}) was seen.
    if let Some(ref instrument) = auto_diagrams_instrument {
        if instrument == "piano" {
            // Plain-text rendering of keyboard diagrams is not supported; emit a note
            // listing the chord names so the reader knows which chords are in use.
            let kbd_defines = song.keyboard_defines();
            let voicings: Vec<_> = song
                .used_chord_names()
                .into_iter()
                .filter_map(|name| chordsketch_core::lookup_keyboard_voicing(&name, &kbd_defines))
                .collect();
            if !voicings.is_empty() {
                output.push(String::new());
                output.push("[Chord Diagrams]".to_string());
                for voicing in &voicings {
                    output.push(format!(
                        "  {}: keys {}",
                        voicing.title(),
                        voicing
                            .keys
                            .iter()
                            .map(|k| k.to_string())
                            .collect::<Vec<_>>()
                            .join(" ")
                    ));
                }
            }
        } else {
            let frets_shown = _config
                .get_path("diagrams.frets")
                .as_f64()
                .map_or(chordsketch_core::chord_diagram::DEFAULT_FRETS_SHOWN, |n| {
                    (n as usize).max(1)
                });
            let defines = song.fretted_defines();
            let diagrams: Vec<_> = song
                .used_chord_names()
                .into_iter()
                .filter_map(|name| {
                    chordsketch_core::lookup_diagram(&name, &defines, instrument, frets_shown)
                })
                .collect();
            if !diagrams.is_empty() {
                output.push(String::new());
                output.push("[Chord Diagrams]".to_string());
                for diagram in &diagrams {
                    output.push(String::new());
                    for diagram_line in
                        chordsketch_core::chord_diagram::render_ascii(diagram).lines()
                    {
                        output.push(diagram_line.to_string());
                    }
                }
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
///
/// Warnings are printed to stderr via `eprintln!`. Use
/// [`render_songs_with_warnings`] to capture them programmatically.
#[must_use]
pub fn render_songs_with_transpose(songs: &[Song], cli_transpose: i8, config: &Config) -> String {
    let result = render_songs_with_warnings(songs, cli_transpose, config);
    for w in &result.warnings {
        eprintln!("warning: {w}");
    }
    result.output
}

/// Render multiple [`Song`]s to plain text, returning warnings programmatically.
///
/// This is the structured variant of [`render_songs_with_transpose`]. Instead
/// of printing warnings to stderr, they are collected into
/// [`RenderResult::warnings`].
#[must_use = "caller must check warnings in the returned RenderResult"]
pub fn render_songs_with_warnings(
    songs: &[Song],
    cli_transpose: i8,
    config: &Config,
) -> RenderResult<String> {
    let mut warnings = Vec::new();
    let mut parts: Vec<String> = songs
        .iter()
        .map(|song| {
            render_song_impl(song, cli_transpose, config, &mut warnings)
                .trim_end()
                .to_string()
        })
        .collect();
    // Ensure the final output ends with a newline.
    if let Some(last) = parts.last_mut() {
        last.push('\n');
    }
    RenderResult::with_warnings(parts.join("\n\n"), warnings)
}

/// Parse a ChordPro source string and render it to plain text.
///
/// Returns `Ok(text)` on success, or the [`chordsketch_core::ParseError`] if
/// the input cannot be parsed.
///
/// For pre-parsed input, use [`render_song`] directly.
#[must_use = "parse errors should be handled"]
pub fn try_render(input: &str) -> Result<String, chordsketch_core::ParseError> {
    let song = chordsketch_core::parse(input)?;
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
fn render_metadata(metadata: &chordsketch_core::ast::Metadata, output: &mut Vec<String>) {
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
/// Alignment is based on Unicode display width (`UnicodeWidthStr::width()`),
/// which correctly handles full-width CJK characters and other wide glyphs.
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

        let chord_len = UnicodeWidthStr::width(chord_name);
        let text_len = UnicodeWidthStr::width(text.as_str());

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
/// - Page-layout directives (`{new_page}`, `{new_physical_page}`, `{column_break}`,
///   `{columns}`) produce no output — plain text has no concept of pages or columns.
/// - Unknown directives are silently ignored.
fn render_directive(directive: &chordsketch_core::ast::Directive, output: &mut Vec<String>) {
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
        DirectiveKind::StartOfMusicxml => {
            render_section_header("MusicXML", &directive.value, output);
        }
        DirectiveKind::StartOfSection(section_name) => {
            // Capitalize the first letter of the section name for display.
            let label = chordsketch_core::capitalize(section_name);
            render_section_header(&label, &directive.value, output);
        }
        DirectiveKind::Image(attrs) if attrs.has_src() => {
            output.push(format!("[Image: {}]", attrs.src));
        }
        DirectiveKind::Image(_) => {}
        // Page-layout directives are intentionally no-ops in plain-text output:
        // plain text has no concept of pages, columns, or column breaks.
        // Explicit arms here make the omission visible to future contributors
        // (renderer-parity.md requires every directive to have an explicit arm).
        DirectiveKind::NewPage
        | DirectiveKind::NewPhysicalPage
        | DirectiveKind::ColumnBreak
        | DirectiveKind::Columns => {}
        // End-of-section, metadata, and unknown directives produce no output.
        _ => {}
    }
}

/// Render a `{chorus}` recall directive.
///
/// Re-renders the stored chorus AST lines with the current transpose offset,
/// so chords are transposed correctly even if `{transpose}` changed after
/// the chorus was defined.
fn render_chorus_recall(
    value: &Option<String>,
    chorus_body: &[Line],
    transpose_offset: i8,
    output: &mut Vec<String>,
) {
    render_section_header("Chorus", value, output);
    for line in chorus_body {
        match line {
            Line::Lyrics(lyrics) => render_lyrics(lyrics, transpose_offset, output),
            Line::Comment(style, text) => render_comment(*style, text, output),
            Line::Empty => output.push(String::new()),
            Line::Directive(d) if !d.kind.is_metadata() => render_directive(d, output),
            _ => {}
        }
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
        let song = chordsketch_core::parse("{title: Test}\n[Am]Hello").unwrap();
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
        // Japanese text: each char is 3 bytes, 1 code point, but 2 columns wide.
        let input = "[Am]こんにちは [G]世界";
        let output = render(input);
        // "こんにちは " = 5×2 + 1 = 11 display columns, "Am" = 2 → pad chord by 9
        // "世界" = 2×2 = 4 display columns, "G" = 1 → pad chord by 3
        assert_eq!(output, "Am         G\nこんにちは 世界\n");
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
        let songs = chordsketch_core::parse_multi(
            "{title: Song One}\n[Am]Hello\n{new_song}\n{title: Song Two}\n[G]World",
        )
        .unwrap();
        let output = render_songs(&songs);
        assert!(output.contains("Song One"));
        assert!(output.contains("Am"));
        assert!(output.contains("Hello"));
        assert!(output.contains("Song Two"));
        assert!(output.contains("G\nWorld"));
        // Two songs are separated by exactly one blank line (double newline).
        assert!(output.contains("\n\n"));
        // Must NOT contain triple newline.
        assert!(
            !output.contains("\n\n\n"),
            "Should not have triple newline between songs"
        );
    }

    #[test]
    fn test_render_songs_single_song() {
        let songs = chordsketch_core::parse_multi("{title: Only One}\nLyrics").unwrap();
        let output = render_songs(&songs);
        assert_eq!(output, render_song(&songs[0]));
    }

    #[test]
    fn test_render_songs_with_transpose() {
        let songs =
            chordsketch_core::parse_multi("{title: S1}\n[C]Do\n{new_song}\n{title: S2}\n[G]Re")
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
        let song = chordsketch_core::parse(input).unwrap();
        let output = render_song(&song);
        // G+2=A, C+2=D
        assert_eq!(output, "A     D\nHello world\n");
    }

    #[test]
    fn test_transpose_directive_down_3() {
        let input = "{transpose: -3}\n[Am]Hello [Em]world";
        let song = chordsketch_core::parse(input).unwrap();
        let output = render_song(&song);
        // Am-3=F#m, Em-3=C#m
        assert_eq!(output, "F#m   C#m\nHello world\n");
    }

    #[test]
    fn test_transpose_directive_replaces_previous() {
        let input = "{transpose: 2}\n[G]First\n{transpose: -1}\n[G]Second";
        let song = chordsketch_core::parse(input).unwrap();
        let output = render_song(&song);
        // First line: G+2=A, Second line: G-1=F#
        assert!(output.contains("A\nFirst"));
        assert!(output.contains("F#\nSecond"));
    }

    #[test]
    fn test_transpose_directive_zero_resets() {
        let input = "{transpose: 5}\n[C]Up\n{transpose: 0}\n[C]Normal";
        let song = chordsketch_core::parse(input).unwrap();
        let output = render_song(&song);
        // First line: C+5=F, Second line: C+0=C
        assert!(output.contains("F\nUp"));
        assert!(output.contains("C\nNormal"));
    }

    #[test]
    fn test_transpose_directive_with_cli_offset() {
        let input = "{transpose: 2}\n[C]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let output = render_song_with_transpose(&song, 3, &Config::defaults());
        // In-file 2 + CLI 3 = 5 total. C+5=F
        assert!(output.contains("F\nHello"));
    }

    #[test]
    fn test_cli_transpose_without_directive() {
        let input = "[G]Hello [C]world";
        let song = chordsketch_core::parse(input).unwrap();
        let output = render_song_with_transpose(&song, 2, &Config::defaults());
        // CLI offset only: G+2=A, C+2=D
        assert_eq!(output, "A     D\nHello world\n");
    }

    #[test]
    fn test_transpose_directive_replaces_with_cli_additive() {
        let input = "{transpose: 2}\n[C]First\n{transpose: -1}\n[C]Second";
        let song = chordsketch_core::parse(input).unwrap();
        let output = render_song_with_transpose(&song, 1, &Config::defaults());
        // First: 2+1=3, C+3=D#. Second: -1+1=0, C+0=C
        assert!(output.contains("D#\nFirst"));
        assert!(output.contains("C\nSecond"));
    }

    #[test]
    fn test_transpose_no_chord_lyrics_unaffected() {
        let input = "{transpose: 5}\nPlain lyrics no chords";
        let song = chordsketch_core::parse(input).unwrap();
        let output = render_song(&song);
        assert_eq!(output, "Plain lyrics no chords\n");
    }

    #[test]
    fn test_transpose_invalid_value_treated_as_zero() {
        let input = "{transpose: abc}\n[G]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let result =
            render_song_with_warnings(&song, 0, &chordsketch_core::config::Config::defaults());
        // Invalid value -> treated as 0
        assert!(result.output.contains("G\nHello"));
        assert!(
            result.warnings.iter().any(|w| w.contains("\"abc\"")),
            "expected warning about unparseable value, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_transpose_out_of_i8_range_emits_warning() {
        // 999 cannot be represented as i8; should fall back to 0 with a warning
        let input = "{transpose: 999}\n[G]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let result =
            render_song_with_warnings(&song, 0, &chordsketch_core::config::Config::defaults());
        assert!(
            result.output.contains("G\nHello"),
            "chord should be untransposed"
        );
        assert!(
            result.warnings.iter().any(|w| w.contains("\"999\"")),
            "expected warning about out-of-range value, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_transpose_no_value_treated_as_zero() {
        let input = "{transpose}\n[G]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let result =
            render_song_with_warnings(&song, 0, &chordsketch_core::config::Config::defaults());
        // No value -> silently treated as 0, no warning emitted.
        assert!(result.output.contains("G\nHello"));
        assert!(
            result.warnings.is_empty(),
            "missing {{transpose}} value should not emit a warning; got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_transpose_whitespace_value_treated_as_zero() {
        // {transpose:   } with whitespace-only value should silently reset to 0,
        // no warning emitted. The parser trims whitespace → Some(""), which the
        // Some("") arm converts to 0.
        let input = "{transpose:   }\n[G]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let result =
            render_song_with_warnings(&song, 0, &chordsketch_core::config::Config::defaults());
        assert!(
            result.output.contains("G\nHello"),
            "chord should be untransposed"
        );
        assert!(
            result.warnings.is_empty(),
            "whitespace-only {{transpose}} value should not emit a warning; got: {:?}",
            result.warnings
        );
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

    #[test]
    fn test_chorus_recall_applies_current_transpose() {
        // Chorus defined with no transpose, recalled after {transpose: 2}.
        // G should become A in the recalled chorus.
        let input = "\
{start_of_chorus}
[G]La la
{end_of_chorus}
{transpose: 2}
{chorus}";
        let output = render(input);
        // Original chorus has [G], recalled chorus should have [A].
        // The recalled chorus should show "A" (G+2) not "G".
        // The output has chord on one line and lyrics on next.
        let recall_idx = output.rfind("[Chorus]").expect("should have recall");
        let recall_section = &output[recall_idx..];
        assert!(
            recall_section.contains('A') && !recall_section.contains('G'),
            "recalled chorus should have transposed chord A (not G), got:\n{recall_section}"
        );
    }

    #[test]
    fn test_chorus_recall_limit_exceeded() {
        // Generate input with more chorus recalls than the limit.
        let mut input = String::from("{start_of_chorus}\nChorus\n{end_of_chorus}\n");
        for _ in 0..1005 {
            input.push_str("{chorus}\n");
        }
        let output = render(&input);
        // Count occurrences of the chorus content (excluding the original).
        let recall_count = output.matches("[Chorus]\nChorus").count() - 1; // subtract original
        assert_eq!(
            recall_count,
            super::MAX_CHORUS_RECALLS,
            "should stop at MAX_CHORUS_RECALLS"
        );
    }

    #[test]
    fn test_page_control_not_replayed_in_chorus_recall() {
        // Page-control directives inside a chorus definition must NOT appear in
        // {chorus} recall output. This mirrors the equivalent test in the HTML and
        // PDF renderers (renderer-parity.md).
        let input = "\
{start_of_chorus}
[G]Chorus line
{new_page}
{column_break}
{columns: 2}
{end_of_chorus}
{chorus}";
        let output = render(input);
        // The recalled chorus must contain the lyric content …
        assert!(
            output.contains("G"),
            "chord from chorus must appear in recall: {output}"
        );
        assert!(
            output.contains("Chorus line"),
            "lyric from chorus must appear in recall: {output}"
        );
        // … but must NOT contain any page-control directive text.
        // (Page-control directives produce no text output in the text renderer,
        //  so if they were erroneously replayed, the output would be unchanged;
        //  the key assertion is that the chorus body stored during collection
        //  does not include the directive AST nodes and cause extra empty lines.)
        let chorus_section_lines: Vec<&str> = output.lines().collect();
        // The definition renders: [Chorus] header + chord line + lyric line.
        // The recall renders: [Chorus] header + chord line + lyric line again.
        // Total: 6 non-empty lines (3 original + 3 recall). No extra blank lines
        // introduced by replayed page-control directives.
        let non_empty_count = chorus_section_lines
            .iter()
            .filter(|l| !l.is_empty())
            .count();
        assert_eq!(
            non_empty_count, 6,
            "expected 6 non-empty output lines (3 original + 3 recall), got {non_empty_count}; \
             output:\n{output}"
        );
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
    fn test_render_musicxml_section() {
        let input = "{start_of_musicxml}\n<score-partwise/>\n{end_of_musicxml}";
        let output = render(input);
        assert!(output.contains("[MusicXML]"));
    }

    #[test]
    fn test_render_musicxml_section_with_label() {
        let input = "{start_of_musicxml: Score}\n<score-partwise/>\n{end_of_musicxml}";
        let output = render(input);
        assert!(output.contains("[MusicXML: Score]"));
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

    // --- Unicode display width alignment ---

    #[test]
    fn test_render_fullwidth_cjk_alignment() {
        // Full-width CJK characters are 2 columns wide
        let input = "[C]日本語";
        let output = render(input);
        // "日本語" = 3×2 = 6 display columns, "C" = 1 → pad chord by 5
        assert_eq!(output, "C\n日本語\n");
    }

    #[test]
    fn test_render_mixed_width_alignment() {
        // Mix of ASCII (width 1) and CJK (width 2)
        let input = "[Am]hello世界 [G]test";
        let output = render(input);
        // "hello世界 " = 5 + 4 + 1 = 10, "Am" = 2 → pad chord by 8
        // "test" = 4, "G" = 1 → pad chord by 3
        assert_eq!(output, "Am        G\nhello世界 test\n");
    }

    #[test]
    fn test_render_image_placeholder() {
        let input = "{image: src=photo.jpg}";
        let output = render(input);
        assert!(output.contains("[Image: photo.jpg]"));
    }

    #[test]
    fn test_render_image_placeholder_with_path() {
        let input = "{image: src=images/cover.png width=200}";
        let output = render(input);
        assert!(output.contains("[Image: images/cover.png]"));
    }

    #[test]
    fn test_render_image_empty_src_suppressed() {
        let input = "{image}";
        let output = render(input);
        assert!(!output.contains("[Image"));
    }

    #[test]
    fn test_render_image_empty_src_with_other_attrs_suppressed() {
        let input = "{image: width=200 height=100}";
        let output = render(input);
        assert!(!output.contains("[Image"));
    }

    // -- Selector filtering integration (#320) --

    #[test]
    fn test_selector_filtering_removes_non_matching_directive() {
        let input = "{title: Song}\n{textfont-piano: Courier}\n[Am]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let ctx = chordsketch_core::selector::SelectorContext::new(Some("guitar"), None);
        let filtered = ctx.filter_song(&song);
        // The piano textfont directive should be absent from the filtered song.
        let has_textfont = filtered.lines.iter().any(|l| {
            matches!(l, chordsketch_core::ast::Line::Directive(d) if d.kind == chordsketch_core::ast::DirectiveKind::TextFont)
        });
        assert!(
            !has_textfont,
            "piano textfont directive should be removed for guitar context"
        );
        let output = render_song(&filtered);
        assert!(output.contains("Hello"), "lyrics should survive filtering");
    }

    #[test]
    fn test_selector_filtering_removes_section_with_contents() {
        let input = "{title: Song}\n{start_of_chorus-piano}\n[C]Piano only\n{end_of_chorus-piano}\n[Am]Guitar verse";
        let song = chordsketch_core::parse(input).unwrap();
        let ctx = chordsketch_core::selector::SelectorContext::new(Some("guitar"), None);
        let filtered = ctx.filter_song(&song);
        let output = render_song(&filtered);
        assert!(
            !output.contains("Piano only"),
            "piano chorus should be removed for guitar context"
        );
        assert!(
            output.contains("Guitar verse"),
            "unselectored content should remain"
        );
    }

    // -- auto-inject ASCII diagram block (issue #1140) ----------------------------

    #[test]
    fn test_diagrams_auto_inject_text() {
        let output = render("{diagrams}\n[Am]Hello");
        assert!(
            output.contains("[Chord Diagrams]"),
            "text output should include Chord Diagrams header"
        );
        assert!(output.contains("Am"), "Am ASCII diagram expected");
        // Am = x o 2 2 1 o (guitar open position)
        assert!(output.contains("x o"), "Am fret pattern expected");
    }

    #[test]
    fn test_no_diagrams_suppresses_text_inject() {
        let output = render("{no_diagrams}\n[Am]Hello");
        assert!(
            !output.contains("[Chord Diagrams]"),
            "{{no_diagrams}} should suppress ASCII diagram block"
        );
    }

    #[test]
    fn test_diagrams_off_suppresses_text_inject() {
        let output = render("{diagrams: off}\n[Am]Hello");
        assert!(
            !output.contains("[Chord Diagrams]"),
            "{{diagrams: off}} should suppress ASCII diagram block"
        );
    }

    #[test]
    fn test_diagrams_piano_auto_inject_text() {
        let output = render("{diagrams: piano}\n[Am]Hello [C]world");
        assert!(
            output.contains("[Chord Diagrams]"),
            "piano instrument should include Chord Diagrams header"
        );
        // Text renderer emits chord name + key numbers (no ASCII art for piano).
        assert!(output.contains("Am:"), "Am entry expected");
        assert!(output.contains("C:"), "C entry expected");
        assert!(output.contains("keys"), "key list label expected");
    }

    #[test]
    fn test_render_decomposed_diacritics_alignment() {
        // "e\u{0301}" is a decomposed e-acute (U+0065 + U+0301 combining acute).
        // The combining character has zero display width, so the word "cafe\u{0301}"
        // should have the same display width (4 columns) as "cafe".
        // This should produce identical alignment to composed "café".
        let input = "[Em]cafe\u{0301} [D]world";
        let output = render(input);
        // "cafe\u{0301} " = 5 display columns (4 + 0 + 1), "Em" = 2 → pad chord by 3
        // Matches composed form: "[Em]café [D]résumé" → "Em   D"
        assert_eq!(output, "Em   D\ncafe\u{0301} world\n");
    }
}
