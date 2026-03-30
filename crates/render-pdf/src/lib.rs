//! PDF renderer for ChordPro documents.
//!
//! Converts a parsed ChordPro AST into a PDF document using built-in PDF
//! Type1 fonts (Helvetica family). No external dependencies — the PDF is
//! generated directly from raw PDF object structures.

use chordpro_core::ast::{CommentStyle, DirectiveKind, Line, LyricsLine, Song};
use chordpro_core::transpose::transpose_chord;

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
    let mut page = PageBuilder::new();
    let mut transpose_offset: i8 = cli_transpose;

    // Title
    if let Some(title) = &song.metadata.title {
        page.text(title, Font::HelveticaBold, TITLE_SIZE);
        page.newline(TITLE_SIZE + LINE_GAP);
    }
    // Subtitles
    for subtitle in &song.metadata.subtitles {
        page.text(subtitle, Font::Helvetica, SUBTITLE_SIZE);
        page.newline(SUBTITLE_SIZE + LINE_GAP);
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
                render_lyrics(lyrics, transpose_offset, &mut page);
            }
            Line::Directive(d) if !d.kind.is_metadata() => {
                if d.kind == DirectiveKind::Transpose {
                    let file_offset: i8 =
                        d.value.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0);
                    transpose_offset = file_offset.saturating_add(cli_transpose);
                    continue;
                }
                match &d.kind {
                    DirectiveKind::StartOfChorus => {
                        render_section_label(d, &mut page);
                        chorus_buf = Some(Vec::new());
                    }
                    DirectiveKind::EndOfChorus => {
                        if let Some(buf) = chorus_buf.take() {
                            chorus_body = buf;
                        }
                    }
                    DirectiveKind::Chorus => {
                        render_chorus_recall(&d.value, &chorus_body, transpose_offset, &mut page);
                    }
                    _ => {
                        if let Some(buf) = chorus_buf.as_mut() {
                            buf.push(line.clone());
                        }
                        render_directive(d, &mut page);
                    }
                }
            }
            Line::Comment(style, text) => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                render_comment(*style, text, &mut page);
            }
            Line::Empty => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                page.newline(LINE_GAP * 2.0);
            }
            _ => {}
        }
    }

    build_pdf(&page.ops)
}

/// Parse and render a ChordPro source string to PDF bytes.
pub fn try_render(input: &str) -> Result<Vec<u8>, chordpro_core::ParseError> {
    let song = chordpro_core::parse(input)?;
    Ok(render_song(&song))
}

// ---------------------------------------------------------------------------
// Content builders
// ---------------------------------------------------------------------------

fn render_lyrics(lyrics: &LyricsLine, transpose_offset: i8, page: &mut PageBuilder) {
    if !lyrics.has_chords() {
        page.text(&lyrics.text(), Font::Helvetica, LYRICS_SIZE);
        page.newline(LYRICS_SIZE + LINE_GAP);
        return;
    }

    // Chord row
    let mut x = MARGIN_LEFT;
    let start_y = page.y;
    for seg in &lyrics.segments {
        if let Some(chord) = &seg.chord {
            let display_name = if transpose_offset != 0 {
                transpose_chord(chord, transpose_offset).name
            } else {
                chord.name.clone()
            };
            page.text_at(&display_name, Font::HelveticaBold, CHORD_SIZE, x, start_y);
        }
        let text_w = seg.text.chars().count() as f32 * LYRICS_SIZE * CHAR_WIDTH;
        let chord_w = seg.chord.as_ref().map_or(0.0, |c| {
            let display_len = if transpose_offset != 0 {
                transpose_chord(c, transpose_offset).name.chars().count()
            } else {
                c.name.chars().count()
            };
            display_len as f32 * CHORD_SIZE * CHAR_WIDTH + 2.0
        });
        x += text_w.max(chord_w);
    }

    // Lyrics row
    page.y -= CHORD_SIZE + 2.0;
    page.text(&lyrics.text(), Font::Helvetica, LYRICS_SIZE);
    page.newline(LYRICS_SIZE + LINE_GAP);
}

/// Render the section label for a start-of-section directive.
fn render_section_label(directive: &chordpro_core::ast::Directive, page: &mut PageBuilder) {
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
        page.text(&text, Font::HelveticaBoldOblique, SECTION_SIZE);
        page.newline(SECTION_SIZE + LINE_GAP);
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

fn render_directive(directive: &chordpro_core::ast::Directive, page: &mut PageBuilder) {
    render_section_label(directive, page);
}

/// Render a `{chorus}` recall directive in the PDF.
///
/// Emits a "Chorus" label (with optional custom label) followed by the content
/// of the most recently defined chorus section.
fn render_chorus_recall(
    value: &Option<String>,
    chorus_body: &[Line],
    transpose_offset: i8,
    page: &mut PageBuilder,
) {
    let text = match value {
        Some(v) if !v.is_empty() => format!("Chorus: {v}"),
        _ => "Chorus".to_string(),
    };
    page.text(&text, Font::HelveticaBoldOblique, SECTION_SIZE);
    page.newline(SECTION_SIZE + LINE_GAP);

    // Replay the stored chorus body lines.
    for line in chorus_body {
        match line {
            Line::Lyrics(lyrics) => render_lyrics(lyrics, transpose_offset, page),
            Line::Comment(style, text) => render_comment(*style, text, page),
            Line::Empty => page.newline(LINE_GAP * 2.0),
            Line::Directive(d) if !d.kind.is_metadata() => render_directive(d, page),
            _ => {}
        }
    }
}

fn render_comment(_style: CommentStyle, text: &str, page: &mut PageBuilder) {
    let font = Font::HelveticaOblique;
    page.text(text, font, COMMENT_SIZE);
    page.newline(COMMENT_SIZE + LINE_GAP);
}

// ---------------------------------------------------------------------------
// PDF text operation accumulator
// ---------------------------------------------------------------------------

/// Tracks the current Y position and accumulates PDF content-stream operations.
struct PageBuilder {
    y: f32,
    ops: Vec<String>,
}

impl PageBuilder {
    fn new() -> Self {
        Self {
            y: PAGE_H - MARGIN_TOP,
            ops: Vec::new(),
        }
    }

    /// Emit text at the current left margin and Y position.
    fn text(&mut self, text: &str, font: Font, size: f32) {
        self.text_at(text, font, size, MARGIN_LEFT, self.y);
    }

    /// Emit text at an explicit (x, y) position.
    fn text_at(&mut self, text: &str, font: Font, size: f32, x: f32, y: f32) {
        self.ops.push("BT".to_string());
        self.ops
            .push(format!("{} {} Tf", font.pdf_name(), fmt_f32(size)));
        self.ops.push(format!("{} {} Td", fmt_f32(x), fmt_f32(y)));
        self.ops.push(format!("({}) Tj", pdf_escape(text)));
        self.ops.push("ET".to_string());
    }

    /// Move the Y cursor down.
    fn newline(&mut self, amount: f32) {
        self.y -= amount;
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
// PDF document assembly
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

/// Build a complete PDF file from content-stream operations.
fn build_pdf(ops: &[String]) -> Vec<u8> {
    let mut offsets: Vec<usize> = Vec::new();
    let mut pdf = Vec::<u8>::new();

    // Header
    let header = b"%PDF-1.4\n";
    pdf.extend_from_slice(header);

    // Object 1: Catalog
    offsets.push(pdf.len());
    let obj1 = "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    pdf.extend_from_slice(obj1.as_bytes());

    // Object 2: Pages
    offsets.push(pdf.len());
    let font_refs: String = FONTS
        .iter()
        .enumerate()
        .map(|(i, _)| format!("{} {} 0 R", FONTS[i].pdf_name(), i + 4))
        .collect::<Vec<_>>()
        .join(" ");
    let obj2 = format!(
        "2 0 obj\n<< /Type /Pages /MediaBox [0 0 {PAGE_W} {PAGE_H}] /Resources << /Font << {font_refs} >> /ProcSet [/PDF /Text] >> /Kids [3 0 R] /Count 1 >>\nendobj\n"
    );
    pdf.extend_from_slice(obj2.as_bytes());

    // Object 3: Page
    offsets.push(pdf.len());
    let content_obj_num = 4 + FONTS.len();
    let obj3 = format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /Contents {content_obj_num} 0 R >>\nendobj\n"
    );
    pdf.extend_from_slice(obj3.as_bytes());

    // Objects 4..7: Font dictionaries
    for font in &FONTS {
        offsets.push(pdf.len());
        let obj = format!(
            "{} 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /{} /Encoding /WinAnsiEncoding >>\nendobj\n",
            offsets.len(),
            font.base_name()
        );
        pdf.extend_from_slice(obj.as_bytes());
    }

    // Content stream
    let content = ops.join("\n");
    offsets.push(pdf.len());
    let stream_obj = format!(
        "{content_obj_num} 0 obj\n<< /Length {} >>\nstream\n{content}\nendstream\nendobj\n",
        content.len() + 1 // +1 for trailing newline in stream
    );
    pdf.extend_from_slice(stream_obj.as_bytes());

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
