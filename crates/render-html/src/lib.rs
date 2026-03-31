//! HTML renderer for ChordPro documents.
//!
//! Converts a parsed ChordPro AST into a self-contained HTML5 document with
//! embedded CSS for chord-over-lyrics layout.
//!
//! # Security
//!
//! Delegate section environments (`{start_of_svg}`, `{start_of_abc}`,
//! `{start_of_ly}`, `{start_of_textblock}`) emit their content as raw,
//! unescaped HTML. This is by design per the ChordPro specification, as these
//! sections contain verbatim markup (e.g., inline SVG). When rendering
//! untrusted ChordPro input, consumers should apply Content Security Policy
//! (CSP) headers or sandbox the HTML output to mitigate script injection.

use chordpro_core::ast::{CommentStyle, DirectiveKind, Line, LyricsLine, Song};
use chordpro_core::config::Config;
use chordpro_core::inline_markup::{SpanAttributes, TextSpan};
use chordpro_core::transpose::transpose_chord;

/// Maximum number of CSS columns allowed.
/// Matches `MAX_COLUMNS` in the PDF renderer.
const MAX_COLUMNS: u32 = 32;

// ---------------------------------------------------------------------------
// Formatting state
// ---------------------------------------------------------------------------

/// Tracks the current font/size/color settings for an element type.
///
/// Formatting directives like `{textfont}`, `{chordsize}`, etc. set these
/// values. The state persists until changed by another directive of the same
/// type.
#[derive(Default, Clone)]
struct ElementStyle {
    font: Option<String>,
    size: Option<String>,
    colour: Option<String>,
}

impl ElementStyle {
    /// Generate a CSS `style` attribute string, or empty if no styles are set.
    ///
    /// All values are passed through [`sanitize_css_value`] to prevent CSS
    /// injection via crafted directive values.
    fn to_css(&self) -> String {
        let mut css = String::new();
        if let Some(ref font) = self.font {
            css.push_str(&format!("font-family: {};", sanitize_css_value(font)));
        }
        if let Some(ref size) = self.size {
            let safe = sanitize_css_value(size);
            if safe.chars().all(|c| c.is_ascii_digit()) {
                css.push_str(&format!("font-size: {}pt;", safe));
            } else {
                css.push_str(&format!("font-size: {};", safe));
            }
        }
        if let Some(ref colour) = self.colour {
            css.push_str(&format!("color: {};", sanitize_css_value(colour)));
        }
        css
    }
}

/// Formatting state for all element types.
#[derive(Default, Clone)]
struct FormattingState {
    text: ElementStyle,
    chord: ElementStyle,
    tab: ElementStyle,
    title: ElementStyle,
    chorus: ElementStyle,
    label: ElementStyle,
    grid: ElementStyle,
}

impl FormattingState {
    /// Apply a formatting directive, updating the appropriate style.
    fn apply(&mut self, kind: &DirectiveKind, value: &Option<String>) {
        let val = value.clone();
        match kind {
            DirectiveKind::TextFont => self.text.font = val,
            DirectiveKind::TextSize => self.text.size = val,
            DirectiveKind::TextColour => self.text.colour = val,
            DirectiveKind::ChordFont => self.chord.font = val,
            DirectiveKind::ChordSize => self.chord.size = val,
            DirectiveKind::ChordColour => self.chord.colour = val,
            DirectiveKind::TabFont => self.tab.font = val,
            DirectiveKind::TabSize => self.tab.size = val,
            DirectiveKind::TabColour => self.tab.colour = val,
            DirectiveKind::TitleFont => self.title.font = val,
            DirectiveKind::TitleSize => self.title.size = val,
            DirectiveKind::TitleColour => self.title.colour = val,
            DirectiveKind::ChorusFont => self.chorus.font = val,
            DirectiveKind::ChorusSize => self.chorus.size = val,
            DirectiveKind::ChorusColour => self.chorus.colour = val,
            DirectiveKind::LabelFont => self.label.font = val,
            DirectiveKind::LabelSize => self.label.size = val,
            DirectiveKind::LabelColour => self.label.colour = val,
            DirectiveKind::GridFont => self.grid.font = val,
            DirectiveKind::GridSize => self.grid.size = val,
            DirectiveKind::GridColour => self.grid.colour = val,
            // Header/Footer/TOC directives are not rendered in the main body
            _ => {}
        }
    }
}

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
    render_song_with_transpose(song, 0, &Config::defaults())
}

/// Render a [`Song`] AST to an HTML5 document with an additional CLI transposition offset.
///
/// The `cli_transpose` parameter is added to any in-file `{transpose}` directive
/// values, allowing the CLI `--transpose` flag to combine with in-file directives.
#[must_use]
pub fn render_song_with_transpose(song: &Song, cli_transpose: i8, config: &Config) -> String {
    let _ = config;
    let mut html = String::new();
    let mut transpose_offset: i8 = cli_transpose;
    let mut fmt_state = FormattingState::default();

    let title = song.metadata.title.as_deref().unwrap_or("Untitled");
    html.push_str(&format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n",
        escape(title)
    ));
    html.push_str("<style>\n");
    html.push_str(CSS);
    html.push_str("</style>\n</head>\n<body>\n<div class=\"song\">\n");

    render_metadata(&song.metadata, &mut html);

    // Tracks whether a multi-column div is currently open.
    let mut columns_open = false;
    // Tracks whether we are inside an SVG delegate section.
    let mut in_svg_section = false;

    // Stores the rendered HTML of the most recently defined chorus body
    // (everything between StartOfChorus and EndOfChorus, excluding the
    // section open/close tags). Used by `{chorus}` recall.
    let mut chorus_html = String::new();
    // Temporary buffer for collecting chorus content while inside a chorus section.
    let mut chorus_buf: Option<String> = None;

    for line in &song.lines {
        match line {
            Line::Lyrics(lyrics_line) => {
                if in_svg_section {
                    // Inside SVG section: emit lyrics text as raw, unescaped content.
                    //
                    // NOTE: This is intentional per the ChordPro specification.
                    // Delegate sections (SVG, ABC, Lilypond, textblock) contain
                    // verbatim markup that must pass through without escaping.
                    // Consumers rendering untrusted ChordPro input should apply
                    // Content Security Policy (CSP) headers or sandbox the output
                    // to mitigate potential script injection.
                    let raw = lyrics_line.text();
                    html.push_str(&raw);
                    html.push('\n');
                } else {
                    let mut target = String::new();
                    render_lyrics(lyrics_line, transpose_offset, &fmt_state, &mut target);
                    if let Some(buf) = chorus_buf.as_mut() {
                        buf.push_str(&target);
                    }
                    html.push_str(&target);
                }
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
                    let (combined, saturated) =
                        chordpro_core::transpose::combine_transpose(file_offset, cli_transpose);
                    if saturated {
                        eprintln!(
                            "warning: transpose offset {file_offset} + {cli_transpose} \
                             exceeds i8 range, clamped to {combined}"
                        );
                    }
                    transpose_offset = combined;
                    continue;
                }
                if directive.kind.is_font_size_color() {
                    fmt_state.apply(&directive.kind, &directive.value);
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
                    DirectiveKind::Columns => {
                        // Clamp to 1..=32 to prevent degenerate CSS output.
                        // Parsing as u32 already rejects non-numeric input;
                        // clamping ensures the formatted value is always safe.
                        let n: u32 = directive
                            .value
                            .as_deref()
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(1)
                            .clamp(1, MAX_COLUMNS);
                        if columns_open {
                            html.push_str("</div>\n");
                            columns_open = false;
                        }
                        if n > 1 {
                            html.push_str(&format!(
                                "<div style=\"column-count: {};column-gap: 2em;\">\n",
                                n
                            ));
                            columns_open = true;
                        }
                    }
                    // All page control directives ({new_page}, {new_physical_page},
                    // {column_break}, {columns}) are intentionally excluded from the
                    // chorus buffer. These affect global page/column layout, and
                    // replaying them during {chorus} recall would produce unexpected
                    // layout changes (e.g., duplicate page breaks, column resets).
                    DirectiveKind::ColumnBreak => {
                        html.push_str("<div style=\"break-before: column;\"></div>\n");
                    }
                    DirectiveKind::NewPage | DirectiveKind::NewPhysicalPage => {
                        // TODO: NewPhysicalPage should eventually emit a
                        // different CSS hint for duplex printing scenarios.
                        html.push_str("<div style=\"break-before: page;\"></div>\n");
                    }
                    DirectiveKind::StartOfSvg => {
                        html.push_str("<div class=\"svg-section\">\n");
                        in_svg_section = true;
                    }
                    DirectiveKind::EndOfSvg => {
                        html.push_str("</div>\n");
                        in_svg_section = false;
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

    // Close any open multi-column div.
    if columns_open {
        html.push_str("</div>\n");
    }

    html.push_str("</div>\n</body>\n</html>\n");
    html
}

/// Render multiple [`Song`]s into a single HTML5 document.
#[must_use]
pub fn render_songs(songs: &[Song]) -> String {
    render_songs_with_transpose(songs, 0, &Config::defaults())
}

/// Render multiple [`Song`]s into a single HTML5 document with transposition.
///
/// When there is only one song, this is identical to [`render_song_with_transpose`].
/// For multiple songs, the document uses the first song's title and separates
/// each song with an `<hr class="song-separator">`.
#[must_use]
pub fn render_songs_with_transpose(songs: &[Song], cli_transpose: i8, config: &Config) -> String {
    if songs.len() <= 1 {
        return songs
            .first()
            .map(|s| render_song_with_transpose(s, cli_transpose, config))
            .unwrap_or_default();
    }
    // Use the first song's title for the document
    let mut html = String::new();
    let title = songs
        .first()
        .and_then(|s| s.metadata.title.as_deref())
        .unwrap_or("Untitled");
    html.push_str(&format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n",
        escape(title)
    ));
    html.push_str("<style>\n");
    html.push_str(CSS);
    html.push_str("</style>\n</head>\n<body>\n");

    for (i, song) in songs.iter().enumerate() {
        if i > 0 {
            html.push_str("<hr class=\"song-separator\">\n");
        }
        // Render each song as a full document, then extract the <div class="song">...</div> body.
        let song_html = render_song_with_transpose(song, cli_transpose, config);
        let song_start = "<div class=\"song\">";
        let body_end = "</div>\n</body>";
        if let Some(start) = song_html.find(song_start) {
            if let Some(end) = song_html.rfind(body_end) {
                html.push_str(&song_html[start..end + "</div>".len()]);
                html.push('\n');
            }
        }
    }

    html.push_str("</body>\n</html>\n");
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
            '\'' => out.push_str("&#39;"),
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
/// Formatting directives (font, size, color) are applied via inline CSS.
fn render_lyrics(
    lyrics_line: &LyricsLine,
    transpose_offset: i8,
    fmt_state: &FormattingState,
    html: &mut String,
) {
    html.push_str("<div class=\"line\">");

    for segment in &lyrics_line.segments {
        html.push_str("<span class=\"chord-block\">");

        if let Some(chord) = &segment.chord {
            let display_name = if transpose_offset != 0 {
                let transposed = transpose_chord(chord, transpose_offset);
                transposed.display_name().to_string()
            } else {
                chord.display_name().to_string()
            };
            let chord_css = fmt_state.chord.to_css();
            if chord_css.is_empty() {
                html.push_str(&format!(
                    "<span class=\"chord\">{}</span>",
                    escape(&display_name)
                ));
            } else {
                html.push_str(&format!(
                    "<span class=\"chord\" style=\"{}\">{}</span>",
                    escape(&chord_css),
                    escape(&display_name)
                ));
            }
        } else if lyrics_line.has_chords() {
            // Empty chord placeholder to maintain vertical alignment.
            html.push_str("<span class=\"chord\"></span>");
        }

        let text_css = fmt_state.text.to_css();
        if text_css.is_empty() {
            html.push_str("<span class=\"lyrics\">");
        } else {
            html.push_str(&format!(
                "<span class=\"lyrics\" style=\"{}\">",
                escape(&text_css)
            ));
        }
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
                    // CSS values are already sanitized by sanitize_css_value();
                    // applying escape() would double-encode (e.g., & → &amp;).
                    html.push_str(&format!("<span style=\"{css}\">"));
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
        css.push_str(&format!(
            "font-family: {};",
            sanitize_css_value(font_family)
        ));
    }
    if let Some(ref size) = attrs.size {
        let safe = sanitize_css_value(size);
        // If the size is a plain number, treat it as pt; otherwise pass through.
        if safe.chars().all(|c| c.is_ascii_digit()) {
            css.push_str(&format!("font-size: {}pt;", safe));
        } else {
            css.push_str(&format!("font-size: {};", safe));
        }
    }
    if let Some(ref fg) = attrs.foreground {
        css.push_str(&format!("color: {};", sanitize_css_value(fg)));
    }
    if let Some(ref bg) = attrs.background {
        css.push_str(&format!("background-color: {};", sanitize_css_value(bg)));
    }
    if let Some(ref weight) = attrs.weight {
        css.push_str(&format!("font-weight: {};", sanitize_css_value(weight)));
    }
    if let Some(ref style) = attrs.style {
        css.push_str(&format!("font-style: {};", sanitize_css_value(style)));
    }
    css
}

/// Sanitize a user-provided value for use in a CSS property value context.
///
/// Uses a whitelist approach: only characters safe in CSS values are retained.
/// Allowed: ASCII alphanumeric, `#` (hex colors), `.` (decimals), `-` (negatives,
/// hyphenated names), ` ` (multi-word font names), `,` (font family lists),
/// `%` (percentages), `+` (font-weight values like `+lighter`).
fn sanitize_css_value(s: &str) -> String {
    s.chars()
        .filter(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '#' | '.' | '-' | ' ' | ',' | '%' | '+')
        })
        .collect()
}

/// Sanitize a string for use as a CSS class name.
///
/// Only allows ASCII alphanumeric characters, hyphens, and underscores.
/// All other characters are replaced with hyphens. Leading hyphens that would
/// create an invalid CSS identifier are preserved since they follow the
/// `section-` prefix.
fn sanitize_css_class(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
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
        // StartOfSvg is handled in the main rendering loop with raw HTML
        // embedding (<div class="svg-section">), not via render_section_open.
        DirectiveKind::StartOfTextblock => {
            render_section_open("textblock", "Textblock", &directive.value, html);
        }
        DirectiveKind::StartOfSection(section_name) => {
            let class = format!("section-{}", sanitize_css_class(section_name));
            let label = chordpro_core::capitalize(&escape(section_name));
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
        DirectiveKind::Image(attrs) => {
            render_image(attrs, html);
        }
        DirectiveKind::Define => {
            if let Some(ref value) = directive.value {
                let def = chordpro_core::ast::ChordDefinition::parse_value(value);
                if let Some(ref raw) = def.raw {
                    if let Some(diagram) =
                        chordpro_core::chord_diagram::DiagramData::from_raw(&def.name, raw, 6)
                    {
                        html.push_str("<div class=\"chord-diagram-container\">");
                        html.push_str(&chordpro_core::chord_diagram::render_svg(&diagram));
                        html.push_str("</div>\n");
                    }
                }
            }
        }
        _ => {}
    }
}

/// Render an `{image}` directive as an HTML `<img>` element.
fn render_image(attrs: &chordpro_core::ast::ImageAttributes, html: &mut String) {
    let mut style = String::new();
    let mut img_attrs = format!("src=\"{}\"", escape(&attrs.src));

    if let Some(ref title) = attrs.title {
        img_attrs.push_str(&format!(" alt=\"{}\"", escape(title)));
    }

    if let Some(ref width) = attrs.width {
        img_attrs.push_str(&format!(" width=\"{}\"", escape(width)));
    }
    if let Some(ref height) = attrs.height {
        img_attrs.push_str(&format!(" height=\"{}\"", escape(height)));
    }
    if let Some(ref scale) = attrs.scale {
        // Scale as a CSS transform
        style.push_str(&format!(
            "transform: scale({});transform-origin: top left;",
            sanitize_css_value(scale)
        ));
    }

    // Determine wrapper alignment
    let align_css = match attrs.anchor.as_deref() {
        Some("line") | None => "",
        Some("column") => "text-align: center;",
        Some("paper") => "text-align: center;",
        _ => "",
    };

    if !align_css.is_empty() {
        html.push_str(&format!("<div style=\"{}\">", align_css));
    } else {
        html.push_str("<div>");
    }

    html.push_str(&format!("<img {}", img_attrs));
    if !style.is_empty() {
        html.push_str(&format!(" style=\"{}\"", escape(&style)));
    }
    html.push_str("></div>\n");
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

    #[test]
    fn test_custom_section_name_escaped() {
        let html = render(
            "{start_of_x<script>alert(1)</script>}\ntext\n{end_of_x<script>alert(1)</script>}",
        );
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn test_custom_section_name_quotes_escaped() {
        let html =
            render("{start_of_x\" onclick=\"alert(1)}\ntext\n{end_of_x\" onclick=\"alert(1)}");
        // The `"` must be escaped to `&quot;` so the attribute boundary is not broken.
        assert!(html.contains("&quot;"));
        assert!(!html.contains("class=\"section-x\""));
    }

    #[test]
    fn test_custom_section_name_single_quotes_escaped() {
        let html = render("{start_of_x' onclick='alert(1)}\ntext\n{end_of_x' onclick='alert(1)}");
        // The `'` must be escaped to `&#39;` so single-quote attribute boundaries
        // cannot be broken.
        assert!(html.contains("&#39;"));
        assert!(!html.contains("onclick='alert"));
    }

    #[test]
    fn test_custom_section_name_space_sanitized_in_class() {
        // Spaces in section names must not create multiple CSS classes
        let html = render("{start_of_foo bar}\ntext\n{end_of_foo bar}");
        // Class should be "section-foo-bar", not "section-foo bar"
        assert!(html.contains("section-foo-bar"));
        assert!(!html.contains("class=\"section-foo bar\""));
    }

    #[test]
    fn test_custom_section_name_special_chars_sanitized_in_class() {
        let html = render("{start_of_a&b<c>d}\ntext\n{end_of_a&b<c>d}");
        // Special characters replaced with hyphens in class name
        assert!(html.contains("section-a-b-c-d"));
        // Label still uses HTML escaping for display
        assert!(html.contains("&amp;"));
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
        let html = render_song_with_transpose(&song, 3, &Config::defaults());
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
    fn test_span_css_injection_url_prevented() {
        let html = render(
            r#"<span foreground="red; background-image: url('https://evil.com/')">text</span>"#,
        );
        // Parentheses and semicolons must be stripped, preventing url() and property injection.
        assert!(!html.contains("url("));
        assert!(!html.contains(";background-image"));
    }

    #[test]
    fn test_span_css_injection_semicolon_stripped() {
        let html =
            render(r#"<span foreground="red; position: absolute; z-index: 9999">text</span>"#);
        // Semicolons must be stripped so injected properties cannot create new
        // CSS property boundaries. Without `;`, "position: absolute" is just
        // noise inside the single `color:` value, not a separate property.
        assert!(!html.contains(";position"));
        assert!(!html.contains("; position"));
        assert!(html.contains("color:"));
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

    // -- formatting directive tests -------------------------------------------

    #[test]
    fn test_textfont_directive_applies_css() {
        let html = render("{textfont: Courier}\nHello world");
        assert!(html.contains("font-family: Courier;"));
    }

    #[test]
    fn test_textsize_directive_applies_css() {
        let html = render("{textsize: 14}\nHello world");
        assert!(html.contains("font-size: 14pt;"));
    }

    #[test]
    fn test_textcolour_directive_applies_css() {
        let html = render("{textcolour: blue}\nHello world");
        assert!(html.contains("color: blue;"));
    }

    #[test]
    fn test_chordfont_directive_applies_css() {
        let html = render("{chordfont: Monospace}\n[Am]Hello");
        assert!(html.contains("font-family: Monospace;"));
    }

    #[test]
    fn test_chordsize_directive_applies_css() {
        let html = render("{chordsize: 16}\n[Am]Hello");
        // Chord span should have the size style
        assert!(html.contains("font-size: 16pt;"));
    }

    #[test]
    fn test_chordcolour_directive_applies_css() {
        let html = render("{chordcolour: green}\n[Am]Hello");
        assert!(html.contains("color: green;"));
    }

    #[test]
    fn test_formatting_persists_across_lines() {
        let html = render("{textcolour: red}\nLine one\nLine two");
        // Both lines should have the color applied
        let count = html.matches("color: red;").count();
        assert!(
            count >= 2,
            "formatting should persist: found {count} matches"
        );
    }

    #[test]
    fn test_formatting_overridden_by_later_directive() {
        let html = render("{textcolour: red}\nRed text\n{textcolour: blue}\nBlue text");
        assert!(html.contains("color: red;"));
        assert!(html.contains("color: blue;"));
    }

    #[test]
    fn test_no_formatting_no_style_attr() {
        let html = render("Plain text");
        // lyrics span should not have a style attribute
        assert!(!html.contains("<span class=\"lyrics\" style="));
    }

    #[test]
    fn test_formatting_directive_css_injection_prevented() {
        let html = render("{textcolour: red; position: fixed; z-index: 9999}\nHello");
        // Semicolons stripped — no additional CSS property injection.
        assert!(!html.contains(";position"));
        assert!(!html.contains("; position"));
        assert!(html.contains("color:"));
    }

    #[test]
    fn test_formatting_directive_url_injection_prevented() {
        let html = render("{textcolour: red; background-image: url('https://evil.com/')}\nHello");
        // Parentheses and semicolons stripped.
        assert!(!html.contains("url("));
    }

    // -- column layout tests --------------------------------------------------

    #[test]
    fn test_columns_directive_generates_css() {
        let html = render("{columns: 2}\nLine one\nLine two");
        assert!(html.contains("column-count: 2"));
    }

    #[test]
    fn test_columns_reset_to_one() {
        let html = render("{columns: 2}\nTwo cols\n{columns: 1}\nOne col");
        // Should open and then close the multi-column div
        let count = html.matches("column-count: 2").count();
        assert_eq!(count, 1);
        assert!(html.contains("One col"));
    }

    #[test]
    fn test_column_break_generates_css() {
        let html = render("{columns: 2}\nCol 1\n{column_break}\nCol 2");
        assert!(html.contains("break-before: column;"));
    }

    #[test]
    fn test_columns_clamped_to_max() {
        let html = render("{columns: 999}\nContent");
        // Should be clamped to 32
        assert!(html.contains("column-count: 32"));
    }

    #[test]
    fn test_columns_zero_treated_as_one() {
        let html = render("{columns: 0}\nContent");
        // 0 is clamped to 1, so no multi-column div should be opened
        assert!(!html.contains("column-count"));
    }

    #[test]
    fn test_columns_non_numeric_defaults_to_one() {
        let html = render("{columns: abc}\nHello");
        // Non-numeric value should default to 1, so no multi-column div.
        assert!(!html.contains("column-count"));
    }

    #[test]
    fn test_new_page_generates_page_break() {
        let html = render("Page 1\n{new_page}\nPage 2");
        assert!(html.contains("break-before: page;"));
    }

    #[test]
    fn test_new_physical_page_generates_page_break() {
        let html = render("Page 1\n{new_physical_page}\nPage 2");
        assert!(html.contains("break-before: page;"));
    }

    #[test]
    fn test_page_control_not_replayed_in_chorus_recall() {
        // Page control directives inside a chorus must NOT appear in {chorus} recall.
        let input = "\
{start_of_chorus}\n\
{new_page}\n\
[G]La la la\n\
{end_of_chorus}\n\
Verse text\n\
{chorus}";
        let html = render(input);
        // The initial chorus renders a page break.
        assert!(html.contains("break-before: page;"));
        // Count: only ONE page-break div should exist (from the original chorus,
        // not from the recall).
        let count = html.matches("break-before: page;").count();
        assert_eq!(count, 1, "page break must not be replayed in chorus recall");
    }

    // -- image directive tests ------------------------------------------------

    #[test]
    fn test_image_basic() {
        let html = render("{image: src=photo.jpg}");
        assert!(html.contains("<img src=\"photo.jpg\""));
    }

    #[test]
    fn test_image_with_dimensions() {
        let html = render("{image: src=photo.jpg width=200 height=100}");
        assert!(html.contains("width=\"200\""));
        assert!(html.contains("height=\"100\""));
    }

    #[test]
    fn test_image_with_title() {
        let html = render("{image: src=photo.jpg title=\"My Photo\"}");
        assert!(html.contains("alt=\"My Photo\""));
    }

    #[test]
    fn test_image_with_scale() {
        let html = render("{image: src=photo.jpg scale=0.5}");
        assert!(html.contains("scale(0.5)"));
    }

    // -- chord diagram tests --------------------------------------------------

    #[test]
    fn test_define_renders_svg_diagram() {
        let html = render("{define: Am base-fret 1 frets x 0 2 2 1 0}");
        assert!(html.contains("<svg"));
        assert!(html.contains("Am"));
        assert!(html.contains("chord-diagram"));
    }

    #[test]
    fn test_define_keyboard_no_diagram() {
        let html = render("{define: Am keys 0 3 7}");
        // Keyboard definitions don't produce SVG diagrams
        assert!(!html.contains("<svg"));
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
        // SVG sections embed content directly (not in a section element)
        assert!(html.contains("<div class=\"svg-section\">"));
        assert!(html.contains("<svg/>"));
        assert!(html.contains("</div>"));
    }

    #[test]
    fn test_render_svg_inline_content() {
        let svg = r#"<svg width="100" height="100"><circle cx="50" cy="50" r="40"/></svg>"#;
        let input = format!("{{start_of_svg}}\n{svg}\n{{end_of_svg}}");
        let html = render(&input);
        assert!(html.contains(svg));
    }

    #[test]
    fn test_render_textblock_section() {
        let html = render("{start_of_textblock}\nPreformatted\n{end_of_textblock}");
        assert!(html.contains("<section class=\"textblock\">"));
        assert!(html.contains("Textblock"));
        assert!(html.contains("</section>"));
    }

    // --- Multi-song rendering ---

    #[test]
    fn test_render_songs_single() {
        let songs = chordpro_core::parse_multi("{title: Only}").unwrap();
        let html = render_songs(&songs);
        // Single song: should be identical to render_song
        assert_eq!(html, render_song(&songs[0]));
    }

    #[test]
    fn test_render_songs_two_songs_with_hr_separator() {
        let songs = chordpro_core::parse_multi(
            "{title: Song A}\n[Am]Hello\n{new_song}\n{title: Song B}\n[G]World",
        )
        .unwrap();
        let html = render_songs(&songs);
        // Document title from first song
        assert!(html.contains("<title>Song A</title>"));
        // Both songs present
        assert!(html.contains("<h1>Song A</h1>"));
        assert!(html.contains("<h1>Song B</h1>"));
        // Separator between songs
        assert!(html.contains("<hr class=\"song-separator\">"));
        // Each song in its own div.song
        assert_eq!(html.matches("<div class=\"song\">").count(), 2);
        // Single HTML document wrapper
        assert_eq!(html.matches("<!DOCTYPE html>").count(), 1);
        assert_eq!(html.matches("</html>").count(), 1);
    }

    #[test]
    fn test_image_scale_css_injection_prevented() {
        // The scale parameter must be sanitized as a CSS value to prevent
        // injection of arbitrary CSS properties via parentheses and semicolons.
        let html = render("{image: src=photo.jpg scale=0.5); position: fixed; z-index: 9999}");
        assert!(!html.contains("position"));
        assert!(!html.contains("z-index"));
        // Dangerous characters should be stripped by sanitize_css_value
        assert!(!html.contains("position: fixed"));
    }

    #[test]
    fn test_render_songs_with_transpose() {
        let songs =
            chordpro_core::parse_multi("{title: S1}\n[C]Do\n{new_song}\n{title: S2}\n[G]Re")
                .unwrap();
        let html = render_songs_with_transpose(&songs, 2, &Config::defaults());
        // C+2=D, G+2=A
        assert!(html.contains(">D<"));
        assert!(html.contains(">A<"));
    }
}
