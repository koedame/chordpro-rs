//! PDF renderer for ChordPro documents.
//!
//! Converts a parsed ChordPro AST into a PDF document using built-in PDF
//! Type1 fonts (Helvetica family). No external dependencies — the PDF is
//! generated directly from raw PDF object structures.
//!
//! Supports multi-page output: content automatically flows to new pages when
//! the current page overflows, and `{new_page}` / `{new_physical_page}`
//! directives trigger explicit page breaks.

use chordpro_core::ast::{CommentStyle, DirectiveKind, ImageAttributes, Line, LyricsLine, Song};
use chordpro_core::inline_markup::TextSpan;
use chordpro_core::transpose::transpose_chord;

// ---------------------------------------------------------------------------
// Formatting state
// ---------------------------------------------------------------------------

/// Tracks the current font size for an element type.
///
/// PDF Type1 fonts are limited to the Helvetica family, so font name changes
/// from directives are not applicable. Size changes are applied directly.
#[derive(Default, Clone)]
struct PdfElementStyle {
    size: Option<f32>,
}

/// Formatting state for PDF rendering.
#[derive(Default, Clone)]
struct PdfFormattingState {
    text: PdfElementStyle,
    chord: PdfElementStyle,
}

impl PdfFormattingState {
    /// Apply a formatting directive, updating the appropriate style.
    fn apply(&mut self, kind: &DirectiveKind, value: &Option<String>) {
        let size_val = value
            .as_deref()
            .and_then(|v| v.parse::<f32>().ok())
            .map(|s| s.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE));
        match kind {
            DirectiveKind::TextSize => self.text.size = size_val,
            DirectiveKind::ChordSize => self.chord.size = size_val,
            _ => {}
        }
    }

    /// Get the effective lyrics font size.
    fn lyrics_size(&self) -> f32 {
        self.text.size.unwrap_or(LYRICS_SIZE)
    }

    /// Get the effective chord font size.
    fn chord_size(&self) -> f32 {
        self.chord.size.unwrap_or(CHORD_SIZE)
    }
}

// ---------------------------------------------------------------------------
// Layout constants (units: PDF points, 1 pt = 1/72 inch)
// ---------------------------------------------------------------------------

/// A4 page width in points.
const PAGE_W: f32 = 595.0;
/// A4 page height in points.
const PAGE_H: f32 = 842.0;
/// Left margin in points.
const MARGIN_LEFT: f32 = 56.0;
/// Top margin (distance from top of page).
const MARGIN_TOP: f32 = 56.0;
/// Bottom margin — content below this Y coordinate triggers a new page.
const MARGIN_BOTTOM: f32 = 56.0;
/// Title font size.
const TITLE_SIZE: f32 = 18.0;
/// Subtitle font size.
const SUBTITLE_SIZE: f32 = 13.0;
/// Chord font size.
const CHORD_SIZE: f32 = 9.0;
/// Lyrics font size.
const LYRICS_SIZE: f32 = 11.0;
/// Section label font size.
const SECTION_SIZE: f32 = 10.0;
/// Comment font size.
const COMMENT_SIZE: f32 = 9.0;
/// Spacing between lines.
const LINE_GAP: f32 = 4.0;
/// Average character width as fraction of font size (Helvetica approximation).
const CHAR_WIDTH: f32 = 0.52;
/// Table of Contents entry font size.
const TOC_ENTRY_SIZE: f32 = 11.0;
/// Maximum number of pages a single document can contain.
/// Prevents resource exhaustion from malicious input.
const MAX_PAGES: usize = 10_000;
/// Maximum number of columns allowed.
/// Prevents degenerate layout and f32 overflow from extreme values.
const MAX_COLUMNS: u32 = 32;
/// Minimum font size (in points) accepted from user directives.
const MIN_FONT_SIZE: f32 = 0.5;
/// Maximum font size (in points) accepted from user directives.
const MAX_FONT_SIZE: f32 = 200.0;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Render a [`Song`] AST to PDF bytes.
///
/// Returns a complete PDF document as `Vec<u8>` using built-in Helvetica
/// fonts. No external font files are required.
///
/// The `{chorus}` directive recalls the most recently defined chorus section,
/// re-rendering its content with a "Chorus" label.
///
/// Long songs automatically flow across multiple pages. The `{new_page}` and
/// `{new_physical_page}` directives trigger explicit page breaks.
#[must_use]
pub fn render_song(song: &Song) -> Vec<u8> {
    render_song_with_transpose(song, 0)
}

/// Render a [`Song`] AST to PDF bytes with an additional CLI transposition offset.
///
/// The `cli_transpose` parameter is added to any in-file `{transpose}` directive
/// values, allowing the CLI `--transpose` flag to combine with in-file directives.
#[must_use]
pub fn render_song_with_transpose(song: &Song, cli_transpose: i8) -> Vec<u8> {
    let mut doc = PdfDocument::new();
    render_song_into_doc(song, cli_transpose, &mut doc);
    doc.build_pdf()
}

/// Render multiple [`Song`]s into a single multi-page PDF document.
///
/// Each song starts on a new page.
#[must_use]
pub fn render_songs(songs: &[Song]) -> Vec<u8> {
    render_songs_with_transpose(songs, 0)
}

/// Render multiple [`Song`]s into a single PDF with transposition.
///
/// Each song starts on a new page (except the first). When there are two or
/// more songs, a Table of Contents page is prepended with song titles and
/// page numbers.
#[must_use]
pub fn render_songs_with_transpose(songs: &[Song], cli_transpose: i8) -> Vec<u8> {
    if songs.len() == 1 {
        return render_song_with_transpose(&songs[0], cli_transpose);
    }

    // Phase 1: render all songs and record which page each starts on.
    let mut body_doc = PdfDocument::new();
    let mut toc_entries: Vec<(String, usize)> = Vec::new(); // (title, page_index)

    for (i, song) in songs.iter().enumerate() {
        if i > 0 {
            body_doc.new_page();
        }
        let start_page = body_doc.page_count();
        let title = song
            .metadata
            .title
            .as_deref()
            .unwrap_or("Untitled")
            .to_string();
        toc_entries.push((title, start_page));
        render_song_into_doc(song, cli_transpose, &mut body_doc);
    }

    // Phase 2: generate ToC pages.
    let mut toc_doc = PdfDocument::new();
    toc_doc.text("Table of Contents", Font::HelveticaBold, TITLE_SIZE);
    toc_doc.newline(TITLE_SIZE + LINE_GAP * 2.0);

    let toc_page_count = {
        for (title, body_page_idx) in &toc_entries {
            toc_doc.ensure_space(TOC_ENTRY_SIZE + LINE_GAP);
            // Page number = toc pages + body page index
            // (we'll calculate toc_page_count after this loop)
            let page_num_placeholder = body_page_idx + 1; // 1-based, offset added later
            let entry_text = format!("{title}  ......  {page_num_placeholder}");
            toc_doc.text(&entry_text, Font::Helvetica, TOC_ENTRY_SIZE);
            toc_doc.newline(TOC_ENTRY_SIZE + LINE_GAP);
        }
        toc_doc.page_count()
    };

    // Phase 3: rebuild ToC with correct page numbers (offset by toc_page_count).
    let mut toc_doc = PdfDocument::new();
    toc_doc.text("Table of Contents", Font::HelveticaBold, TITLE_SIZE);
    toc_doc.newline(TITLE_SIZE + LINE_GAP * 2.0);

    for (title, body_page_idx) in &toc_entries {
        toc_doc.ensure_space(TOC_ENTRY_SIZE + LINE_GAP);
        let page_num = body_page_idx + toc_page_count; // 1-based page number
        let x = toc_doc.margin_left();
        let y = toc_doc.y();

        // Render title on the left
        toc_doc.text_at(title, Font::Helvetica, TOC_ENTRY_SIZE, x, y);

        // Render page number right-aligned
        let num_str = page_num.to_string();
        let num_width = num_str.len() as f32 * TOC_ENTRY_SIZE * CHAR_WIDTH;
        let right_x = PAGE_W - MARGIN_RIGHT - num_width;
        toc_doc.text_at(&num_str, Font::Helvetica, TOC_ENTRY_SIZE, right_x, y);

        toc_doc.newline(TOC_ENTRY_SIZE + LINE_GAP);
    }

    // Phase 4: combine ToC pages + body pages.
    let mut combined = toc_doc;
    for page_ops in body_doc.take_pages() {
        combined.push_page(page_ops);
    }

    combined.build_pdf()
}

/// Render a single song's content into an existing [`PdfDocument`].
///
/// This is the shared implementation used by both [`render_song_with_transpose`]
/// and [`render_songs_with_transpose`]. It does not call `build_pdf`; the caller
/// is responsible for finalising the document.
fn render_song_into_doc(song: &Song, cli_transpose: i8, doc: &mut PdfDocument) {
    let mut transpose_offset: i8 = cli_transpose;
    let mut fmt_state = PdfFormattingState::default();

    // Title
    if let Some(title) = &song.metadata.title {
        doc.text(title, Font::HelveticaBold, TITLE_SIZE);
        doc.newline(TITLE_SIZE + LINE_GAP);
    }
    // Subtitles
    for subtitle in &song.metadata.subtitles {
        doc.text(subtitle, Font::Helvetica, SUBTITLE_SIZE);
        doc.newline(SUBTITLE_SIZE + LINE_GAP);
    }

    // Stores the AST lines of the most recently defined chorus body for replay.
    let mut chorus_body: Vec<Line> = Vec::new();
    // Temporary buffer for collecting chorus content while inside a chorus section.
    let mut chorus_buf: Option<Vec<Line>> = None;

    for line in &song.lines {
        match line {
            Line::Lyrics(lyrics) => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                render_lyrics(lyrics, transpose_offset, &fmt_state, doc);
            }
            Line::Directive(d) if !d.kind.is_metadata() => {
                if d.kind == DirectiveKind::Transpose {
                    let file_offset: i8 =
                        d.value.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0);
                    transpose_offset = file_offset.saturating_add(cli_transpose);
                    continue;
                }
                if d.kind.is_font_size_color() {
                    fmt_state.apply(&d.kind, &d.value);
                    continue;
                }
                match &d.kind {
                    DirectiveKind::StartOfChorus => {
                        render_section_label(d, doc);
                        chorus_buf = Some(Vec::new());
                    }
                    DirectiveKind::EndOfChorus => {
                        if let Some(buf) = chorus_buf.take() {
                            chorus_body = buf;
                        }
                    }
                    DirectiveKind::Chorus => {
                        render_chorus_recall(
                            &d.value,
                            &chorus_body,
                            transpose_offset,
                            &fmt_state,
                            doc,
                        );
                    }
                    // Page control directives are intentionally NOT captured
                    // in the chorus buffer. Replaying page/column breaks during
                    // {chorus} recall would produce unexpected layout changes.
                    DirectiveKind::NewPage | DirectiveKind::NewPhysicalPage => {
                        // TODO: NewPhysicalPage should eventually handle duplex
                        // printing differently (e.g., insert a blank page to
                        // ensure the next content starts on a recto page).
                        doc.new_page();
                    }
                    DirectiveKind::Columns => {
                        let n: u32 = d
                            .value
                            .as_deref()
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(1);
                        doc.set_columns(n);
                    }
                    DirectiveKind::ColumnBreak => {
                        doc.column_break();
                    }
                    _ => {
                        if let Some(buf) = chorus_buf.as_mut() {
                            buf.push(line.clone());
                        }
                        render_directive(d, doc);
                    }
                }
            }
            Line::Comment(style, text) => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                render_comment(*style, text, doc);
            }
            Line::Empty => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                doc.newline(LINE_GAP * 2.0);
            }
            _ => {}
        }
    }
}

/// Parse and render a ChordPro source string to PDF bytes.
pub fn try_render(input: &str) -> Result<Vec<u8>, chordpro_core::ParseError> {
    let song = chordpro_core::parse(input)?;
    Ok(render_song(&song))
}

// ---------------------------------------------------------------------------
// Content builders
// ---------------------------------------------------------------------------

fn render_lyrics(
    lyrics: &LyricsLine,
    transpose_offset: i8,
    fmt_state: &PdfFormattingState,
    doc: &mut PdfDocument,
) {
    let has_markup = lyrics.segments.iter().any(|s| s.has_markup());
    let lyrics_size = fmt_state.lyrics_size();
    let chord_size = fmt_state.chord_size();

    if !lyrics.has_chords() {
        doc.ensure_space(lyrics_size + LINE_GAP);
        if has_markup {
            render_lyrics_spans(lyrics, lyrics_size, doc);
        } else {
            doc.text(&lyrics.text(), Font::Helvetica, lyrics_size);
        }
        doc.newline(lyrics_size + LINE_GAP);
        return;
    }

    // Need space for chord row + lyrics row
    doc.ensure_space(chord_size + 2.0 + lyrics_size + LINE_GAP);

    // Chord row
    let mut x = doc.margin_left();
    let start_y = doc.y();
    for seg in &lyrics.segments {
        if let Some(chord) = &seg.chord {
            let display_name = if transpose_offset != 0 {
                transpose_chord(chord, transpose_offset)
                    .display_name()
                    .to_string()
            } else {
                chord.display_name().to_string()
            };
            doc.text_at(&display_name, Font::HelveticaBold, chord_size, x, start_y);
        }
        let text_w = seg.text.chars().count() as f32 * lyrics_size * CHAR_WIDTH;
        let chord_w = seg.chord.as_ref().map_or(0.0, |c| {
            let display_len = if transpose_offset != 0 {
                transpose_chord(c, transpose_offset)
                    .display_name()
                    .chars()
                    .count()
            } else {
                c.display_name().chars().count()
            };
            display_len as f32 * chord_size * CHAR_WIDTH + 2.0
        });
        x += text_w.max(chord_w);
    }

    // Lyrics row
    doc.advance_y(chord_size + 2.0);
    if has_markup {
        render_lyrics_spans(lyrics, lyrics_size, doc);
    } else {
        doc.text(&lyrics.text(), Font::Helvetica, lyrics_size);
    }
    doc.newline(lyrics_size + LINE_GAP);
}

/// Render lyrics line with inline markup using font changes.
///
/// Walks the span tree for each segment, switching between Helvetica,
/// HelveticaBold, HelveticaOblique, and HelveticaBoldOblique as needed.
fn render_lyrics_spans(lyrics: &LyricsLine, font_size: f32, doc: &mut PdfDocument) {
    let mut x = doc.margin_left();
    let y = doc.y();
    for seg in &lyrics.segments {
        if seg.has_markup() {
            x = render_span_list(&seg.spans, doc, x, y, font_size, false, false);
        } else {
            doc.text_at(&seg.text, Font::Helvetica, font_size, x, y);
            x += seg.text.chars().count() as f32 * font_size * CHAR_WIDTH;
        }
    }
}

/// Recursively render a list of [`TextSpan`]s at the given (x, y) position.
///
/// Returns the new X position after all text has been emitted.
fn render_span_list(
    spans: &[TextSpan],
    doc: &mut PdfDocument,
    mut x: f32,
    y: f32,
    font_size: f32,
    bold: bool,
    italic: bool,
) -> f32 {
    for span in spans {
        match span {
            TextSpan::Plain(text) => {
                let font = match (bold, italic) {
                    (true, true) => Font::HelveticaBoldOblique,
                    (true, false) => Font::HelveticaBold,
                    (false, true) => Font::HelveticaOblique,
                    (false, false) => Font::Helvetica,
                };
                doc.text_at(text, font, font_size, x, y);
                x += text.chars().count() as f32 * font_size * CHAR_WIDTH;
            }
            TextSpan::Bold(children) => {
                x = render_span_list(children, doc, x, y, font_size, true, italic);
            }
            TextSpan::Italic(children) => {
                x = render_span_list(children, doc, x, y, font_size, bold, true);
            }
            TextSpan::Highlight(children) | TextSpan::Comment(children) => {
                // Highlight/comment: render children with current style
                // (no distinct visual in basic PDF)
                x = render_span_list(children, doc, x, y, font_size, bold, italic);
            }
            TextSpan::Span(attrs, children) => {
                // Apply weight/style from span attributes
                let span_bold = bold
                    || attrs
                        .weight
                        .as_deref()
                        .is_some_and(|w| w.eq_ignore_ascii_case("bold"));
                let span_italic = italic
                    || attrs
                        .style
                        .as_deref()
                        .is_some_and(|s| s.eq_ignore_ascii_case("italic"));
                x = render_span_list(children, doc, x, y, font_size, span_bold, span_italic);
            }
        }
    }
    x
}

/// Render the section label for a start-of-section directive.
fn render_section_label(directive: &chordpro_core::ast::Directive, doc: &mut PdfDocument) {
    let label: Option<String> = match &directive.kind {
        DirectiveKind::StartOfChorus => Some("Chorus".to_string()),
        DirectiveKind::StartOfVerse => Some("Verse".to_string()),
        DirectiveKind::StartOfBridge => Some("Bridge".to_string()),
        DirectiveKind::StartOfTab => Some("Tab".to_string()),
        DirectiveKind::StartOfGrid => Some("Grid".to_string()),
        DirectiveKind::StartOfAbc => Some("ABC".to_string()),
        DirectiveKind::StartOfLy => Some("Lilypond".to_string()),
        DirectiveKind::StartOfSvg => Some("SVG".to_string()),
        DirectiveKind::StartOfTextblock => Some("Textblock".to_string()),
        DirectiveKind::StartOfSection(section_name) => {
            Some(chordpro_core::capitalize(section_name))
        }
        _ => None,
    };
    if let Some(label) = label {
        let text = match &directive.value {
            Some(v) if !v.is_empty() => format!("{label}: {v}"),
            _ => label,
        };
        doc.ensure_space(SECTION_SIZE + LINE_GAP);
        doc.text(&text, Font::HelveticaBoldOblique, SECTION_SIZE);
        doc.newline(SECTION_SIZE + LINE_GAP);
    }
}

fn render_directive(directive: &chordpro_core::ast::Directive, doc: &mut PdfDocument) {
    if directive.kind == DirectiveKind::Define {
        if let Some(ref value) = directive.value {
            let def = chordpro_core::ast::ChordDefinition::parse_value(value);
            if let Some(ref raw) = def.raw {
                if let Some(diagram) =
                    chordpro_core::chord_diagram::DiagramData::from_raw(&def.name, raw, 6)
                {
                    render_chord_diagram_pdf(&diagram, doc);
                    return;
                }
            }
        }
    }

    if let DirectiveKind::Image(ref attrs) = directive.kind {
        render_image(attrs, doc);
        return;
    }

    render_section_label(directive, doc);
}

/// Render an `{image}` directive by embedding a JPEG file into the PDF.
///
/// Only JPEG files (`.jpg` / `.jpeg` extension) are supported. If the file
/// cannot be read, has no recognisable JPEG header, or is not a JPEG
/// extension, the directive is silently skipped.
fn render_image(attrs: &ImageAttributes, doc: &mut PdfDocument) {
    if attrs.src.is_empty() {
        return;
    }

    // Only support JPEG files for now.
    let src_lower = attrs.src.to_ascii_lowercase();
    if !src_lower.ends_with(".jpg") && !src_lower.ends_with(".jpeg") {
        return;
    }

    let data = match std::fs::read(&attrs.src) {
        Ok(d) => d,
        Err(_) => return,
    };

    let (pixel_w, pixel_h) = match parse_jpeg_dimensions(&data) {
        Some(dims) => dims,
        None => return,
    };

    if pixel_w == 0 || pixel_h == 0 {
        return;
    }

    // Compute rendered dimensions in PDF points (1 pt = 1/72 inch).
    // Default: use pixel dimensions as points (1 pixel = 1 point).
    let native_w = pixel_w as f32;
    let native_h = pixel_h as f32;
    let aspect = native_w / native_h;

    let (render_w, render_h) = compute_image_dimensions(attrs, native_w, native_h, aspect);

    // Clamp to printable area.
    let max_w = PAGE_W - MARGIN_LEFT - MARGIN_RIGHT;
    let (render_w, render_h) = if render_w > max_w {
        (max_w, max_w / aspect)
    } else {
        (render_w, render_h)
    };

    doc.ensure_space(render_h + LINE_GAP);

    let img_idx = doc.embed_jpeg(data, pixel_w, pixel_h);
    let x = doc.margin_left();
    // PDF images are placed with the origin at the bottom-left corner.
    let y = doc.y() - render_h;
    doc.draw_image(img_idx, x, y, render_w, render_h);
    doc.advance_y(render_h);
    doc.newline(LINE_GAP);
}

/// Compute the rendered width and height of an image based on the directive
/// attributes (`width`, `height`, `scale`).
///
/// Priority: explicit width/height > scale > native dimensions.
fn compute_image_dimensions(
    attrs: &ImageAttributes,
    native_w: f32,
    native_h: f32,
    aspect: f32,
) -> (f32, f32) {
    let parsed_w = attrs
        .width
        .as_deref()
        .and_then(|v| v.trim().parse::<f32>().ok());
    let parsed_h = attrs
        .height
        .as_deref()
        .and_then(|v| v.trim().parse::<f32>().ok());
    let parsed_scale = attrs
        .scale
        .as_deref()
        .and_then(|v| v.trim().parse::<f32>().ok());

    match (parsed_w, parsed_h) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, w / aspect),
        (None, Some(h)) => (h * aspect, h),
        (None, None) => {
            if let Some(s) = parsed_scale {
                (native_w * s, native_h * s)
            } else {
                (native_w, native_h)
            }
        }
    }
}

/// Render a chord diagram directly into the PDF content stream.
///
/// Uses PDF line/circle drawing operations to reproduce the chord grid,
/// finger dots, open/muted string markers, and the chord name.
fn render_chord_diagram_pdf(
    data: &chordpro_core::chord_diagram::DiagramData,
    doc: &mut PdfDocument,
) {
    let cell_w: f32 = 10.0;
    let cell_h: f32 = 12.0;
    let num_strings = data.strings;
    let num_frets = data.frets_shown;
    let grid_w = (num_strings - 1) as f32 * cell_w;
    let grid_h = num_frets as f32 * cell_h;
    let total_h = grid_h + 25.0; // title + top markers + grid

    doc.ensure_space(total_h);

    let base_x = doc.margin_left();
    // PDF Y is bottom-up, so top of diagram is at doc.y
    let top_y = doc.y();

    // Chord name
    doc.text_at(&data.name, Font::HelveticaBold, 9.0, base_x, top_y);

    let grid_top = top_y - 15.0; // below the name

    // Nut line (thick for open position)
    if data.base_fret == 1 {
        doc.line_at(base_x, grid_top, base_x + grid_w, grid_top, 2.0);
    } else {
        // Show fret number
        let fret_label = format!("{}fr", data.base_fret);
        doc.text_at(
            &fret_label,
            Font::Helvetica,
            6.0,
            base_x - 16.0,
            grid_top - cell_h / 2.0,
        );
    }

    // Vertical lines (strings)
    for i in 0..num_strings {
        let x = base_x + i as f32 * cell_w;
        doc.line_at(x, grid_top, x, grid_top - grid_h, 0.5);
    }

    // Horizontal lines (frets)
    for j in 0..=num_frets {
        let y = grid_top - j as f32 * cell_h;
        doc.line_at(base_x, y, base_x + grid_w, y, 0.5);
    }

    // Finger positions, open, and muted markers
    for (i, &fret) in data.frets.iter().enumerate() {
        if i >= num_strings {
            break;
        }
        let x = base_x + i as f32 * cell_w;
        if fret == -1 {
            // Muted: X above nut
            doc.text_at("X", Font::Helvetica, 7.0, x - 2.5, grid_top + 4.0);
        } else if fret == 0 {
            // Open: circle above nut
            doc.stroked_circle_at(x, grid_top + 6.0, 2.5);
        } else {
            // Fretted: filled dot
            let y = grid_top - (fret as f32 - 0.5) * cell_h;
            doc.filled_circle_at(x, y, 3.0);
        }
    }

    doc.advance_y(total_h);
    doc.newline(LINE_GAP);
}

/// Render a `{chorus}` recall directive in the PDF.
///
/// Emits a "Chorus" label (with optional custom label) followed by the content
/// of the most recently defined chorus section.
fn render_chorus_recall(
    value: &Option<String>,
    chorus_body: &[Line],
    transpose_offset: i8,
    fmt_state: &PdfFormattingState,
    doc: &mut PdfDocument,
) {
    let text = match value {
        Some(v) if !v.is_empty() => format!("Chorus: {v}"),
        _ => "Chorus".to_string(),
    };
    doc.ensure_space(SECTION_SIZE + LINE_GAP);
    doc.text(&text, Font::HelveticaBoldOblique, SECTION_SIZE);
    doc.newline(SECTION_SIZE + LINE_GAP);

    // Replay the stored chorus body lines.
    for line in chorus_body {
        match line {
            Line::Lyrics(lyrics) => render_lyrics(lyrics, transpose_offset, fmt_state, doc),
            Line::Comment(style, text) => render_comment(*style, text, doc),
            Line::Empty => doc.newline(LINE_GAP * 2.0),
            Line::Directive(d) if !d.kind.is_metadata() => render_directive(d, doc),
            _ => {}
        }
    }
}

fn render_comment(_style: CommentStyle, text: &str, doc: &mut PdfDocument) {
    let font = Font::HelveticaOblique;
    doc.ensure_space(COMMENT_SIZE + LINE_GAP);
    doc.text(text, font, COMMENT_SIZE);
    doc.newline(COMMENT_SIZE + LINE_GAP);
}

// ---------------------------------------------------------------------------
// Multi-page PDF document builder
// ---------------------------------------------------------------------------

/// Right margin in points.
const MARGIN_RIGHT: f32 = 56.0;
/// Gap between columns in points.
const COLUMN_GAP: f32 = 20.0;

// ---------------------------------------------------------------------------
// JPEG header parsing
// ---------------------------------------------------------------------------

/// Parse JPEG dimensions from raw file data by locating a SOF0 or SOF2 marker.
///
/// JPEG files consist of a sequence of markers. This function scans for
/// `0xFF 0xC0` (SOF0, baseline DCT) or `0xFF 0xC2` (SOF2, progressive DCT)
/// and reads the image height (2 bytes at marker+3) and width (2 bytes at
/// marker+5).
///
/// Returns `None` if the data is too short or no SOF marker is found.
fn parse_jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    // Minimum valid JPEG: SOI (2 bytes) + at least one marker segment
    if data.len() < 4 {
        return None;
    }
    // Verify JPEG SOI marker
    if data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }

    let mut i = 2;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            // Not a valid marker prefix — skip byte
            i += 1;
            continue;
        }
        let marker = data[i + 1];

        // Skip padding 0xFF bytes
        if marker == 0xFF {
            i += 1;
            continue;
        }

        // SOF0 (baseline) or SOF2 (progressive)
        if marker == 0xC0 || marker == 0xC2 {
            // Need at least 7 more bytes after the marker: length(2) + precision(1) + height(2) + width(2)
            if i + 9 > data.len() {
                return None;
            }
            let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            return Some((width, height));
        }

        // SOS marker — image data follows, no more headers to scan
        if marker == 0xDA {
            return None;
        }

        // Other markers: skip using the length field
        if i + 3 >= data.len() {
            return None;
        }
        let length = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        i += 2 + length;
    }

    None
}

/// Accumulates content across multiple pages and builds the final PDF.
///
/// Each page has its own content stream. When the Y cursor drops below the
/// bottom margin, content flows to the next column (if multi-column) or a
/// new page is started.
struct PdfDocument {
    /// Content-stream operations for each page.
    pages: Vec<Vec<String>>,
    /// Current Y position on the current page.
    y: f32,
    /// Number of columns (1 = single-column layout).
    num_columns: u32,
    /// Current column index (0-based).
    current_column: u32,
    /// Embedded JPEG images: (raw JPEG data, width in pixels, height in pixels).
    images: Vec<(Vec<u8>, u32, u32)>,
}

impl PdfDocument {
    /// Create a new document with one empty page.
    fn new() -> Self {
        Self {
            pages: vec![Vec::new()],
            y: PAGE_H - MARGIN_TOP,
            num_columns: 1,
            current_column: 0,
            images: Vec::new(),
        }
    }

    /// Returns the current Y position.
    fn y(&self) -> f32 {
        self.y
    }

    /// Returns the number of pages.
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Returns the left margin for the current column.
    fn margin_left(&self) -> f32 {
        if self.num_columns <= 1 {
            return MARGIN_LEFT;
        }
        let usable_width = PAGE_W - MARGIN_LEFT - MARGIN_RIGHT;
        let total_gaps = (self.num_columns - 1) as f32 * COLUMN_GAP;
        let col_width = (usable_width - total_gaps) / self.num_columns as f32;
        MARGIN_LEFT + self.current_column as f32 * (col_width + COLUMN_GAP)
    }

    /// Set the number of columns (clamped to 1..=[`MAX_COLUMNS`]). Resets to column 0.
    fn set_columns(&mut self, n: u32) {
        self.num_columns = n.clamp(1, MAX_COLUMNS);
        self.current_column = 0;
    }

    /// Force a column break. Advances to the next column, or to a new page
    /// if already in the last column.
    fn column_break(&mut self) {
        if self.num_columns <= 1 {
            self.new_page();
            return;
        }
        if self.current_column + 1 < self.num_columns {
            self.current_column += 1;
            self.y = PAGE_H - MARGIN_TOP;
        } else {
            self.new_page();
        }
    }

    /// Ensure there is at least `needed` points of vertical space remaining.
    /// If not, advance to next column or start a new page.
    fn ensure_space(&mut self, needed: f32) {
        if self.y - needed < MARGIN_BOTTOM {
            self.next_column_or_page();
        }
    }

    /// Advance to the next column, or to a new page if in the last column.
    fn next_column_or_page(&mut self) {
        if self.num_columns > 1 && self.current_column + 1 < self.num_columns {
            self.current_column += 1;
            self.y = PAGE_H - MARGIN_TOP;
        } else {
            self.new_page();
        }
    }

    /// Start a new page, resetting the Y cursor and column index.
    ///
    /// Silently does nothing when [`MAX_PAGES`] has been reached so that
    /// malicious input cannot cause unbounded memory allocation.
    fn new_page(&mut self) {
        if self.pages.len() >= MAX_PAGES {
            return;
        }
        self.pages.push(Vec::new());
        self.y = PAGE_H - MARGIN_TOP;
        self.current_column = 0;
    }

    /// Emit text at the current column margin and Y position.
    fn text(&mut self, text: &str, font: Font, size: f32) {
        let x = self.margin_left();
        self.text_at(text, font, size, x, self.y);
    }

    /// Emit text at an explicit (x, y) position.
    fn text_at(&mut self, text: &str, font: Font, size: f32, x: f32, y: f32) {
        let ops = self.current_page_mut();
        ops.push("BT".to_string());
        ops.push(format!("{} {} Tf", font.pdf_name(), fmt_f32(size)));
        ops.push(format!("{} {} Td", fmt_f32(x), fmt_f32(y)));
        ops.push(format!("({}) Tj", pdf_escape(text)));
        ops.push("ET".to_string());
    }

    /// Move the Y cursor down. May trigger a column/page break if past bottom margin.
    fn newline(&mut self, amount: f32) {
        self.y -= amount;
        if self.y < MARGIN_BOTTOM {
            self.next_column_or_page();
        }
    }

    /// Advance Y cursor without triggering auto page break.
    ///
    /// Used for intra-element positioning (e.g., chord row to lyrics row).
    fn advance_y(&mut self, amount: f32) {
        self.y -= amount;
    }

    /// Draw a line from (x1, y1) to (x2, y2) with the given width.
    fn line_at(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32) {
        let ops = self.current_page_mut();
        ops.push(format!("{} w", fmt_f32(width)));
        ops.push(format!(
            "{} {} m {} {} l S",
            fmt_f32(x1),
            fmt_f32(y1),
            fmt_f32(x2),
            fmt_f32(y2)
        ));
    }

    /// Draw a filled circle at (cx, cy) with the given radius.
    fn filled_circle_at(&mut self, cx: f32, cy: f32, r: f32) {
        // Approximate circle with 4 Bezier curves (kappa = 0.5523)
        let k = r * 0.5523;
        let ops = self.current_page_mut();
        ops.push(format!(
            "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c f",
            fmt_f32(cx + r), fmt_f32(cy),
            fmt_f32(cx + r), fmt_f32(cy + k), fmt_f32(cx + k), fmt_f32(cy + r), fmt_f32(cx), fmt_f32(cy + r),
            fmt_f32(cx - k), fmt_f32(cy + r), fmt_f32(cx - r), fmt_f32(cy + k), fmt_f32(cx - r), fmt_f32(cy),
            fmt_f32(cx - r), fmt_f32(cy - k), fmt_f32(cx - k), fmt_f32(cy - r), fmt_f32(cx), fmt_f32(cy - r),
            fmt_f32(cx + k), fmt_f32(cy - r), fmt_f32(cx + r), fmt_f32(cy - k), fmt_f32(cx + r), fmt_f32(cy),
        ));
    }

    /// Draw a stroked (unfilled) circle at (cx, cy) with the given radius.
    fn stroked_circle_at(&mut self, cx: f32, cy: f32, r: f32) {
        let k = r * 0.5523;
        let ops = self.current_page_mut();
        ops.push("0.5 w".to_string());
        ops.push(format!(
            "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c S",
            fmt_f32(cx + r), fmt_f32(cy),
            fmt_f32(cx + r), fmt_f32(cy + k), fmt_f32(cx + k), fmt_f32(cy + r), fmt_f32(cx), fmt_f32(cy + r),
            fmt_f32(cx - k), fmt_f32(cy + r), fmt_f32(cx - r), fmt_f32(cy + k), fmt_f32(cx - r), fmt_f32(cy),
            fmt_f32(cx - r), fmt_f32(cy - k), fmt_f32(cx - k), fmt_f32(cy - r), fmt_f32(cx), fmt_f32(cy - r),
            fmt_f32(cx + k), fmt_f32(cy - r), fmt_f32(cx + r), fmt_f32(cy - k), fmt_f32(cx + r), fmt_f32(cy),
        ));
    }

    /// Store a JPEG image and return its index for later drawing.
    ///
    /// The raw JPEG bytes are stored as-is; the PDF will use `/DCTDecode`
    /// (JPEG passthrough) so no re-encoding is needed.
    fn embed_jpeg(&mut self, data: Vec<u8>, width: u32, height: u32) -> usize {
        let idx = self.images.len();
        self.images.push((data, width, height));
        idx
    }

    /// Draw a previously embedded image at the given position and size.
    ///
    /// Emits PDF `cm` (concat matrix) and `Do` (paint XObject) operators
    /// wrapped in `q`/`Q` (save/restore graphics state).
    fn draw_image(&mut self, img_idx: usize, x: f32, y: f32, w: f32, h: f32) {
        let name = format!("/Im{}", img_idx + 1);
        let ops = self.current_page_mut();
        ops.push("q".to_string());
        ops.push(format!(
            "{} 0 0 {} {} {} cm",
            fmt_f32(w),
            fmt_f32(h),
            fmt_f32(x),
            fmt_f32(y)
        ));
        ops.push(format!("{name} Do"));
        ops.push("Q".to_string());
    }

    /// Returns a mutable reference to the current page's operations.
    fn current_page_mut(&mut self) -> &mut Vec<String> {
        // pages always has at least one element (initialized in new())
        self.pages.last_mut().expect("pages is never empty")
    }

    /// Take all pages out of this document, replacing them with a single
    /// empty page so that the "pages is never empty" invariant is preserved.
    fn take_pages(&mut self) -> Vec<Vec<String>> {
        let pages = std::mem::take(&mut self.pages);
        self.pages.push(Vec::new());
        self.y = PAGE_H - MARGIN_TOP;
        self.current_column = 0;
        pages
    }

    /// Append a pre-built page to this document.
    ///
    /// Silently drops the page when [`MAX_PAGES`] has been reached, consistent
    /// with [`new_page`](Self::new_page).
    fn push_page(&mut self, ops: Vec<String>) {
        if self.pages.len() >= MAX_PAGES {
            return;
        }
        self.pages.push(ops);
    }

    /// Build the complete multi-page PDF document.
    fn build_pdf(&self) -> Vec<u8> {
        let num_pages = self.pages.len();
        let num_images = self.images.len();
        let mut offsets: Vec<usize> = Vec::new();
        let mut pdf = Vec::<u8>::new();

        // Header
        pdf.extend_from_slice(b"%PDF-1.4\n");

        // Object 1: Catalog
        offsets.push(pdf.len());
        pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

        // Object 2: Pages (parent of all page objects)
        offsets.push(pdf.len());
        let font_refs: String = FONTS
            .iter()
            .enumerate()
            .map(|(i, _)| format!("{} {} 0 R", FONTS[i].pdf_name(), i + 3))
            .collect::<Vec<_>>()
            .join(" ");

        // Image XObject references for the Resources dict
        let image_obj_base = 3 + FONTS.len(); // first image object number
        let xobject_refs = if num_images > 0 {
            let refs: String = (0..num_images)
                .map(|i| format!("/Im{} {} 0 R", i + 1, image_obj_base + i))
                .collect::<Vec<_>>()
                .join(" ");
            format!(" /XObject << {refs} >>")
        } else {
            String::new()
        };

        let procset = if num_images > 0 {
            "/ProcSet [/PDF /Text /ImageB /ImageC]"
        } else {
            "/ProcSet [/PDF /Text]"
        };

        // Kids: page objects start after fonts + image XObjects
        let page_obj_start = 3 + FONTS.len() + num_images;
        let kids: String = (0..num_pages)
            .map(|i| format!("{} 0 R", page_obj_start + i * 2))
            .collect::<Vec<_>>()
            .join(" ");
        let obj2 = format!(
            "2 0 obj\n<< /Type /Pages /MediaBox [0 0 {} {}] /Resources << /Font << {} >>{} {} >> /Kids [{}] /Count {} >>\nendobj\n",
            fmt_f32(PAGE_W),
            fmt_f32(PAGE_H),
            font_refs,
            xobject_refs,
            procset,
            kids,
            num_pages
        );
        pdf.extend_from_slice(obj2.as_bytes());

        // Font dictionaries: objects 3 .. 3+FONTS.len()-1
        for font in &FONTS {
            offsets.push(pdf.len());
            let obj_num = offsets.len();
            let obj = format!(
                "{} 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>\nendobj\n",
                obj_num,
                font.base_name()
            );
            pdf.extend_from_slice(obj.as_bytes());
        }

        // Image XObject streams (JPEG passthrough via /DCTDecode)
        for (img_data, img_w, img_h) in &self.images {
            offsets.push(pdf.len());
            let obj_num = offsets.len();
            let header = format!(
                "{} 0 obj\n<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
                obj_num,
                img_w,
                img_h,
                img_data.len()
            );
            pdf.extend_from_slice(header.as_bytes());
            pdf.extend_from_slice(img_data);
            pdf.extend_from_slice(b"\nendstream\nendobj\n");
        }

        // Page + content stream pairs
        for (i, page_ops) in self.pages.iter().enumerate() {
            let page_obj_num = page_obj_start + i * 2;
            let content_obj_num = page_obj_num + 1;

            // Page object
            offsets.push(pdf.len());
            let page_obj = format!(
                "{} 0 obj\n<< /Type /Page /Parent 2 0 R /Contents {} 0 R >>\nendobj\n",
                page_obj_num, content_obj_num
            );
            pdf.extend_from_slice(page_obj.as_bytes());

            // Content stream
            let content = page_ops.join("\n");
            offsets.push(pdf.len());
            let stream_obj = format!(
                "{} 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
                content_obj_num,
                content.len() + 1, // +1 for trailing newline in stream
                content
            );
            pdf.extend_from_slice(stream_obj.as_bytes());
        }

        // Cross-reference table
        let xref_offset = pdf.len();
        let num_objects = offsets.len() + 1; // +1 for object 0
        pdf.extend_from_slice(format!("xref\n0 {num_objects}\n").as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in &offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }

        // Trailer
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {num_objects} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
            )
            .as_bytes(),
        );

        pdf
    }
}

// ---------------------------------------------------------------------------
// Font enum
// ---------------------------------------------------------------------------

/// Built-in PDF Type1 fonts (available in all conforming PDF readers).
#[derive(Clone, Copy)]
enum Font {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
}

impl Font {
    /// Returns the PDF font resource name (must match the page Resources dict).
    fn pdf_name(self) -> &'static str {
        match self {
            Self::Helvetica => "/F1",
            Self::HelveticaBold => "/F2",
            Self::HelveticaOblique => "/F3",
            Self::HelveticaBoldOblique => "/F4",
        }
    }

    /// Returns the PDF BaseFont name for the font dictionary.
    fn base_name(self) -> &'static str {
        match self {
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::HelveticaOblique => "Helvetica-Oblique",
            Self::HelveticaBoldOblique => "Helvetica-BoldOblique",
        }
    }
}

/// The four fonts used in the output.
const FONTS: [Font; 4] = [
    Font::Helvetica,
    Font::HelveticaBold,
    Font::HelveticaOblique,
    Font::HelveticaBoldOblique,
];

// ---------------------------------------------------------------------------
// PDF helpers
// ---------------------------------------------------------------------------

/// Escape a string for inclusion in a PDF literal string `(...)`.
fn pdf_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            _ if c.is_ascii() => out.push(c),
            // Non-ASCII: replace with '?' for Type1 font compatibility.
            _ => out.push('?'),
        }
    }
    out
}

/// Format f32 without trailing zeros for compact PDF output.
fn fmt_f32(v: f32) -> String {
    let s = format!("{v:.2}");
    // Trim trailing zeros after decimal point.
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_produces_valid_pdf() {
        let song = chordpro_core::parse("{title: Test}\n[Am]Hello [G]world").unwrap();
        let bytes = render_song(&song);
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn test_empty_song() {
        let song = chordpro_core::parse("").unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_try_render_success() {
        let result = try_render("{title: Test}\n[G]Hello");
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(b"%PDF"));
    }

    #[test]
    fn test_try_render_error() {
        let result = try_render("{unclosed");
        assert!(result.is_err());
    }

    #[test]
    fn test_full_song() {
        let input = "\
{title: Amazing Grace}
{subtitle: Traditional}

{start_of_verse}
[G]Amazing [G7]grace
{end_of_verse}

{comment: Repeat}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        // Should contain the title text in the content stream
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Amazing Grace"));
    }

    #[test]
    fn test_pdf_escape() {
        assert_eq!(pdf_escape("hello"), "hello");
        assert_eq!(pdf_escape("a(b)c"), "a\\(b\\)c");
        assert_eq!(pdf_escape("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_render_grid_section() {
        let input = "{start_of_grid}\n| Am . | C . |\n{end_of_grid}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Grid"));
    }

    // --- Custom sections (#108) ---

    #[test]
    fn test_custom_section_in_pdf() {
        let input = "\
{title: Test}

{start_of_intro: Guitar}
[Am]Intro line
{end_of_intro}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Intro: Guitar"));
    }

    // --- Issue #109: {chorus} recall ---

    #[test]
    fn test_chorus_recall_produces_valid_pdf() {
        let input = "\
{start_of_chorus}
[G]La la la
{end_of_chorus}

{chorus}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        // "Chorus" label should appear at least twice in the content stream
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.matches("Chorus").count() >= 2);
    }

    #[test]
    fn test_chorus_recall_with_label() {
        let input = "\
{start_of_chorus}
Sing along
{end_of_chorus}

{chorus: Repeat}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Chorus: Repeat"));
    }

    #[test]
    fn test_chorus_recall_no_chorus_defined() {
        let input = "{chorus}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Chorus"));
    }

    #[test]
    fn test_custom_section_solo_in_pdf() {
        let input = "{start_of_solo}\n[Em]Solo\n{end_of_solo}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Solo"));
    }

    #[test]
    fn test_render_grid_section_with_label() {
        let input = "{start_of_grid: Intro}\n| Am |\n{end_of_grid}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Grid: Intro"));
    }
}

#[cfg(test)]
mod transpose_tests {
    use super::*;

    #[test]
    fn test_transpose_directive_produces_pdf() {
        let input = "{transpose: 2}\n[G]Hello [C]world";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        // Transposed chords should appear in the PDF content: G+2=A, C+2=D
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("(A)"));
        assert!(content.contains("(D)"));
    }

    #[test]
    fn test_transpose_with_cli_offset() {
        let input = "{transpose: 2}\n[C]Hello";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song_with_transpose(&song, 3);
        // 2+3=5, C+5=F
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("(F)"));
    }
}

#[cfg(test)]
mod delegate_tests {
    use super::*;

    #[test]
    fn test_abc_section_in_pdf() {
        let input = "{start_of_abc: Melody}\nX:1\n{end_of_abc}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("ABC: Melody"));
    }

    #[test]
    fn test_ly_section_in_pdf() {
        let input = "{start_of_ly}\nnotes\n{end_of_ly}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Lilypond"));
    }

    #[test]
    fn test_svg_section_in_pdf() {
        let input = "{start_of_svg}\n<svg/>\n{end_of_svg}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("SVG"));
    }

    #[test]
    fn test_textblock_section_in_pdf() {
        let input = "{start_of_textblock}\nText\n{end_of_textblock}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Textblock"));
    }
}

#[cfg(test)]
mod inline_markup_tests {
    use super::*;

    #[test]
    fn test_bold_markup_uses_bold_font() {
        let input = "Hello <b>bold</b> world";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // PDF should contain both Helvetica (regular) and HelveticaBold
        assert!(content.contains("/F1"));
        assert!(content.contains("/F2"));
        assert!(content.contains("bold"));
    }

    #[test]
    fn test_italic_markup_uses_oblique_font() {
        let input = "Hello <i>italic</i> text";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/F3")); // HelveticaOblique
        assert!(content.contains("italic"));
    }

    #[test]
    fn test_bold_italic_markup_uses_bold_oblique_font() {
        let input = "<b><i>bold italic</i></b>";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/F4")); // HelveticaBoldOblique
        assert!(content.contains("bold italic"));
    }

    #[test]
    fn test_markup_with_chords_produces_valid_pdf() {
        let input = "[Am]Hello <b>bold</b> world";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Am"));
        assert!(content.contains("bold"));
    }

    #[test]
    fn test_span_weight_bold_uses_bold_font() {
        let input = r#"<span weight="bold">weighted</span>"#;
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/F2")); // HelveticaBold
        assert!(content.contains("weighted"));
    }
}

#[cfg(test)]
mod formatting_directive_tests {
    use super::*;

    #[test]
    fn test_textsize_directive_changes_font_size() {
        let input = "{textsize: 14}\nHello world";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // The PDF should use 14pt for lyrics text
        assert!(content.contains("14"));
        assert!(content.contains("Hello world"));
    }

    #[test]
    fn test_chordsize_directive_changes_chord_size() {
        let input = "{chordsize: 16}\n[Am]Hello";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Am"));
    }

    #[test]
    fn test_formatting_directive_produces_valid_pdf() {
        let input = "{textsize: 14}\n{chordsize: 12}\n[Am]Hello <b>bold</b> world";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_textsize_clamped_to_max() {
        let input = "{textsize: 99999}\nHello";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // Font size must be clamped to MAX_FONT_SIZE (200), not 99999.
        assert!(!content.contains("99999"));
        assert!(content.contains("200"));
    }

    #[test]
    fn test_textsize_clamped_to_min() {
        let input = "{textsize: -5}\nHello";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // Negative size must be clamped to MIN_FONT_SIZE (0.5).
        assert!(content.contains("0.5"));
    }

    #[test]
    fn test_chordsize_clamped_to_max() {
        let input = "{chordsize: 500}\n[Am]Hello";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(!content.contains("500"));
        assert!(content.contains("200"));
    }
}

#[cfg(test)]
mod multipage_tests {
    use super::*;

    #[test]
    fn test_new_page_directive_creates_two_pages() {
        let input = "{title: Test}\nPage one\n{new_page}\nPage two";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        // Should have /Count 2 in the Pages object
        assert!(content.contains("/Count 2"));
        assert!(content.contains("Page one"));
        assert!(content.contains("Page two"));
    }

    #[test]
    fn test_new_physical_page_directive_creates_two_pages() {
        let input = "Page one\n{new_physical_page}\nPage two";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/Count 2"));
    }

    #[test]
    fn test_single_page_has_count_one() {
        let input = "{title: Short Song}\n[Am]Hello";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/Count 1"));
    }

    #[test]
    fn test_automatic_page_break_for_long_content() {
        // Generate enough lines to overflow a single A4 page
        let mut lines = vec!["{title: Long Song}".to_string()];
        for i in 0..80 {
            lines.push(format!("[Am]Line number {i}"));
        }
        let input = lines.join("\n");
        let song = chordpro_core::parse(&input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // Should have more than one page
        assert!(
            !content.contains("/Count 1"),
            "80 chord-lyrics lines should overflow one page"
        );
    }

    #[test]
    fn test_multiple_new_page_directives() {
        let input = "Page 1\n{new_page}\nPage 2\n{new_page}\nPage 3";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/Count 3"));
    }

    #[test]
    fn test_multipage_pdf_structure_valid() {
        let input = "First page\n{new_page}\nSecond page";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        // Verify both pages have content
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("First page"));
        assert!(content.contains("Second page"));
    }

    #[test]
    fn test_page_count_method() {
        let mut doc = PdfDocument::new();
        assert_eq!(doc.page_count(), 1);
        doc.new_page();
        assert_eq!(doc.page_count(), 2);
        doc.new_page();
        assert_eq!(doc.page_count(), 3);
    }

    #[test]
    fn test_new_page_respects_max_limit() {
        let mut doc = PdfDocument::new();
        // Already has 1 page; add MAX_PAGES more attempts.
        for _ in 0..MAX_PAGES {
            doc.new_page();
        }
        assert_eq!(doc.page_count(), MAX_PAGES);
    }

    #[test]
    fn test_take_pages_preserves_invariant() {
        let mut doc = PdfDocument::new();
        doc.new_page();
        assert_eq!(doc.page_count(), 2);

        let taken = doc.take_pages();
        assert_eq!(taken.len(), 2);
        // Document is usable after take — invariant preserved.
        assert_eq!(doc.page_count(), 1);
        // current_page_mut must not panic.
        let _ = doc.current_page_mut();
    }

    #[test]
    fn test_new_page_works_after_take_pages() {
        let mut doc = PdfDocument::new();
        let _ = doc.take_pages();
        doc.new_page();
        assert_eq!(doc.page_count(), 2);
    }

    #[test]
    fn test_push_page_respects_max_limit() {
        let mut doc = PdfDocument::new();
        // Fill to MAX_PAGES via new_page.
        for _ in 1..MAX_PAGES {
            doc.new_page();
        }
        assert_eq!(doc.page_count(), MAX_PAGES);

        // push_page should be silently dropped.
        doc.push_page(vec!["BT (overflow) Tj ET".to_string()]);
        assert_eq!(doc.page_count(), MAX_PAGES);
    }

    #[test]
    fn test_combined_toc_and_body_respects_max_limit() {
        let mut toc_doc = PdfDocument::new();
        // Simulate ToC with a few pages.
        for _ in 1..5 {
            toc_doc.new_page();
        }
        assert_eq!(toc_doc.page_count(), 5);

        let mut body_doc = PdfDocument::new();
        // Fill body to MAX_PAGES.
        for _ in 1..MAX_PAGES {
            body_doc.new_page();
        }
        assert_eq!(body_doc.page_count(), MAX_PAGES);

        // Combine: push body pages into toc_doc.
        for page_ops in body_doc.take_pages() {
            toc_doc.push_page(page_ops);
        }
        // Combined must not exceed MAX_PAGES.
        assert_eq!(toc_doc.page_count(), MAX_PAGES);
    }
}

#[cfg(test)]
mod column_tests {
    use super::*;

    #[test]
    fn test_columns_directive_produces_valid_pdf() {
        let input = "{columns: 2}\nColumn one\n{column_break}\nColumn two";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Column one"));
        assert!(content.contains("Column two"));
    }

    #[test]
    fn test_column_break_in_single_column_creates_new_page() {
        let input = "Page one\n{column_break}\nPage two";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/Count 2"));
    }

    #[test]
    fn test_columns_reset_to_one() {
        let input = "{columns: 2}\nTwo cols\n{columns: 1}\nOne col";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Two cols"));
        assert!(content.contains("One col"));
    }

    #[test]
    fn test_margin_left_single_column() {
        let doc = PdfDocument::new();
        assert!((doc.margin_left() - MARGIN_LEFT).abs() < 0.01);
    }

    #[test]
    fn test_margin_left_multi_column() {
        let mut doc = PdfDocument::new();
        doc.set_columns(2);
        // First column should start at MARGIN_LEFT
        assert!((doc.margin_left() - MARGIN_LEFT).abs() < 0.01);
        // Second column should be offset
        doc.current_column = 1;
        assert!(doc.margin_left() > MARGIN_LEFT);
    }

    #[test]
    fn test_column_break_advances_column() {
        let mut doc = PdfDocument::new();
        doc.set_columns(2);
        assert_eq!(doc.current_column, 0);
        doc.column_break();
        assert_eq!(doc.current_column, 1);
    }

    #[test]
    fn test_set_columns_clamps_to_max() {
        let mut doc = PdfDocument::new();
        doc.set_columns(999);
        assert_eq!(doc.num_columns, MAX_COLUMNS);
    }

    #[test]
    fn test_set_columns_clamps_zero_to_one() {
        let mut doc = PdfDocument::new();
        doc.set_columns(0);
        assert_eq!(doc.num_columns, 1);
    }

    #[test]
    fn test_margin_left_at_max_columns_no_overflow() {
        let mut doc = PdfDocument::new();
        doc.set_columns(MAX_COLUMNS);
        // Verify margin_left produces a finite, non-negative value for every column.
        for col in 0..MAX_COLUMNS {
            doc.current_column = col;
            let m = doc.margin_left();
            assert!(m.is_finite(), "margin_left must be finite for column {col}");
            assert!(
                m >= 0.0,
                "margin_left must be non-negative for column {col}"
            );
        }
    }

    #[test]
    fn test_column_break_last_column_new_page() {
        let mut doc = PdfDocument::new();
        doc.set_columns(2);
        doc.column_break(); // → column 1
        assert_eq!(doc.page_count(), 1);
        doc.column_break(); // → new page, column 0
        assert_eq!(doc.page_count(), 2);
        assert_eq!(doc.current_column, 0);
    }

    // --- Multi-song rendering ---

    #[test]
    fn test_render_songs_single() {
        let songs = chordpro_core::parse_multi("{title: Only}\n[Am]Hello").unwrap();
        let bytes = render_songs(&songs);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        // Single song: output should match render_song
        assert_eq!(bytes, render_song(&songs[0]));
    }

    #[test]
    fn test_render_songs_two_songs_multi_page() {
        let songs = chordpro_core::parse_multi(
            "{title: Song A}\n[Am]Hello\n{new_song}\n{title: Song B}\n[G]World",
        )
        .unwrap();
        let bytes = render_songs(&songs);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        let content = String::from_utf8_lossy(&bytes);
        // Both songs should be present
        assert!(content.contains("Song A"));
        assert!(content.contains("Song B"));
        // ToC page + 2 song pages = 3 pages
        assert!(content.contains("/Count 3"));
        // Should contain "Table of Contents"
        assert!(content.contains("Table of Contents"));
    }

    #[test]
    fn test_render_songs_with_transpose() {
        let songs =
            chordpro_core::parse_multi("{title: S1}\n[C]Do\n{new_song}\n{title: S2}\n[G]Re")
                .unwrap();
        let bytes = render_songs_with_transpose(&songs, 2);
        let content = String::from_utf8_lossy(&bytes);
        // C+2=D, G+2=A — both transposed chords should appear
        assert!(content.contains("(D)"));
        assert!(content.contains("(A)"));
    }

    #[test]
    fn test_render_song_into_doc_helper() {
        let song = chordpro_core::parse("{title: Test}\n[Am]Hello").unwrap();
        let mut doc = PdfDocument::new();
        render_song_into_doc(&song, 0, &mut doc);
        // Document should have 1 page with content
        assert_eq!(doc.page_count(), 1);
        let pdf = doc.build_pdf();
        assert!(pdf.starts_with(b"%PDF-1.4"));
        let content = String::from_utf8_lossy(&pdf);
        assert!(content.contains("Test"));
    }
}

#[cfg(test)]
mod toc_tests {
    use super::*;

    #[test]
    fn test_toc_generated_for_multi_song() {
        let songs = chordpro_core::parse_multi(
            "{title: First}\nLyrics 1\n{new_song}\n{title: Second}\nLyrics 2",
        )
        .unwrap();
        let bytes = render_songs(&songs);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Table of Contents"));
        assert!(content.contains("First"));
        assert!(content.contains("Second"));
    }

    #[test]
    fn test_toc_not_generated_for_single_song() {
        let song = chordpro_core::parse("{title: Only Song}\nLyrics").unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(!content.contains("Table of Contents"));
    }

    #[test]
    fn test_toc_page_numbers_present() {
        let songs = chordpro_core::parse_multi(
            "{title: Song A}\nA\n{new_song}\n{title: Song B}\nB\n{new_song}\n{title: Song C}\nC",
        )
        .unwrap();
        let bytes = render_songs(&songs);
        let content = String::from_utf8_lossy(&bytes);
        // ToC is 1 page, then Song A=page 2, Song B=page 3, Song C=page 4
        assert!(content.contains("/Count 4"));
        assert!(content.contains("Table of Contents"));
    }

    #[test]
    fn test_toc_valid_pdf_structure() {
        let songs =
            chordpro_core::parse_multi("{title: A}\nText\n{new_song}\n{title: B}\nText").unwrap();
        let bytes = render_songs(&songs);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }
}

#[cfg(test)]
mod chord_diagram_pdf_tests {
    use super::*;

    #[test]
    fn test_define_renders_diagram_in_pdf() {
        let input = "{define: Am base-fret 1 frets x 0 2 2 1 0}\n[Am]Hello";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        // Should contain the chord name
        assert!(content.contains("Am"));
        // Should contain circle drawing operations (Bezier curves)
        assert!(content.contains(" c "));
    }

    #[test]
    fn test_define_keyboard_no_diagram_in_pdf() {
        let input = "{define: Am keys 0 3 7}\n[Am]Hello";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        // Should still be valid
    }

    #[test]
    fn test_define_diagram_valid_pdf() {
        let input = "{define: F base-fret 1 frets 1 1 2 3 3 1}\n[F]Lyrics";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }
}

#[cfg(test)]
mod jpeg_tests {
    use super::*;

    /// Build a minimal valid JPEG byte sequence with a SOF0 marker.
    ///
    /// This is not a displayable image but contains a structurally correct
    /// JPEG header that `parse_jpeg_dimensions` can parse.
    fn minimal_jpeg(width: u16, height: u16) -> Vec<u8> {
        let mut data = Vec::new();
        // SOI marker
        data.extend_from_slice(&[0xFF, 0xD8]);
        // APP0 marker (minimal, 2-byte length = 2 means no payload beyond length)
        data.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x02]);
        // SOF0 marker
        data.extend_from_slice(&[0xFF, 0xC0]);
        // Length: 8 bytes (length field itself + precision + height + width + components)
        data.extend_from_slice(&[0x00, 0x08]);
        // Precision: 8 bits
        data.push(0x08);
        // Height (big-endian)
        data.extend_from_slice(&height.to_be_bytes());
        // Width (big-endian)
        data.extend_from_slice(&width.to_be_bytes());
        // Number of components: 0 (invalid for real JPEG, but sufficient for parsing)
        data.push(0x00);
        data
    }

    #[test]
    fn test_parse_jpeg_dimensions_basic() {
        let jpeg = minimal_jpeg(640, 480);
        let dims = parse_jpeg_dimensions(&jpeg);
        assert_eq!(dims, Some((640, 480)));
    }

    #[test]
    fn test_parse_jpeg_dimensions_square() {
        let jpeg = minimal_jpeg(100, 100);
        let dims = parse_jpeg_dimensions(&jpeg);
        assert_eq!(dims, Some((100, 100)));
    }

    #[test]
    fn test_parse_jpeg_dimensions_too_short() {
        assert_eq!(parse_jpeg_dimensions(&[0xFF]), None);
        assert_eq!(parse_jpeg_dimensions(&[]), None);
    }

    #[test]
    fn test_parse_jpeg_dimensions_not_jpeg() {
        // PNG signature
        assert_eq!(
            parse_jpeg_dimensions(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A]),
            None
        );
    }

    #[test]
    fn test_parse_jpeg_dimensions_sof2_progressive() {
        let mut data = Vec::new();
        // SOI
        data.extend_from_slice(&[0xFF, 0xD8]);
        // APP0 with minimal length
        data.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x02]);
        // SOF2 (progressive DCT)
        data.extend_from_slice(&[0xFF, 0xC2]);
        data.extend_from_slice(&[0x00, 0x08]);
        data.push(0x08);
        data.extend_from_slice(&300_u16.to_be_bytes()); // height
        data.extend_from_slice(&400_u16.to_be_bytes()); // width
        data.push(0x00);
        let dims = parse_jpeg_dimensions(&data);
        assert_eq!(dims, Some((400, 300)));
    }

    #[test]
    fn test_image_directive_nonexistent_file_no_crash() {
        let input = "{image: src=nonexistent_file_that_does_not_exist.jpg}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        // Should produce a valid PDF without crashing
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn test_image_directive_non_jpeg_skipped() {
        let input = "{image: src=photo.png}";
        let song = chordpro_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn test_embed_jpeg_produces_xobject() {
        let jpeg = minimal_jpeg(320, 240);
        let mut doc = PdfDocument::new();
        let idx = doc.embed_jpeg(jpeg, 320, 240);
        assert_eq!(idx, 0);
        doc.draw_image(idx, 56.0, 700.0, 320.0, 240.0);
        let pdf = doc.build_pdf();
        let content = String::from_utf8_lossy(&pdf);
        // The PDF should contain image XObject references
        assert!(content.contains("/XObject"));
        assert!(content.contains("/Im1"));
        assert!(content.contains("/DCTDecode"));
        assert!(content.contains("/Subtype /Image"));
    }

    #[test]
    fn test_embed_multiple_jpegs() {
        let jpeg1 = minimal_jpeg(100, 50);
        let jpeg2 = minimal_jpeg(200, 150);
        let mut doc = PdfDocument::new();
        let idx1 = doc.embed_jpeg(jpeg1, 100, 50);
        let idx2 = doc.embed_jpeg(jpeg2, 200, 150);
        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        doc.draw_image(idx1, 56.0, 700.0, 100.0, 50.0);
        doc.draw_image(idx2, 56.0, 600.0, 200.0, 150.0);
        let pdf = doc.build_pdf();
        let content = String::from_utf8_lossy(&pdf);
        assert!(content.contains("/Im1"));
        assert!(content.contains("/Im2"));
    }

    #[test]
    fn test_no_images_no_xobject_dict() {
        let doc = PdfDocument::new();
        let pdf = doc.build_pdf();
        let content = String::from_utf8_lossy(&pdf);
        // Without images, there should be no XObject dictionary
        assert!(!content.contains("/XObject"));
    }

    #[test]
    fn test_draw_image_emits_cm_do_operators() {
        let jpeg = minimal_jpeg(50, 50);
        let mut doc = PdfDocument::new();
        let idx = doc.embed_jpeg(jpeg, 50, 50);
        doc.draw_image(idx, 100.0, 200.0, 50.0, 50.0);
        let ops = &doc.pages[0];
        assert!(ops.iter().any(|op| op == "q"));
        assert!(ops.iter().any(|op| op.contains("cm")));
        assert!(ops.iter().any(|op| op.contains("/Im1 Do")));
        assert!(ops.iter().any(|op| op == "Q"));
    }

    #[test]
    fn test_compute_image_dimensions_explicit_width() {
        let attrs = ImageAttributes {
            src: "test.jpg".to_string(),
            width: Some("200".to_string()),
            height: None,
            scale: None,
            title: None,
            anchor: None,
        };
        let (w, h) = compute_image_dimensions(&attrs, 400.0, 300.0, 400.0 / 300.0);
        assert!((w - 200.0).abs() < 0.01);
        assert!((h - 150.0).abs() < 0.01); // preserves aspect ratio
    }

    #[test]
    fn test_compute_image_dimensions_explicit_height() {
        let attrs = ImageAttributes {
            src: "test.jpg".to_string(),
            width: None,
            height: Some("100".to_string()),
            scale: None,
            title: None,
            anchor: None,
        };
        let (w, h) = compute_image_dimensions(&attrs, 400.0, 200.0, 2.0);
        assert!((w - 200.0).abs() < 0.01);
        assert!((h - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_image_dimensions_scale() {
        let attrs = ImageAttributes {
            src: "test.jpg".to_string(),
            width: None,
            height: None,
            scale: Some("0.5".to_string()),
            title: None,
            anchor: None,
        };
        let (w, h) = compute_image_dimensions(&attrs, 400.0, 300.0, 400.0 / 300.0);
        assert!((w - 200.0).abs() < 0.01);
        assert!((h - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_image_dimensions_native() {
        let attrs = ImageAttributes::default();
        let (w, h) = compute_image_dimensions(&attrs, 800.0, 600.0, 800.0 / 600.0);
        assert!((w - 800.0).abs() < 0.01);
        assert!((h - 600.0).abs() < 0.01);
    }
}
