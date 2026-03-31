//! PDF renderer for ChordPro documents.
//!
//! Converts a parsed ChordPro AST into a PDF document using built-in PDF
//! Type1 fonts (Helvetica family). No external dependencies — the PDF is
//! generated directly from raw PDF object structures.
//!
//! Supports multi-page output: content automatically flows to new pages when
//! the current page overflows, and `{new_page}` / `{new_physical_page}`
//! directives trigger explicit page breaks.

use chordpro_core::ast::{CommentStyle, DirectiveKind, Line, LyricsLine, Song};
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
        let size_val = value.as_deref().and_then(|v| v.parse::<f32>().ok());
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
                    DirectiveKind::NewPage | DirectiveKind::NewPhysicalPage => {
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
        DirectiveKind::StartOfSection(section_name) => Some(capitalize(section_name)),
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

/// Capitalize the first character of a string.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

fn render_directive(directive: &chordpro_core::ast::Directive, doc: &mut PdfDocument) {
    render_section_label(directive, doc);
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
}

impl PdfDocument {
    /// Create a new document with one empty page.
    fn new() -> Self {
        Self {
            pages: vec![Vec::new()],
            y: PAGE_H - MARGIN_TOP,
            num_columns: 1,
            current_column: 0,
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

    /// Set the number of columns. Resets to column 0.
    fn set_columns(&mut self, n: u32) {
        self.num_columns = n.max(1);
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
    fn new_page(&mut self) {
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

    /// Returns a mutable reference to the current page's operations.
    fn current_page_mut(&mut self) -> &mut Vec<String> {
        // pages always has at least one element (initialized in new())
        self.pages.last_mut().expect("pages is never empty")
    }

    /// Take all pages out of this document, leaving it empty.
    fn take_pages(&mut self) -> Vec<Vec<String>> {
        std::mem::take(&mut self.pages)
    }

    /// Append a pre-built page to this document.
    fn push_page(&mut self, ops: Vec<String>) {
        self.pages.push(ops);
    }

    /// Build the complete multi-page PDF document.
    fn build_pdf(&self) -> Vec<u8> {
        let num_pages = self.pages.len();
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
        // Kids: page objects start at object 3+FONTS.len()
        let page_obj_start = 3 + FONTS.len();
        let kids: String = (0..num_pages)
            .map(|i| format!("{} 0 R", page_obj_start + i * 2))
            .collect::<Vec<_>>()
            .join(" ");
        let obj2 = format!(
            "2 0 obj\n<< /Type /Pages /MediaBox [0 0 {} {}] /Resources << /Font << {} >> /ProcSet [/PDF /Text] >> /Kids [{}] /Count {} >>\nendobj\n",
            fmt_f32(PAGE_W),
            fmt_f32(PAGE_H),
            font_refs,
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
