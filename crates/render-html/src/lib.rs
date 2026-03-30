//! HTML renderer for ChordPro documents.
//!
//! Converts a parsed ChordPro AST into a self-contained HTML5 document with
//! embedded CSS for chord-over-lyrics layout.

use chordpro_core::ast::{CommentStyle, DirectiveKind, Line, LyricsLine, Song};
use chordpro_core::transpose::transpose_chord;

/// Render a [`Song`] AST to an HTML5 document string.
///
/// The output is a complete `<!DOCTYPE html>` document with embedded CSS
/// that positions chords above their corresponding lyrics.
#[must_use]
pub fn render_song(song: &Song) -> String {
    render_song_with_transpose(song, 0)
}

/// Render a [`Song`] AST to an HTML5 document with an additional CLI transposition offset.
///
/// The `cli_transpose` parameter is added to any in-file `{transpose}` directive
/// values, allowing the CLI `--transpose` flag to combine with in-file directives.
#[must_use]
pub fn render_song_with_transpose(song: &Song, cli_transpose: i8) -> String {
    let mut html = String::new();
    let mut transpose_offset: i8 = cli_transpose;

    let title = song.metadata.title.as_deref().unwrap_or("Untitled");
    html.push_str(&format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n",
        escape(title)
    ));
    html.push_str("<style>\n");
    html.push_str(CSS);
    html.push_str("</style>\n</head>\n<body>\n<div class=\"song\">\n");

    render_metadata(&song.metadata, &mut html);

    for line in &song.lines {
        match line {
            Line::Lyrics(lyrics_line) => render_lyrics(lyrics_line, transpose_offset, &mut html),
            Line::Directive(directive) => {
                if directive.kind == DirectiveKind::Transpose {
                    let file_offset: i8 = directive
                        .value
                        .as_deref()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    transpose_offset = file_offset.saturating_add(cli_transpose);
                } else if !directive.kind.is_metadata() {
                    render_directive(directive, &mut html);
                }
            }
            Line::Comment(style, text) => render_comment(*style, text, &mut html),
            Line::Empty => html.push_str("<div class=\"empty-line\"></div>\n"),
        }
    }

    html.push_str("</div>\n</body>\n</html>\n");
    html
}

/// Parse a ChordPro source string and render it to HTML.
///
/// Returns `Ok(html)` on success, or the [`chordpro_core::ParseError`] if
/// the input cannot be parsed.
pub fn try_render(input: &str) -> Result<String, chordpro_core::ParseError> {
    let song = chordpro_core::parse(input)?;
    Ok(render_song(&song))
}

/// Parse a ChordPro source string and render it to HTML.
///
/// Convenience wrapper that converts parse errors to a string.
/// Use [`try_render`] if you need error handling.
#[must_use]
pub fn render(input: &str) -> String {
    match try_render(input) {
        Ok(html) => html,
        Err(e) => format!(
            "<!DOCTYPE html><html><body><pre>Parse error at line {} column {}: {}</pre></body></html>\n",
            e.line(),
            e.column(),
            escape(&e.message)
        ),
    }
}

// ---------------------------------------------------------------------------
// CSS
// ---------------------------------------------------------------------------

/// Embedded CSS for chord-over-lyrics layout.
const CSS: &str = "\
body { font-family: serif; max-width: 800px; margin: 2em auto; padding: 0 1em; }
h1 { margin-bottom: 0.2em; }
h2 { margin-top: 0; font-weight: normal; color: #555; }
.line { display: flex; flex-wrap: wrap; margin: 0.1em 0; }
.chord-block { display: inline-flex; flex-direction: column; align-items: flex-start; }
.chord { font-weight: bold; color: #b00; font-size: 0.9em; min-height: 1.2em; }
.lyrics { white-space: pre; }
.empty-line { height: 1em; }
section { margin: 1em 0; }
section > .section-label { font-weight: bold; font-style: italic; margin-bottom: 0.3em; }
.comment { font-style: italic; color: #666; margin: 0.3em 0; }
.comment-box { border: 1px solid #999; padding: 0.2em 0.5em; display: inline-block; margin: 0.3em 0; }
";

// ---------------------------------------------------------------------------
// Escape
// ---------------------------------------------------------------------------

/// Escape HTML special characters.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

/// Render song metadata (title, subtitle) as HTML header elements.
fn render_metadata(metadata: &chordpro_core::ast::Metadata, html: &mut String) {
    if let Some(title) = &metadata.title {
        html.push_str(&format!("<h1>{}</h1>\n", escape(title)));
    }
    for subtitle in &metadata.subtitles {
        html.push_str(&format!("<h2>{}</h2>\n", escape(subtitle)));
    }
}

// ---------------------------------------------------------------------------
// Lyrics (chord-over-lyrics layout)
// ---------------------------------------------------------------------------

/// Render a lyrics line with chord-over-lyrics layout.
///
/// Each chord+text pair is wrapped in a `<span class="chord-block">` with
/// the chord in `<span class="chord">` and the text in `<span class="lyrics">`.
fn render_lyrics(lyrics_line: &LyricsLine, transpose_offset: i8, html: &mut String) {
    html.push_str("<div class=\"line\">");

    for segment in &lyrics_line.segments {
        html.push_str("<span class=\"chord-block\">");

        if let Some(chord) = &segment.chord {
            let display_name = if transpose_offset != 0 {
                let transposed = transpose_chord(chord, transpose_offset);
                transposed.name
            } else {
                chord.name.clone()
            };
            html.push_str(&format!(
                "<span class=\"chord\">{}</span>",
                escape(&display_name)
            ));
        } else if lyrics_line.has_chords() {
            // Empty chord placeholder to maintain vertical alignment.
            html.push_str("<span class=\"chord\"></span>");
        }

        html.push_str(&format!(
            "<span class=\"lyrics\">{}</span>",
            escape(&segment.text)
        ));
        html.push_str("</span>");
    }

    html.push_str("</div>\n");
}

// ---------------------------------------------------------------------------
// Directives
// ---------------------------------------------------------------------------

/// Render a directive as HTML.
fn render_directive(directive: &chordpro_core::ast::Directive, html: &mut String) {
    match &directive.kind {
        DirectiveKind::StartOfChorus => {
            render_section_open("chorus", "Chorus", &directive.value, html);
        }
        DirectiveKind::StartOfVerse => {
            render_section_open("verse", "Verse", &directive.value, html);
        }
        DirectiveKind::StartOfBridge => {
            render_section_open("bridge", "Bridge", &directive.value, html);
        }
        DirectiveKind::StartOfTab => {
            render_section_open("tab", "Tab", &directive.value, html);
        }
        DirectiveKind::EndOfChorus
        | DirectiveKind::EndOfVerse
        | DirectiveKind::EndOfBridge
        | DirectiveKind::EndOfTab => {
            html.push_str("</section>\n");
        }
        _ => {}
    }
}

/// Open a `<section>` with a class and optional label.
fn render_section_open(class: &str, label: &str, value: &Option<String>, html: &mut String) {
    html.push_str(&format!("<section class=\"{class}\">\n"));
    let display_label = match value {
        Some(v) if !v.is_empty() => format!("{label}: {}", escape(v)),
        _ => label.to_string(),
    };
    html.push_str(&format!(
        "<div class=\"section-label\">{display_label}</div>\n"
    ));
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

/// Render a comment as HTML.
fn render_comment(style: CommentStyle, text: &str, html: &mut String) {
    match style {
        CommentStyle::Normal => {
            html.push_str(&format!("<p class=\"comment\">{}</p>\n", escape(text)));
        }
        CommentStyle::Italic => {
            html.push_str(&format!(
                "<p class=\"comment\"><em>{}</em></p>\n",
                escape(text)
            ));
        }
        CommentStyle::Boxed => {
            html.push_str(&format!(
                "<div class=\"comment-box\">{}</div>\n",
                escape(text)
            ));
        }
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
        let song = chordpro_core::parse("").unwrap();
        let html = render_song(&song);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn test_render_title() {
        let html = render("{title: My Song}");
        assert!(html.contains("<h1>My Song</h1>"));
        assert!(html.contains("<title>My Song</title>"));
    }

    #[test]
    fn test_render_subtitle() {
        let html = render("{title: Song}\n{subtitle: By Someone}");
        assert!(html.contains("<h2>By Someone</h2>"));
    }

    #[test]
    fn test_render_lyrics_with_chords() {
        let html = render("[Am]Hello [G]world");
        assert!(html.contains("chord-block"));
        assert!(html.contains("<span class=\"chord\">Am</span>"));
        assert!(html.contains("<span class=\"lyrics\">Hello </span>"));
        assert!(html.contains("<span class=\"chord\">G</span>"));
    }

    #[test]
    fn test_render_lyrics_no_chords() {
        let html = render("Just plain text");
        assert!(html.contains("<span class=\"lyrics\">Just plain text</span>"));
        // Should NOT have chord spans when no chords are present
        assert!(!html.contains("class=\"chord\""));
    }

    #[test]
    fn test_render_chorus_section() {
        let html = render("{start_of_chorus}\n[G]La la\n{end_of_chorus}");
        assert!(html.contains("<section class=\"chorus\">"));
        assert!(html.contains("</section>"));
        assert!(html.contains("Chorus"));
    }

    #[test]
    fn test_render_verse_with_label() {
        let html = render("{start_of_verse: Verse 1}\nLyrics\n{end_of_verse}");
        assert!(html.contains("<section class=\"verse\">"));
        assert!(html.contains("Verse: Verse 1"));
    }

    #[test]
    fn test_render_comment() {
        let html = render("{comment: A note}");
        assert!(html.contains("<p class=\"comment\">A note</p>"));
    }

    #[test]
    fn test_render_comment_italic() {
        let html = render("{comment_italic: Softly}");
        assert!(html.contains("<em>Softly</em>"));
    }

    #[test]
    fn test_render_comment_box() {
        let html = render("{comment_box: Important}");
        assert!(html.contains("<div class=\"comment-box\">Important</div>"));
    }

    #[test]
    fn test_html_escaping() {
        let html = render("{title: Tom & Jerry <3}");
        assert!(html.contains("Tom &amp; Jerry &lt;3"));
    }

    #[test]
    fn test_try_render_success() {
        let result = try_render("{title: Test}");
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_render_error() {
        let result = try_render("{unclosed");
        assert!(result.is_err());
    }

    #[test]
    fn test_render_valid_html_structure() {
        let html = render("{title: Test}\n\n{start_of_verse}\n[G]Hello [C]world\n{end_of_verse}");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("<html"));
        assert!(html.contains("<head>"));
        assert!(html.contains("<style>"));
        assert!(html.contains("<body>"));
        assert!(html.contains("</html>"));
    }

    #[test]
    fn test_text_before_first_chord() {
        let html = render("Hello [Am]world");
        // Should have empty chord placeholder for the "Hello " segment
        assert!(html.contains("<span class=\"chord\"></span><span class=\"lyrics\">Hello </span>"));
    }

    #[test]
    fn test_empty_line() {
        let html = render("Line one\n\nLine two");
        assert!(html.contains("empty-line"));
    }
}

#[cfg(test)]
mod transpose_tests {
    use super::*;

    #[test]
    fn test_transpose_directive_up_2() {
        let input = "{transpose: 2}\n[G]Hello [C]world";
        let song = chordpro_core::parse(input).unwrap();
        let html = render_song(&song);
        // G+2=A, C+2=D
        assert!(html.contains("<span class=\"chord\">A</span>"));
        assert!(html.contains("<span class=\"chord\">D</span>"));
        assert!(!html.contains("<span class=\"chord\">G</span>"));
        assert!(!html.contains("<span class=\"chord\">C</span>"));
    }

    #[test]
    fn test_transpose_directive_replaces_previous() {
        let input = "{transpose: 2}\n[G]First\n{transpose: 0}\n[G]Second";
        let song = chordpro_core::parse(input).unwrap();
        let html = render_song(&song);
        // First G transposed +2 = A, second G at 0 = G
        assert!(html.contains("<span class=\"chord\">A</span>"));
        assert!(html.contains("<span class=\"chord\">G</span>"));
    }

    #[test]
    fn test_transpose_directive_with_cli_offset() {
        let input = "{transpose: 2}\n[C]Hello";
        let song = chordpro_core::parse(input).unwrap();
        let html = render_song_with_transpose(&song, 3);
        // 2 + 3 = 5, C+5=F
        assert!(html.contains("<span class=\"chord\">F</span>"));
    }
}
