//! HTML renderer for ChordPro documents.
//!
//! Converts a parsed ChordPro AST into a self-contained HTML5 document with
//! embedded CSS for chord-over-lyrics layout.

use chordpro_core::ast::{CommentStyle, DirectiveKind, Line, LyricsLine, Song};
use chordpro_core::inline_markup::{SpanAttributes, TextSpan};
use chordpro_core::transpose::transpose_chord;

/// Render a [`Song`] AST to an HTML5 document string.
///
/// The output is a complete `<!DOCTYPE html>` document with embedded CSS
/// that positions chords above their corresponding lyrics.
///
/// The `{chorus}` directive recalls the most recently defined chorus section.
/// Recalled chorus content is wrapped in `<div class="chorus-recall">` and
/// includes the full chorus body.
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

    // Stores the rendered HTML of the most recently defined chorus body
    // (everything between StartOfChorus and EndOfChorus, excluding the
    // section open/close tags). Used by `{chorus}` recall.
    let mut chorus_html = String::new();
    // Temporary buffer for collecting chorus content while inside a chorus section.
    let mut chorus_buf: Option<String> = None;

    for line in &song.lines {
        match line {
            Line::Lyrics(lyrics_line) => {
                let mut target = String::new();
                render_lyrics(lyrics_line, transpose_offset, &mut target);
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push_str(&target);
                }
                html.push_str(&target);
            }
            Line::Directive(directive) => {
                if directive.kind.is_metadata() {
                    continue;
                }
                if directive.kind == DirectiveKind::Transpose {
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
                        render_section_open("chorus", "Chorus", &directive.value, &mut html);
                        // Begin collecting chorus content.
                        chorus_buf = Some(String::new());
                    }
                    DirectiveKind::EndOfChorus => {
                        html.push_str("</section>\n");
                        // Finish collecting: store the buffered HTML as the
                        // most recent chorus for future recall.
                        if let Some(buf) = chorus_buf.take() {
                            chorus_html = buf;
                        }
                    }
                    DirectiveKind::Chorus => {
                        render_chorus_recall(&directive.value, &chorus_html, &mut html);
                    }
                    _ => {
                        let mut target = String::new();
                        render_directive_inner(directive, &mut target);
                        if let Some(buf) = chorus_buf.as_mut() {
                            buf.push_str(&target);
                        }
                        html.push_str(&target);
                    }
                }
            }
            Line::Comment(style, text) => {
                let mut target = String::new();
                render_comment(*style, text, &mut target);
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push_str(&target);
                }
                html.push_str(&target);
            }
            Line::Empty => {
                let empty = "<div class=\"empty-line\"></div>\n";
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push_str(empty);
                }
                html.push_str(empty);
            }
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
.chorus-recall { margin: 1em 0; }
.chorus-recall > .section-label { font-weight: bold; font-style: italic; margin-bottom: 0.3em; }
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

        html.push_str("<span class=\"lyrics\">");
        if segment.has_markup() {
            render_spans(&segment.spans, html);
        } else {
            html.push_str(&escape(&segment.text));
        }
        html.push_str("</span>");
        html.push_str("</span>");
    }

    html.push_str("</div>\n");
}

/// Render a list of [`TextSpan`]s as HTML inline elements.
///
/// Maps each markup tag to its HTML equivalent:
/// - `Bold` → `<b>`
/// - `Italic` → `<i>`
/// - `Highlight` → `<mark>`
/// - `Comment` → `<span class="comment">`
/// - `Span` → `<span style="...">` with CSS properties from attributes
fn render_spans(spans: &[TextSpan], html: &mut String) {
    for span in spans {
        match span {
            TextSpan::Plain(text) => html.push_str(&escape(text)),
            TextSpan::Bold(children) => {
                html.push_str("<b>");
                render_spans(children, html);
                html.push_str("</b>");
            }
            TextSpan::Italic(children) => {
                html.push_str("<i>");
                render_spans(children, html);
                html.push_str("</i>");
            }
            TextSpan::Highlight(children) => {
                html.push_str("<mark>");
                render_spans(children, html);
                html.push_str("</mark>");
            }
            TextSpan::Comment(children) => {
                html.push_str("<span class=\"comment\">");
                render_spans(children, html);
                html.push_str("</span>");
            }
            TextSpan::Span(attrs, children) => {
                let css = span_attrs_to_css(attrs);
                if css.is_empty() {
                    html.push_str("<span>");
                } else {
                    html.push_str(&format!("<span style=\"{}\">", escape(&css)));
                }
                render_spans(children, html);
                html.push_str("</span>");
            }
        }
    }
}

/// Convert [`SpanAttributes`] to a CSS inline style string.
fn span_attrs_to_css(attrs: &SpanAttributes) -> String {
    let mut css = String::new();
    if let Some(ref font_family) = attrs.font_family {
        css.push_str(&format!("font-family: {};", font_family));
    }
    if let Some(ref size) = attrs.size {
        // If the size is a plain number, treat it as pt; otherwise pass through.
        if size.chars().all(|c| c.is_ascii_digit()) {
            css.push_str(&format!("font-size: {}pt;", size));
        } else {
            css.push_str(&format!("font-size: {};", size));
        }
    }
    if let Some(ref fg) = attrs.foreground {
        css.push_str(&format!("color: {};", fg));
    }
    if let Some(ref bg) = attrs.background {
        css.push_str(&format!("background-color: {};", bg));
    }
    if let Some(ref weight) = attrs.weight {
        css.push_str(&format!("font-weight: {};", weight));
    }
    if let Some(ref style) = attrs.style {
        css.push_str(&format!("font-style: {};", style));
    }
    css
}

// ---------------------------------------------------------------------------
// Directives
// ---------------------------------------------------------------------------

/// Render a directive as HTML (dispatches to section open/close/other).
///
/// StartOfChorus, EndOfChorus, and Chorus are handled directly in
/// `render_song` for chorus-recall state tracking.
fn render_directive_inner(directive: &chordpro_core::ast::Directive, html: &mut String) {
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
        DirectiveKind::StartOfGrid => {
            render_section_open("grid", "Grid", &directive.value, html);
        }
        DirectiveKind::StartOfAbc => {
            render_section_open("abc", "ABC", &directive.value, html);
        }
        DirectiveKind::StartOfLy => {
            render_section_open("ly", "Lilypond", &directive.value, html);
        }
        DirectiveKind::StartOfSvg => {
            render_section_open("svg", "SVG", &directive.value, html);
        }
        DirectiveKind::StartOfTextblock => {
            render_section_open("textblock", "Textblock", &directive.value, html);
        }
        DirectiveKind::StartOfSection(section_name) => {
            let class = format!("section-{}", section_name);
            let label = capitalize(section_name);
            render_section_open(&class, &label, &directive.value, html);
        }
        DirectiveKind::EndOfChorus
        | DirectiveKind::EndOfVerse
        | DirectiveKind::EndOfBridge
        | DirectiveKind::EndOfTab
        | DirectiveKind::EndOfGrid
        | DirectiveKind::EndOfAbc
        | DirectiveKind::EndOfLy
        | DirectiveKind::EndOfSvg
        | DirectiveKind::EndOfTextblock
        | DirectiveKind::EndOfSection(_) => {
            html.push_str("</section>\n");
        }
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

/// Render a `{chorus}` recall directive as HTML.
///
/// Wraps the recalled chorus content in a `<div class="chorus-recall">` with
/// a section label. If no chorus has been defined yet, only the label is emitted.
fn render_chorus_recall(value: &Option<String>, chorus_html: &str, html: &mut String) {
    html.push_str("<div class=\"chorus-recall\">\n");
    let display_label = match value {
        Some(v) if !v.is_empty() => format!("Chorus: {}", escape(v)),
        _ => "Chorus".to_string(),
    };
    html.push_str(&format!(
        "<div class=\"section-label\">{display_label}</div>\n"
    ));
    html.push_str(chorus_html);
    html.push_str("</div>\n");
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

    #[test]
    fn test_render_grid_section() {
        let html = render("{start_of_grid}\n| Am . | C . |\n{end_of_grid}");
        assert!(html.contains("<section class=\"grid\">"));
        assert!(html.contains("Grid"));
        assert!(html.contains("</section>"));
    }

    // --- Custom sections (#108) ---

    #[test]
    fn test_render_custom_section_intro() {
        let html = render("{start_of_intro}\n[Am]Da da\n{end_of_intro}");
        assert!(html.contains("<section class=\"section-intro\">"));
        assert!(html.contains("Intro"));
        assert!(html.contains("</section>"));
    }

    #[test]
    fn test_render_grid_section_with_label() {
        let html = render("{start_of_grid: Intro}\n| Am |\n{end_of_grid}");
        assert!(html.contains("<section class=\"grid\">"));
        assert!(html.contains("Grid: Intro"));
    }

    #[test]
    fn test_render_grid_short_alias() {
        let html = render("{sog}\n| G . |\n{eog}");
        assert!(html.contains("<section class=\"grid\">"));
        assert!(html.contains("</section>"));
    }

    #[test]
    fn test_render_custom_section_with_label() {
        let html = render("{start_of_intro: Guitar}\nNotes\n{end_of_intro}");
        assert!(html.contains("<section class=\"section-intro\">"));
        assert!(html.contains("Intro: Guitar"));
    }

    #[test]
    fn test_render_custom_section_outro() {
        let html = render("{start_of_outro}\nFinal\n{end_of_outro}");
        assert!(html.contains("<section class=\"section-outro\">"));
        assert!(html.contains("Outro"));
    }

    #[test]
    fn test_render_custom_section_solo() {
        let html = render("{start_of_solo}\n[Em]Solo\n{end_of_solo}");
        assert!(html.contains("<section class=\"section-solo\">"));
        assert!(html.contains("Solo"));
        assert!(html.contains("</section>"));
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

    // --- Issue #109: {chorus} recall ---

    #[test]
    fn test_render_chorus_recall_basic() {
        let html = render("{start_of_chorus}\n[G]La la\n{end_of_chorus}\n\n{chorus}");
        // Should contain chorus-recall div
        assert!(html.contains("<div class=\"chorus-recall\">"));
        // The recalled content should include the chord
        assert!(html.contains("chorus-recall"));
        // Check the original section is still there
        assert!(html.contains("<section class=\"chorus\">"));
    }

    #[test]
    fn test_render_chorus_recall_with_label() {
        let html = render("{start_of_chorus}\nSing\n{end_of_chorus}\n{chorus: Repeat}");
        assert!(html.contains("Chorus: Repeat"));
        assert!(html.contains("chorus-recall"));
    }

    #[test]
    fn test_render_chorus_recall_no_chorus_defined() {
        let html = render("{chorus}");
        // Should still produce a chorus-recall div with just the label
        assert!(html.contains("<div class=\"chorus-recall\">"));
        assert!(html.contains("Chorus"));
    }

    #[test]
    fn test_render_chorus_recall_content_replayed() {
        let html = render("{start_of_chorus}\nChorus text\n{end_of_chorus}\n{chorus}");
        // "Chorus text" should appear twice: once in original, once in recall
        let count = html.matches("Chorus text").count();
        assert_eq!(count, 2, "chorus content should appear twice");
    }

    // -- inline markup rendering tests ----------------------------------------

    #[test]
    fn test_render_bold_markup() {
        let html = render("Hello <b>bold</b> world");
        assert!(html.contains("<b>bold</b>"));
        assert!(html.contains("Hello "));
        assert!(html.contains(" world"));
    }

    #[test]
    fn test_render_italic_markup() {
        let html = render("Hello <i>italic</i> text");
        assert!(html.contains("<i>italic</i>"));
    }

    #[test]
    fn test_render_highlight_markup() {
        let html = render("<highlight>important</highlight>");
        assert!(html.contains("<mark>important</mark>"));
    }

    #[test]
    fn test_render_comment_inline_markup() {
        let html = render("<comment>note</comment>");
        assert!(html.contains("<span class=\"comment\">note</span>"));
    }

    #[test]
    fn test_render_span_with_foreground() {
        let html = render(r#"<span foreground="red">red text</span>"#);
        assert!(html.contains("color: red;"));
        assert!(html.contains("red text"));
    }

    #[test]
    fn test_render_span_with_multiple_attrs() {
        let html = render(
            r#"<span font_family="Serif" size="14" foreground="blue" weight="bold">styled</span>"#,
        );
        assert!(html.contains("font-family: Serif;"));
        assert!(html.contains("font-size: 14pt;"));
        assert!(html.contains("color: blue;"));
        assert!(html.contains("font-weight: bold;"));
        assert!(html.contains("styled"));
    }

    #[test]
    fn test_render_nested_markup() {
        let html = render("<b><i>bold italic</i></b>");
        assert!(html.contains("<b><i>bold italic</i></b>"));
    }

    #[test]
    fn test_render_markup_with_chord() {
        let html = render("[Am]Hello <b>bold</b> world");
        assert!(html.contains("<b>bold</b>"));
        assert!(html.contains("<span class=\"chord\">Am</span>"));
    }

    #[test]
    fn test_render_no_markup_unchanged() {
        let html = render("Just plain text");
        // Should NOT have any inline formatting tags
        assert!(!html.contains("<b>"));
        assert!(!html.contains("<i>"));
        assert!(html.contains("Just plain text"));
    }
}

#[cfg(test)]
mod delegate_tests {
    use super::*;

    #[test]
    fn test_render_abc_section() {
        let html = render("{start_of_abc}\nX:1\n{end_of_abc}");
        assert!(html.contains("<section class=\"abc\">"));
        assert!(html.contains("ABC"));
        assert!(html.contains("</section>"));
    }

    #[test]
    fn test_render_abc_section_with_label() {
        let html = render("{start_of_abc: Melody}\nX:1\n{end_of_abc}");
        assert!(html.contains("<section class=\"abc\">"));
        assert!(html.contains("ABC: Melody"));
    }

    #[test]
    fn test_render_ly_section() {
        let html = render("{start_of_ly}\nnotes\n{end_of_ly}");
        assert!(html.contains("<section class=\"ly\">"));
        assert!(html.contains("Lilypond"));
        assert!(html.contains("</section>"));
    }

    #[test]
    fn test_render_svg_section() {
        let html = render("{start_of_svg}\n<svg/>\n{end_of_svg}");
        assert!(html.contains("<section class=\"svg\">"));
        assert!(html.contains("SVG"));
        assert!(html.contains("</section>"));
    }

    #[test]
    fn test_render_textblock_section() {
        let html = render("{start_of_textblock}\nPreformatted\n{end_of_textblock}");
        assert!(html.contains("<section class=\"textblock\">"));
        assert!(html.contains("Textblock"));
        assert!(html.contains("</section>"));
    }
}
