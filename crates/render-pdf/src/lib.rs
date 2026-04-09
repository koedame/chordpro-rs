//! PDF renderer for ChordPro documents.
//!
//! Converts a parsed ChordPro AST into a PDF document using built-in PDF
//! Type1 fonts (Helvetica family). Uses `flate2` for PNG image decompression.
//!
//! Supports multi-page output: content automatically flows to new pages when
//! the current page overflows, and `{new_page}` / `{new_physical_page}`
//! directives trigger explicit page breaks.

use chordsketch_core::ast::{CommentStyle, DirectiveKind, ImageAttributes, Line, LyricsLine, Song};
use chordsketch_core::config::Config;
use chordsketch_core::inline_markup::TextSpan;
use chordsketch_core::render_result::RenderResult;
use chordsketch_core::transpose::transpose_chord;

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::io::{Read as IoRead, Write as IoWrite};

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
/// Per-character width as fraction of font size for Helvetica.
///
/// Uses standard Helvetica AFM glyph widths (divided by 1000) for ASCII
/// printable characters. Non-ASCII and control characters fall back to
/// the average width of 0.52.
#[must_use]
fn char_width(c: char) -> f32 {
    // Helvetica AFM widths for ASCII 32–126, divided by 1000.
    #[rustfmt::skip]
    const WIDTHS: [f32; 95] = [
        0.278, // space
        0.278, // !
        0.355, // "
        0.556, // #
        0.556, // $
        0.889, // %
        0.667, // &
        0.222, // '
        0.333, // (
        0.333, // )
        0.389, // *
        0.584, // +
        0.278, // ,
        0.333, // -
        0.278, // .
        0.278, // /
        0.556, // 0
        0.556, // 1
        0.556, // 2
        0.556, // 3
        0.556, // 4
        0.556, // 5
        0.556, // 6
        0.556, // 7
        0.556, // 8
        0.556, // 9
        0.278, // :
        0.278, // ;
        0.584, // <
        0.584, // =
        0.584, // >
        0.556, // ?
        1.015, // @
        0.667, // A
        0.667, // B
        0.722, // C
        0.722, // D
        0.667, // E
        0.611, // F
        0.778, // G
        0.722, // H
        0.278, // I
        0.500, // J
        0.667, // K
        0.556, // L
        0.833, // M
        0.722, // N
        0.778, // O
        0.667, // P
        0.778, // Q
        0.722, // R
        0.667, // S
        0.611, // T
        0.722, // U
        0.667, // V
        0.944, // W
        0.667, // X
        0.667, // Y
        0.611, // Z
        0.278, // [
        0.278, // backslash
        0.278, // ]
        0.469, // ^
        0.556, // _
        0.333, // `
        0.556, // a
        0.556, // b
        0.500, // c
        0.556, // d
        0.556, // e
        0.278, // f
        0.556, // g
        0.556, // h
        0.222, // i
        0.222, // j
        0.500, // k
        0.222, // l
        0.833, // m
        0.556, // n
        0.556, // o
        0.556, // p
        0.556, // q
        0.333, // r
        0.500, // s
        0.278, // t
        0.556, // u
        0.500, // v
        0.722, // w
        0.500, // x
        0.500, // y
        0.500, // z
        0.334, // {
        0.260, // |
        0.334, // }
        0.584, // ~
    ];
    let code = c as u32;
    if (32..=126).contains(&code) {
        WIDTHS[(code - 32) as usize]
    } else {
        0.52 // fallback for non-ASCII
    }
}

/// Compute text width in points for a string at the given font size.
#[must_use]
fn text_width(s: &str, font_size: f32) -> f32 {
    s.chars().map(|c| char_width(c) * font_size).sum()
}

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
/// Maximum image file size in bytes (50 MB).
const MAX_IMAGE_FILE_SIZE: u64 = 50 * 1024 * 1024;
/// Maximum native image dimension in pixels.  JPEG headers can report up to
/// 65535 pixels; without explicit width/height/scale this maps 1:1 to PDF
/// points, producing a ~23-metre image.  Clamping to 10 000 keeps the default
/// within a few A0-sized pages while still being generous for real photographs.
const MAX_IMAGE_PIXELS: u32 = 10_000;
/// Maximum number of images that can be embedded in a single PDF document.
/// Prevents memory exhaustion from documents with many large image directives.
const MAX_IMAGES: usize = 1_000;
/// Maximum number of chorus recall directives allowed per song.
/// Prevents output amplification from malicious inputs with many `{chorus}` lines.
const MAX_CHORUS_RECALLS: usize = 1000;

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
    render_song_with_transpose(song, 0, &Config::defaults())
}

/// Render a [`Song`] AST to PDF bytes with an additional CLI transposition offset.
///
/// The `cli_transpose` parameter is added to any in-file `{transpose}` directive
/// values, allowing the CLI `--transpose` flag to combine with in-file directives.
///
/// Warnings are printed to stderr via `eprintln!`. Use
/// [`render_song_with_warnings`] to capture them programmatically.
#[must_use]
pub fn render_song_with_transpose(song: &Song, cli_transpose: i8, config: &Config) -> Vec<u8> {
    let result = render_song_with_warnings(song, cli_transpose, config);
    for w in &result.warnings {
        eprintln!("warning: {w}");
    }
    result.output
}

/// Render a [`Song`] AST to PDF bytes, returning warnings programmatically.
///
/// This is the structured variant of [`render_song_with_transpose`]. Instead
/// of printing warnings to stderr, they are collected into
/// [`RenderResult::warnings`].
pub fn render_song_with_warnings(
    song: &Song,
    cli_transpose: i8,
    config: &Config,
) -> RenderResult<Vec<u8>> {
    let mut warnings = Vec::new();
    // Apply song-level config overrides before creating the document.
    let song_overrides = song.config_overrides();
    let song_config;
    let effective_config = if song_overrides.is_empty() {
        config
    } else {
        song_config = config
            .clone()
            .with_song_overrides(&song_overrides, &mut warnings);
        &song_config
    };
    let mut doc = PdfDocument::from_config_with_warnings(effective_config, &mut warnings);
    render_song_into_doc(
        song,
        cli_transpose,
        effective_config,
        &mut doc,
        &mut warnings,
    );
    RenderResult::with_warnings(doc.build_pdf(), warnings)
}

/// Render multiple [`Song`]s into a single multi-page PDF document.
///
/// Each song starts on a new page.
#[must_use]
pub fn render_songs(songs: &[Song]) -> Vec<u8> {
    render_songs_with_transpose(songs, 0, &Config::defaults())
}

/// Render multiple [`Song`]s into a single PDF with transposition.
///
/// Each song starts on a new page (except the first). When there are two or
/// more songs, a Table of Contents page is prepended with song titles and
/// page numbers.
///
/// Warnings are printed to stderr via `eprintln!`. Use
/// [`render_songs_with_warnings`] to capture them programmatically.
#[must_use]
pub fn render_songs_with_transpose(songs: &[Song], cli_transpose: i8, config: &Config) -> Vec<u8> {
    let result = render_songs_with_warnings(songs, cli_transpose, config);
    for w in &result.warnings {
        eprintln!("warning: {w}");
    }
    result.output
}

/// Render multiple [`Song`]s into a single PDF, returning warnings programmatically.
///
/// This is the structured variant of [`render_songs_with_transpose`]. Instead
/// of printing warnings to stderr, they are collected into
/// [`RenderResult::warnings`].
pub fn render_songs_with_warnings(
    songs: &[Song],
    cli_transpose: i8,
    config: &Config,
) -> RenderResult<Vec<u8>> {
    let mut warnings = Vec::new();

    if songs.len() <= 1 {
        return songs
            .first()
            .map(|s| render_song_with_warnings(s, cli_transpose, config))
            .unwrap_or_else(|| RenderResult::with_warnings(Vec::new(), warnings));
    }

    // Phase 1: render all songs and record which page each starts on.
    let mut body_doc = PdfDocument::from_config_with_warnings(config, &mut warnings);
    let mut toc_entries: Vec<(String, usize)> = Vec::new(); // (title, page_index)

    for (i, song) in songs.iter().enumerate() {
        if i > 0 {
            body_doc.new_page();
        }
        // Apply per-song config overrides (e.g. pdf.margins.*), always
        // resetting to the base-config margins first so that song N's
        // overrides do not bleed into song N+1.
        let song_overrides = song.config_overrides();
        let song_config;
        let effective_config = if song_overrides.is_empty() {
            config
        } else {
            song_config = config
                .clone()
                .with_song_overrides(&song_overrides, &mut warnings);
            &song_config
        };
        body_doc.reset_margins_from_config(effective_config, &mut warnings);
        let start_page = body_doc.page_count();
        let title = song
            .metadata
            .title
            .as_deref()
            .unwrap_or("Untitled")
            .to_string();
        toc_entries.push((title, start_page));
        render_song_into_doc(
            song,
            cli_transpose,
            effective_config,
            &mut body_doc,
            &mut warnings,
        );
    }

    // Phase 2: generate ToC pages.
    let mut toc_doc = PdfDocument::from_config_with_warnings(config, &mut warnings);
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
    let mut toc_doc = PdfDocument::from_config_with_warnings(config, &mut warnings);
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
        let num_width = text_width(&num_str, TOC_ENTRY_SIZE);
        let right_x = PAGE_W - toc_doc.margin_right - num_width;
        toc_doc.text_at(&num_str, Font::Helvetica, TOC_ENTRY_SIZE, right_x, y);

        toc_doc.newline(TOC_ENTRY_SIZE + LINE_GAP);
    }

    // Phase 4: combine ToC pages + body pages.
    let mut combined = toc_doc;
    for page_ops in body_doc.take_pages() {
        combined.push_page(page_ops);
    }

    RenderResult::with_warnings(combined.build_pdf(), warnings)
}

/// Render a single song's content into an existing [`PdfDocument`].
///
/// This is the shared implementation used by both [`render_song_with_transpose`]
/// and [`render_songs_with_transpose`]. It does not call `build_pdf`; the caller
/// is responsible for finalising the document.
fn render_song_into_doc(
    song: &Song,
    cli_transpose: i8,
    config: &Config,
    doc: &mut PdfDocument,
    warnings: &mut Vec<String>,
) {
    // Extract song-level transpose delta from {+config.settings.transpose}.
    // The base config transpose is already folded into cli_transpose by the caller.
    let song_overrides = song.config_overrides();
    let song_transpose_delta = Config::song_transpose_delta(&song_overrides);
    let (combined_transpose, _) =
        chordsketch_core::transpose::combine_transpose(cli_transpose, song_transpose_delta);
    let mut transpose_offset: i8 = combined_transpose;
    let mut fmt_state = PdfFormattingState::default();

    // Read configurable frets_shown for chord diagrams.
    let diagram_frets = config
        .get_path("diagrams.frets")
        .as_f64()
        .map_or(chordsketch_core::chord_diagram::DEFAULT_FRETS_SHOWN, |n| {
            (n as usize).max(1)
        });

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

    // Controls whether chord diagrams are rendered. Set by {diagrams: off/on}.
    let mut show_diagrams = true;

    // Instrument for the auto-inject diagram block at end of song.
    let default_instrument = config
        .get_path("diagrams.instrument")
        .as_str()
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "guitar".to_string());
    let mut auto_diagrams_instrument: Option<String> = None;

    // Stores the AST lines of the most recently defined chorus body for replay.
    let mut chorus_body: Vec<Line> = Vec::new();
    // Temporary buffer for collecting chorus content while inside a chorus section.
    let mut chorus_buf: Option<Vec<Line>> = None;
    let mut saved_fmt_state: Option<PdfFormattingState> = None;
    let mut chorus_recall_count: usize = 0;

    for line in &song.lines {
        match line {
            Line::Lyrics(lyrics) => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                render_lyrics(lyrics, transpose_offset, &fmt_state, doc);
            }
            Line::Directive(d) if !d.kind.is_metadata() => {
                if d.kind == DirectiveKind::Diagrams {
                    let val = d.value.as_deref().unwrap_or("on");
                    if val.eq_ignore_ascii_case("off") {
                        show_diagrams = false;
                        auto_diagrams_instrument = None;
                    } else {
                        show_diagrams = true;
                        let instr = match val.to_ascii_lowercase().as_str() {
                            "ukulele" | "uke" => "ukulele",
                            "guitar" => "guitar",
                            _ => &default_instrument,
                        };
                        auto_diagrams_instrument = Some(instr.to_string());
                    }
                    continue;
                }
                if d.kind == DirectiveKind::NoDiagrams {
                    show_diagrams = false;
                    auto_diagrams_instrument = None;
                    continue;
                }
                if d.kind == DirectiveKind::Transpose {
                    let file_offset: i8 =
                        d.value.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0);
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
                if d.kind.is_font_size_color() {
                    fmt_state.apply(&d.kind, &d.value);
                    continue;
                }
                match &d.kind {
                    DirectiveKind::StartOfChorus => {
                        render_section_label(d, doc);
                        chorus_buf = Some(Vec::new());
                        saved_fmt_state = Some(fmt_state.clone());
                    }
                    DirectiveKind::EndOfChorus => {
                        if let Some(buf) = chorus_buf.take() {
                            chorus_body = buf;
                        }
                        if let Some(saved) = saved_fmt_state.take() {
                            fmt_state = saved;
                        }
                    }
                    DirectiveKind::Chorus => {
                        if chorus_recall_count < MAX_CHORUS_RECALLS {
                            render_chorus_recall(
                                &d.value,
                                &chorus_body,
                                transpose_offset,
                                &fmt_state,
                                show_diagrams,
                                diagram_frets,
                                doc,
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
                    // All page control directives ({new_page}, {new_physical_page},
                    // {column_break}, {columns}) are intentionally excluded from the
                    // chorus buffer. These affect global page/column layout, and
                    // replaying them during {chorus} recall would produce unexpected
                    // layout changes (e.g., duplicate page breaks, column resets).
                    DirectiveKind::NewPage => {
                        doc.new_page();
                    }
                    DirectiveKind::NewPhysicalPage => {
                        doc.new_page();
                        // In duplex printing, recto pages are odd-numbered
                        // (1, 3, 5, …).  If the new page is even (verso),
                        // insert a blank page so the next content starts on
                        // a recto page.
                        if doc.page_count() % 2 == 0 {
                            doc.new_page();
                        }
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
                        render_directive(d, show_diagrams, diagram_frets, doc);
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

    // Auto-inject diagram block when {diagrams} was seen.
    if let Some(ref instrument) = auto_diagrams_instrument {
        let defines = song.fretted_defines();
        let diagrams: Vec<_> = song
            .used_chord_names()
            .into_iter()
            .filter_map(|name| {
                chordsketch_core::lookup_diagram(&name, &defines, instrument, diagram_frets)
            })
            .collect();
        for diagram in &diagrams {
            render_chord_diagram_pdf(diagram, doc);
        }
    }
}

/// Parse and render a ChordPro source string to PDF bytes.
#[must_use = "parse errors should be handled"]
pub fn try_render(input: &str) -> Result<Vec<u8>, chordsketch_core::ParseError> {
    let song = chordsketch_core::parse(input)?;
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
        // Compute the display name once per segment, reusing it for both
        // rendering and width measurement to avoid duplicate transposition.
        let chord_display: Option<String> = seg.chord.as_ref().map(|c| {
            if transpose_offset != 0 {
                transpose_chord(c, transpose_offset)
                    .display_name()
                    .to_string()
            } else {
                c.display_name().to_string()
            }
        });
        if let Some(ref name) = chord_display {
            doc.text_at(name, Font::HelveticaBold, chord_size, x, start_y);
        }
        let text_w = text_width(&seg.text, lyrics_size);
        let chord_w = chord_display
            .as_ref()
            .map_or(0.0, |name| text_width(name, chord_size) + 2.0);
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
    let clip = doc.num_columns > 1;
    let col_right = if clip {
        doc.margin_left() + doc.column_width()
    } else {
        0.0
    };
    let mut x = doc.margin_left();
    let y = doc.y();
    if clip {
        let clip_w = (col_right - x).max(0.0);
        let ops = doc.current_page_mut();
        ops.push("q".to_string());
        ops.push(format!(
            "{} {} {} {} re W n",
            fmt_f32(x),
            fmt_f32(0.0),
            fmt_f32(clip_w),
            fmt_f32(PAGE_H)
        ));
    }
    for seg in &lyrics.segments {
        if seg.has_markup() {
            x = render_span_list(&seg.spans, doc, x, y, font_size, false, false);
        } else {
            doc.text_at_raw(&seg.text, Font::Helvetica, font_size, x, y);
            x += text_width(&seg.text, font_size);
        }
    }
    if clip {
        let ops = doc.current_page_mut();
        ops.push("Q".to_string());
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
                doc.text_at_raw(text, font, font_size, x, y);
                x += text_width(text, font_size);
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
fn render_section_label(directive: &chordsketch_core::ast::Directive, doc: &mut PdfDocument) {
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
            Some(chordsketch_core::capitalize(section_name))
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

fn render_directive(
    directive: &chordsketch_core::ast::Directive,
    show_diagrams: bool,
    diagram_frets: usize,
    doc: &mut PdfDocument,
) {
    if directive.kind == DirectiveKind::Define && show_diagrams {
        if let Some(ref value) = directive.value {
            let def = chordsketch_core::ast::ChordDefinition::parse_value(value);
            if let Some(ref raw) = def.raw {
                if let Some(mut diagram) =
                    chordsketch_core::chord_diagram::DiagramData::from_raw_infer_frets(
                        &def.name,
                        raw,
                        diagram_frets,
                    )
                {
                    diagram.display_name = def.display.clone();
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

/// Check whether an image path is safe to open.
///
/// Rejects empty paths, paths containing null bytes, absolute paths, and
/// paths containing `..` components to prevent directory traversal attacks.
/// Only relative paths that stay within (or below) the current working
/// directory are accepted.
fn is_safe_image_path(path: &str) -> bool {
    // Reject empty paths and paths with null bytes (defense-in-depth).
    if path.is_empty() || path.contains('\0') {
        return false;
    }

    // Reject Windows-style absolute paths on all platforms.  On Unix,
    // `Path::is_absolute()` does not flag `C:\…` or `\\…` as absolute,
    // so we perform string-level checks for drive letters and UNC paths.
    if chordsketch_core::image_path::is_windows_absolute(path) {
        return false;
    }

    let p = std::path::Path::new(path);

    // Reject absolute paths (Unix `/…` and Windows `C:\…`).
    // Also explicitly check for leading `/` since `is_absolute()` on Windows
    // does not consider Unix-style root paths as absolute.
    if p.is_absolute() || path.starts_with('/') {
        return false;
    }

    // Reject directory traversal (`..` path components).
    // Uses the shared helper that splits on both `/` and `\`, so
    // backslash-separated traversal like `images\..\..\etc\passwd` is
    // also caught on Unix (where `Path::components()` treats `\` as a
    // literal character).
    if chordsketch_core::image_path::has_traversal(path) {
        return false;
    }

    true
}

/// Read an image file, rejecting symlinks and files exceeding
/// [`MAX_IMAGE_FILE_SIZE`].
///
/// On Unix the file is opened with `O_NOFOLLOW` so that opening a symlink
/// fails with `ELOOP`, and the size is checked via `fstat` on the open file
/// descriptor.  This eliminates the TOCTOU window that would exist if
/// `symlink_metadata` and `read` were separate operations on the path.
///
/// On non-Unix platforms, the function falls back to `symlink_metadata`
/// followed by `std::fs::read`, which has a theoretical race window but is
/// the best available option.
fn read_image_file(path: &str) -> Option<Vec<u8>> {
    #[cfg(unix)]
    {
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;

        // O_NOFOLLOW: the open call itself fails if path is a symlink.
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .ok()?;

        // fstat on the fd — no TOCTOU with the open above.
        let meta = file.metadata().ok()?;
        if meta.len() > MAX_IMAGE_FILE_SIZE {
            return None;
        }

        let mut buf = Vec::with_capacity(meta.len() as usize);
        file.read_to_end(&mut buf).ok()?;

        // Belt-and-suspenders: verify actual bytes read.
        if buf.len() as u64 > MAX_IMAGE_FILE_SIZE {
            return None;
        }
        Some(buf)
    }

    #[cfg(not(unix))]
    {
        // Fallback: symlink_metadata + read (has a theoretical TOCTOU gap).
        let meta = std::fs::symlink_metadata(path).ok()?;
        if meta.file_type().is_symlink() {
            return None;
        }
        if meta.len() > MAX_IMAGE_FILE_SIZE {
            return None;
        }
        let data = std::fs::read(path).ok()?;
        if data.len() as u64 > MAX_IMAGE_FILE_SIZE {
            return None;
        }
        Some(data)
    }
}

/// Render an `{image}` directive by embedding an image file into the PDF.
///
/// Supported formats: JPEG (`.jpg`/`.jpeg`) and PNG (`.png`).
/// The image is read from disk, validated, and embedded as an XObject.
/// If the file cannot be read, has no recognisable header, has an
/// unsupported extension, is a symlink, or exceeds [`MAX_IMAGE_FILE_SIZE`],
/// the directive is silently skipped.
///
/// The `anchor` attribute controls horizontal alignment: `"line"` (default)
/// places the image at the column margin, `"column"` centres it within the
/// column, and `"paper"` centres it on the full page.
fn render_image(attrs: &ImageAttributes, doc: &mut PdfDocument) {
    if !attrs.has_src() {
        return;
    }

    // Limit the number of embedded images to prevent memory exhaustion.
    if doc.images.len() >= MAX_IMAGES {
        return;
    }

    // Reject paths that could escape the working directory.
    if !is_safe_image_path(&attrs.src) {
        return;
    }

    let src_lower = attrs.src.to_ascii_lowercase();
    let is_jpeg = src_lower.ends_with(".jpg") || src_lower.ends_with(".jpeg");
    let is_png = src_lower.ends_with(".png");
    if !is_jpeg && !is_png {
        return;
    }

    // Read the image file, rejecting symlinks and oversized files.
    let data = match read_image_file(&attrs.src) {
        Some(d) => d,
        None => return,
    };

    // Parse image dimensions and embed based on format.
    let (pixel_w, pixel_h, img_idx) = if is_jpeg {
        let (w, h, components) = match parse_jpeg_dimensions(&data) {
            Some(dims) => dims,
            None => return,
        };
        if w == 0 || h == 0 {
            return;
        }
        let idx = doc.embed_jpeg(data, w, h, components);
        (w, h, idx)
    } else {
        let info = match parse_png(&data) {
            Some(info) => info,
            None => return,
        };
        if info.width == 0 || info.height == 0 {
            return;
        }
        let w = info.width;
        let h = info.height;
        let idx = doc.embed_png(info);
        (w, h, idx)
    };

    // Clamp native pixel dimensions for rendering, but preserve originals
    // for the PDF XObject metadata (which must match the actual stream).
    let clamped_w = pixel_w.min(MAX_IMAGE_PIXELS);
    let clamped_h = pixel_h.min(MAX_IMAGE_PIXELS);

    // Compute rendered dimensions in PDF points (1 pt = 1/72 inch).
    let native_w = clamped_w as f32;
    let native_h = clamped_h as f32;
    let aspect = native_w / native_h;

    let (render_w, render_h) = compute_image_dimensions(attrs, native_w, native_h, aspect);

    // Clamp to printable area (per-column width in multi-column layouts).
    let max_w = doc.column_width();
    let max_h = PAGE_H - doc.margin_top - doc.margin_bottom;
    let (render_w, render_h) = clamp_to_printable_area(render_w, render_h, max_w, max_h, aspect);

    doc.ensure_space(render_h + LINE_GAP);

    // Compute horizontal position based on the anchor attribute.
    let x = match attrs.anchor.as_deref() {
        Some("column") => {
            let col_left = doc.margin_left();
            let col_w = doc.column_width();
            col_left + (col_w - render_w) / 2.0
        }
        Some("paper") => (PAGE_W - render_w) / 2.0,
        _ => doc.margin_left(),
    };

    // PDF images are placed with the origin at the bottom-left corner.
    let y = doc.y() - render_h;
    doc.draw_image(img_idx, x, y, render_w, render_h);
    doc.advance_y(render_h);
    doc.newline(LINE_GAP);
}

/// Clamp rendered image dimensions to fit within the printable area while
/// preserving the aspect ratio.
///
/// If width exceeds `max_w`, it is clamped and height is scaled down
/// proportionally (then clamped to `max_h` if still too tall).
/// If height exceeds `max_h`, it is clamped and width is scaled down
/// proportionally (then clamped to `max_w` if still too wide).
fn clamp_to_printable_area(w: f32, h: f32, max_w: f32, max_h: f32, aspect: f32) -> (f32, f32) {
    if w > max_w {
        let clamped_h = max_w / aspect;
        if clamped_h > max_h {
            let clamped_w = (max_h * aspect).min(max_w);
            (clamped_w, max_h)
        } else {
            (max_w, clamped_h)
        }
    } else if h > max_h {
        let clamped_w = (max_h * aspect).min(max_w);
        (clamped_w, clamped_w / aspect)
    } else {
        (w, h)
    }
}

/// Parse a dimension value that may be an absolute number or a percentage.
///
/// Returns `None` if the value is not a valid positive number (or percentage).
/// Percentage values (e.g. `"50%"`) are resolved against `reference`.
fn parse_dimension(value: &str, reference: f32) -> Option<f32> {
    let trimmed = value.trim();
    if let Some(pct_str) = trimmed.strip_suffix('%') {
        let pct: f32 = pct_str.trim().parse().ok()?;
        let result = reference * pct / 100.0;
        if result > 0.0 && result.is_finite() {
            Some(result)
        } else {
            None
        }
    } else {
        let v: f32 = trimmed.parse().ok()?;
        if v > 0.0 && v.is_finite() {
            Some(v)
        } else {
            None
        }
    }
}

/// Compute the rendered width and height of an image based on the directive
/// attributes (`width`, `height`, `scale`).
///
/// Priority: explicit width/height > scale > native dimensions.
/// Width and height values may be absolute points or percentages of the
/// native image dimensions (e.g. `"50%"`).
fn compute_image_dimensions(
    attrs: &ImageAttributes,
    native_w: f32,
    native_h: f32,
    aspect: f32,
) -> (f32, f32) {
    let parsed_w = attrs
        .width
        .as_deref()
        .and_then(|v| parse_dimension(v, native_w));
    let parsed_h = attrs
        .height
        .as_deref()
        .and_then(|v| parse_dimension(v, native_h));
    let parsed_scale = attrs
        .scale
        .as_deref()
        .and_then(|v| v.trim().parse::<f32>().ok())
        .filter(|&v| v > 0.0 && v.is_finite());

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
    data: &chordsketch_core::chord_diagram::DiagramData,
    doc: &mut PdfDocument,
) {
    // Guard: mirror render_svg bounds checks for strings and frets_shown.
    if data.strings < chordsketch_core::chord_diagram::MIN_STRINGS
        || data.strings > chordsketch_core::chord_diagram::MAX_STRINGS
        || data.frets_shown < chordsketch_core::chord_diagram::MIN_FRETS_SHOWN
        || data.frets_shown > chordsketch_core::chord_diagram::MAX_FRETS_SHOWN
    {
        return;
    }

    // PDF cell dimensions in points (1 pt = 1/72 inch). Intentionally
    // smaller than the SVG renderer's 16x20 px because PDF targets printed
    // pages where diagrams sit alongside text and must be compact.
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

    // Chord name (uses display override if present)
    doc.text_at(data.title(), Font::HelveticaBold, 9.0, base_x, top_y);

    let grid_top = top_y - 15.0; // below the name

    // Nut line (thick for open position)
    if data.base_fret == 1 {
        doc.line_at(base_x, grid_top, base_x + grid_w, grid_top, 2.0);
    } else {
        // Show fret number, clamping x to >= 0 to prevent off-page rendering.
        let fret_label = format!("{}fr", data.base_fret);
        let fret_label_x = (base_x - 16.0).max(0.0);
        doc.text_at(
            &fret_label,
            Font::Helvetica,
            6.0,
            fret_label_x,
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
            // Finger number inside the dot (if available and non-zero)
            if let Some(&finger) = data.fingers.get(i) {
                if finger > 0 {
                    let label = finger.to_string();
                    doc.white_text_at(&label, Font::Helvetica, 5.0, x - 1.5, y - 1.5);
                }
            }
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
    show_diagrams: bool,
    diagram_frets: usize,
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
            Line::Directive(d) if !d.kind.is_metadata() => {
                render_directive(d, show_diagrams, diagram_frets, doc);
            }
            _ => {}
        }
    }
}

fn render_comment(style: CommentStyle, text: &str, doc: &mut PdfDocument) {
    let font = match style {
        CommentStyle::Normal => Font::Helvetica,
        CommentStyle::Italic | CommentStyle::Boxed => Font::HelveticaOblique,
    };
    if style == CommentStyle::Boxed {
        let padding = 3.0_f32;
        let box_h = COMMENT_SIZE + padding * 2.0;
        doc.ensure_space(box_h + LINE_GAP);
        let x = doc.margin_left();
        let text_y = doc.y();
        // PDF rect y is bottom-left; text_y is the baseline top.
        let rect_y = text_y - COMMENT_SIZE - padding;
        let text_w = text_width(text, COMMENT_SIZE);
        let box_w = text_w + padding * 2.0;
        doc.rect_stroke(x, rect_y, box_w, box_h, 0.5);
        doc.text_at(text, font, COMMENT_SIZE, x + padding, text_y);
        doc.newline(box_h + LINE_GAP);
    } else {
        doc.ensure_space(COMMENT_SIZE + LINE_GAP);
        doc.text(text, font, COMMENT_SIZE);
        doc.newline(COMMENT_SIZE + LINE_GAP);
    }
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

/// Parse JPEG dimensions and component count from raw file data by locating a
/// Start of Frame marker.
///
/// JPEG files consist of a sequence of markers. This function scans for any
/// valid SOF marker (SOF0–SOF3, SOF5–SOF7, SOF9–SOF11, SOF13–SOF15) and reads
/// the image height (2 bytes at marker+3), width (2 bytes at marker+5), and
/// number of color components (1 byte at marker+7).
///
/// Returns `(width, height, components)` or `None` if the data is too short
/// or no SOF marker is found.
fn parse_jpeg_dimensions(data: &[u8]) -> Option<(u32, u32, u8)> {
    // Maximum number of bytes to scan for the SOF marker.  Real JPEG files
    // contain the SOF within the first few KB.  This limit prevents a crafted
    // file from forcing a byte-by-byte scan through megabytes of data.
    const MAX_SCAN_BYTES: usize = 64 * 1024;

    // Minimum valid JPEG: SOI (2 bytes) + at least one marker segment
    if data.len() < 4 {
        return None;
    }
    // Verify JPEG SOI marker
    if data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }

    let scan_limit = data.len().min(MAX_SCAN_BYTES);
    let mut i = 2;
    while i + 1 < scan_limit {
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

        // Any SOF marker (SOF0–SOF3, SOF5–SOF7, SOF9–SOF11, SOF13–SOF15).
        // Excludes 0xC4 (DHT), 0xC8 (JPG reserved), and 0xCC (DAC).
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            // Need at least 8 more bytes after the marker:
            // length(2) + precision(1) + height(2) + width(2) + components(1)
            if i + 10 > data.len() {
                return None;
            }
            let height = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
            let width = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
            let components = data[i + 9];
            return Some((width, height, components));
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

// ---------------------------------------------------------------------------
// PNG parsing
// ---------------------------------------------------------------------------

/// PNG file signature (8 bytes).
const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

/// Parsed PNG image data ready for PDF embedding.
struct PngInfo {
    /// Image width in pixels.
    width: u32,
    /// Image height in pixels.
    height: u32,
    /// Bit depth per channel.
    bit_depth: u8,
    /// Number of color channels in the output stream (after alpha removal).
    /// 1 = grayscale, 3 = RGB.  Indexed images are expanded to RGB.
    colors: u8,
    /// Zlib-compressed pixel data for the main image (alpha stripped if present).
    idat_data: Vec<u8>,
    /// PLTE chunk data for indexed color images (color type 3).
    palette: Option<Vec<u8>>,
    /// Zlib-compressed alpha channel data (for color types 4 and 6).
    smask: Option<Vec<u8>>,
}

/// Parse a PNG file and prepare its data for PDF embedding.
///
/// Extracts dimensions from the IHDR chunk, concatenates IDAT chunks, and
/// handles alpha separation for color types 4 (gray+alpha) and 6 (RGBA).
///
/// Returns `None` if the data is not a valid PNG or cannot be processed.
fn parse_png(data: &[u8]) -> Option<PngInfo> {
    // Verify PNG signature.
    if data.len() < 8 || data[..8] != PNG_SIGNATURE {
        return None;
    }

    // Parse IHDR (must be the first chunk after the signature).
    if data.len() < 8 + 4 + 4 + 13 {
        return None;
    }
    let ihdr_len = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    if ihdr_len < 13 {
        return None;
    }
    let chunk_type = &data[12..16];
    if chunk_type != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    let bit_depth = data[24];
    let color_type = data[25];

    // Collect IDAT chunks and optionally PLTE.
    let mut idat_chunks: Vec<u8> = Vec::new();
    let mut palette: Option<Vec<u8>> = None;
    let mut pos = 8; // skip signature

    while pos + 12 <= data.len() {
        let chunk_len =
            u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        let ctype = &data[pos + 4..pos + 8];

        // Guard against malformed chunk lengths.
        if pos + 12 + chunk_len > data.len() + 4 {
            break;
        }
        let chunk_data_start = pos + 8;
        let chunk_data_end = chunk_data_start + chunk_len;
        if chunk_data_end > data.len() {
            break;
        }

        if ctype == b"IDAT" {
            idat_chunks.extend_from_slice(&data[chunk_data_start..chunk_data_end]);
        } else if ctype == b"PLTE" {
            palette = Some(data[chunk_data_start..chunk_data_end].to_vec());
        } else if ctype == b"IEND" {
            break;
        }

        // 4 (length) + 4 (type) + chunk_len + 4 (CRC)
        pos += 12 + chunk_len;
    }

    if idat_chunks.is_empty() {
        return None;
    }

    let has_alpha = color_type == 4 || color_type == 6;

    if has_alpha {
        // Decompress IDAT, separate color and alpha, recompress both.
        separate_alpha(&idat_chunks, width, height, bit_depth, color_type)
    } else {
        // Color types 0 (gray), 2 (RGB), 3 (indexed): IDAT passthrough.
        let colors = match color_type {
            0 => 1, // grayscale
            3 => 3, // indexed → presented as RGB via palette lookup in PDF
            _ => 3, // RGB
        };
        Some(PngInfo {
            width,
            height,
            bit_depth,
            colors,
            idat_data: idat_chunks,
            palette,
            smask: None,
        })
    }
}

/// Decompress PNG IDAT data and separate color from alpha channels.
///
/// For color type 4 (gray+alpha), splits into 1-channel gray + 1-channel alpha.
/// For color type 6 (RGBA), splits into 3-channel RGB + 1-channel alpha.
///
/// Both output streams are zlib-compressed with PNG sub-filter byte prefixes
/// suitable for PDF `/FlateDecode` with `/Predictor 15`.
fn separate_alpha(
    idat_data: &[u8],
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
) -> Option<PngInfo> {
    let w = width as usize;
    let h = height as usize;
    let bytes_per_sample = if bit_depth == 16 { 2 } else { 1 };

    // Compute expected decompressed size from IHDR dimensions and apply a
    // safety cap to prevent memory exhaustion from high-compression-ratio PNGs.
    // Each row has a 1-byte filter prefix plus (width * channels * bytes_per_sample).
    let channels: usize = match color_type {
        4 => 2, // gray + alpha
        6 => 4, // RGBA
        _ => return None,
    };
    let expected_size = h.checked_mul(1 + w * channels * bytes_per_sample)?;
    // Hard cap at 256 MB regardless of declared dimensions.
    const MAX_DECOMPRESSED_SIZE: u64 = 256 * 1024 * 1024;
    let limit = (expected_size as u64).min(MAX_DECOMPRESSED_SIZE);

    // Decompress the IDAT zlib stream with size limit.
    let mut decoder = ZlibDecoder::new(idat_data).take(limit + 1);
    let mut raw = Vec::new();
    if decoder.read_to_end(&mut raw).is_err() || raw.len() as u64 > limit {
        return None;
    }

    // Channels in the raw decompressed data (including alpha).
    let (color_channels, alpha_channels) = match color_type {
        4 => (1, 1), // gray + alpha
        6 => (3, 1), // RGB + alpha
        _ => return None,
    };
    let total_channels = color_channels + alpha_channels;
    let src_stride = 1 + w * total_channels * bytes_per_sample; // +1 for filter byte

    if raw.len() < h * src_stride {
        return None;
    }

    // Un-filter all rows at once, then separate channels.
    let bpp = total_channels * bytes_per_sample;
    let row_bytes = w * total_channels * bytes_per_sample;
    let mut decoded = vec![0u8; h * row_bytes];

    for row in 0..h {
        let src_start = row * src_stride;
        let filter = raw[src_start];
        let src_row = &raw[src_start + 1..src_start + src_stride];
        let dst_start = row * row_bytes;

        decoded[dst_start..dst_start + row_bytes].copy_from_slice(src_row);

        match filter {
            0 => {} // None
            1 => {
                // Sub
                for i in bpp..row_bytes {
                    decoded[dst_start + i] =
                        decoded[dst_start + i].wrapping_add(decoded[dst_start + i - bpp]);
                }
            }
            2 => {
                // Up
                if row > 0 {
                    let prev_start = (row - 1) * row_bytes;
                    for i in 0..row_bytes {
                        decoded[dst_start + i] =
                            decoded[dst_start + i].wrapping_add(decoded[prev_start + i]);
                    }
                }
            }
            3 => {
                // Average
                let prev_start = if row > 0 { (row - 1) * row_bytes } else { 0 };
                for i in 0..row_bytes {
                    let left = if i >= bpp {
                        decoded[dst_start + i - bpp]
                    } else {
                        0
                    };
                    let up = if row > 0 { decoded[prev_start + i] } else { 0 };
                    decoded[dst_start + i] =
                        decoded[dst_start + i].wrapping_add(((left as u16 + up as u16) / 2) as u8);
                }
            }
            4 => {
                // Paeth
                let prev_start = if row > 0 { (row - 1) * row_bytes } else { 0 };
                for i in 0..row_bytes {
                    let left = if i >= bpp {
                        decoded[dst_start + i - bpp] as i16
                    } else {
                        0
                    };
                    let up = if row > 0 {
                        decoded[prev_start + i] as i16
                    } else {
                        0
                    };
                    let up_left = if i >= bpp && row > 0 {
                        decoded[prev_start + i - bpp] as i16
                    } else {
                        0
                    };
                    decoded[dst_start + i] =
                        decoded[dst_start + i].wrapping_add(paeth_predictor(left, up, up_left));
                }
            }
            _ => return None,
        }
    }

    // Separate color and alpha channels, writing with filter 0 (None).
    let color_stride = 1 + w * color_channels * bytes_per_sample;
    let alpha_stride = 1 + w * bytes_per_sample;
    let mut color_raw = Vec::with_capacity(h * color_stride);
    let mut alpha_raw = Vec::with_capacity(h * alpha_stride);

    for row in 0..h {
        color_raw.push(0); // filter byte = None
        alpha_raw.push(0); // filter byte = None
        let row_start = row * row_bytes;
        for x in 0..w {
            let pixel_start = row_start + x * total_channels * bytes_per_sample;
            // Color channels
            for c in 0..color_channels {
                let offset = pixel_start + c * bytes_per_sample;
                color_raw.extend_from_slice(&decoded[offset..offset + bytes_per_sample]);
            }
            // Alpha channel
            let alpha_offset = pixel_start + color_channels * bytes_per_sample;
            alpha_raw.extend_from_slice(&decoded[alpha_offset..alpha_offset + bytes_per_sample]);
        }
    }

    // Recompress both streams with zlib.
    let idat_data = zlib_compress(&color_raw)?;
    let smask = zlib_compress(&alpha_raw)?;

    Some(PngInfo {
        width,
        height,
        bit_depth,
        colors: color_channels as u8,
        idat_data,
        palette: None,
        smask: Some(smask),
    })
}

/// Paeth predictor function used by PNG filter type 4.
fn paeth_predictor(a: i16, b: i16, c: i16) -> u8 {
    let p = a + b - c;
    let pa = (p - a).unsigned_abs();
    let pb = (p - b).unsigned_abs();
    let pc = (p - c).unsigned_abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

/// Compress data with zlib (deflate).
fn zlib_compress(data: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    if encoder.write_all(data).is_err() {
        return None;
    }
    encoder.finish().ok()
}

// ---------------------------------------------------------------------------
// Embedded image types
// ---------------------------------------------------------------------------

/// Format-specific data for an embedded image.
enum ImageFormat {
    /// Raw JPEG data — embedded with `/DCTDecode` (passthrough, no re-encoding).
    Jpeg {
        /// Raw JPEG file bytes.
        data: Vec<u8>,
        /// Number of color components (1 = gray, 3 = RGB, 4 = CMYK).
        components: u8,
    },
    /// PNG image data — IDAT chunks concatenated, embedded with `/FlateDecode`.
    Png {
        /// Concatenated raw IDAT chunk payloads (zlib-compressed pixel data).
        idat_data: Vec<u8>,
        /// Bit depth (typically 8).
        bit_depth: u8,
        /// Number of color channels (1 = gray, 3 = RGB) after alpha removal.
        colors: u8,
        /// PLTE chunk data for indexed color images (color type 3).
        palette: Option<Vec<u8>>,
        /// Zlib-compressed alpha channel for images with transparency.
        /// Stored as a separate `/FlateDecode` stream for the PDF SMask.
        smask: Option<Vec<u8>>,
    },
}

/// An embedded image with its pixel dimensions and format-specific data.
struct EmbeddedImage {
    /// Image width in pixels.
    width: u32,
    /// Image height in pixels.
    height: u32,
    /// Format-specific data.
    format: ImageFormat,
}

impl EmbeddedImage {
    /// Returns the number of PDF objects this image will produce.
    ///
    /// JPEG images and PNG images without alpha need 1 object (the XObject).
    /// PNG images with an alpha channel need 2 objects (XObject + SMask).
    fn num_pdf_objects(&self) -> usize {
        match &self.format {
            ImageFormat::Jpeg { .. } => 1,
            ImageFormat::Png { smask, .. } => {
                if smask.is_some() {
                    2
                } else {
                    1
                }
            }
        }
    }
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
    /// Embedded images (JPEG or PNG).
    images: Vec<EmbeddedImage>,
    /// Layout margins (in points), overridable via config.
    margin_top: f32,
    margin_bottom: f32,
    margin_left: f32,
    margin_right: f32,
}

impl PdfDocument {
    /// Create a new document with one empty page using default margins.
    #[cfg(test)]
    fn new() -> Self {
        Self::with_margins(MARGIN_TOP, MARGIN_BOTTOM, MARGIN_LEFT, MARGIN_RIGHT)
    }

    /// Create a new document with one empty page and custom margins.
    fn with_margins(top: f32, bottom: f32, left: f32, right: f32) -> Self {
        Self {
            pages: vec![Vec::new()],
            y: PAGE_H - top,
            num_columns: 1,
            current_column: 0,
            images: Vec::new(),
            margin_top: top,
            margin_bottom: bottom,
            margin_left: left,
            margin_right: right,
        }
    }

    /// Maximum allowed margin value in points. A4 short side is 595pt, so
    /// margins above half that are unreasonable.
    const MAX_MARGIN: f32 = 297.0;

    /// Validate and clamp a margin value. Returns the default if the value is
    /// negative, non-finite, or exceeds `MAX_MARGIN`.
    fn validate_margin(value: f32, default: f32, name: &str, warnings: &mut Vec<String>) -> f32 {
        if !value.is_finite() || !(0.0..=Self::MAX_MARGIN).contains(&value) {
            warnings.push(format!(
                "invalid pdf.margins.{name} value {value}, using default {default}"
            ));
            default
        } else {
            value
        }
    }

    /// Create a new document reading margins from config.
    ///
    /// Warnings are printed to stderr. Use [`from_config_with_warnings`](Self::from_config_with_warnings)
    /// to capture them programmatically.
    #[cfg(test)]
    fn from_config(config: &Config) -> Self {
        let mut warnings = Vec::new();
        let doc = Self::from_config_with_warnings(config, &mut warnings);
        for w in &warnings {
            eprintln!("warning: {w}");
        }
        doc
    }

    /// Create a new document reading margins from config, collecting warnings.
    fn from_config_with_warnings(config: &Config, warnings: &mut Vec<String>) -> Self {
        let top = config
            .get_path("pdf.margins.top")
            .as_f64()
            .map(|v| Self::validate_margin(v as f32, MARGIN_TOP, "top", warnings))
            .unwrap_or(MARGIN_TOP);
        let bottom = config
            .get_path("pdf.margins.bottom")
            .as_f64()
            .map(|v| Self::validate_margin(v as f32, MARGIN_BOTTOM, "bottom", warnings))
            .unwrap_or(MARGIN_BOTTOM);
        let left = config
            .get_path("pdf.margins.left")
            .as_f64()
            .map(|v| Self::validate_margin(v as f32, MARGIN_LEFT, "left", warnings))
            .unwrap_or(MARGIN_LEFT);
        let right = config
            .get_path("pdf.margins.right")
            .as_f64()
            .map(|v| Self::validate_margin(v as f32, MARGIN_RIGHT, "right", warnings))
            .unwrap_or(MARGIN_RIGHT);
        Self::with_margins(top, bottom, left, right)
    }

    /// Reset the document margins from a config, falling back to the built-in
    /// defaults for any margin not explicitly set. Used in multi-song rendering
    /// so that per-song overrides from song N do not bleed into song N+1.
    fn reset_margins_from_config(&mut self, config: &Config, warnings: &mut Vec<String>) {
        self.margin_top = config
            .get_path("pdf.margins.top")
            .as_f64()
            .map(|v| Self::validate_margin(v as f32, MARGIN_TOP, "top", warnings))
            .unwrap_or(MARGIN_TOP);
        self.margin_bottom = config
            .get_path("pdf.margins.bottom")
            .as_f64()
            .map(|v| Self::validate_margin(v as f32, MARGIN_BOTTOM, "bottom", warnings))
            .unwrap_or(MARGIN_BOTTOM);
        self.margin_left = config
            .get_path("pdf.margins.left")
            .as_f64()
            .map(|v| Self::validate_margin(v as f32, MARGIN_LEFT, "left", warnings))
            .unwrap_or(MARGIN_LEFT);
        self.margin_right = config
            .get_path("pdf.margins.right")
            .as_f64()
            .map(|v| Self::validate_margin(v as f32, MARGIN_RIGHT, "right", warnings))
            .unwrap_or(MARGIN_RIGHT);
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
            return self.margin_left;
        }
        let col_width = self.column_width();
        let result = self.margin_left + self.current_column as f32 * (col_width + COLUMN_GAP);
        debug_assert!(
            result.is_finite(),
            "margin_left() produced non-finite value"
        );
        result
    }

    /// Returns the width of a single column in points.
    ///
    /// For single-column layouts this equals the full printable width.
    /// For multi-column layouts it accounts for inter-column gaps.
    fn column_width(&self) -> f32 {
        let usable_width = PAGE_W - self.margin_left - self.margin_right;
        if self.num_columns <= 1 {
            return usable_width;
        }
        let total_gaps = (self.num_columns - 1) as f32 * COLUMN_GAP;
        ((usable_width - total_gaps) / self.num_columns as f32).max(0.0)
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
            self.y = PAGE_H - self.margin_top;
        } else {
            self.new_page();
        }
    }

    /// Ensure there is at least `needed` points of vertical space remaining.
    /// If not, advance to next column or start a new page.
    fn ensure_space(&mut self, needed: f32) {
        if self.y - needed < self.margin_bottom {
            self.next_column_or_page();
        }
    }

    /// Advance to the next column, or to a new page if in the last column.
    fn next_column_or_page(&mut self) {
        if self.num_columns > 1 && self.current_column + 1 < self.num_columns {
            self.current_column += 1;
            self.y = PAGE_H - self.margin_top;
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
        self.y = PAGE_H - self.margin_top;
        self.current_column = 0;
    }

    /// Emit text at the current column margin and Y position.
    fn text(&mut self, text: &str, font: Font, size: f32) {
        let x = self.margin_left();
        self.text_at(text, font, size, x, self.y);
    }

    /// Emit text at an explicit (x, y) position without clipping.
    ///
    /// Used by callers that manage their own clipping context (e.g.,
    /// `render_lyrics_spans` which wraps the entire line in a single clip).
    fn text_at_raw(&mut self, text: &str, font: Font, size: f32, x: f32, y: f32) {
        let ops = self.current_page_mut();
        ops.push("BT".to_string());
        ops.push(format!("{} {} Tf", font.pdf_name(), fmt_f32(size)));
        ops.push(format!("{} {} Td", fmt_f32(x), fmt_f32(y)));
        ops.push(format!("({}) Tj", pdf_escape(text)));
        ops.push("ET".to_string());
    }

    /// Emit text at an explicit (x, y) position.
    ///
    /// In multi-column layouts, a clipping rectangle is applied to prevent
    /// text from overflowing the column boundary into adjacent columns.
    fn text_at(&mut self, text: &str, font: Font, size: f32, x: f32, y: f32) {
        let clip = self.num_columns > 1;
        // Pre-compute column right edge before borrowing self mutably.
        let col_right = if clip {
            self.margin_left() + self.column_width()
        } else {
            0.0
        };
        let ops = self.current_page_mut();
        if clip {
            let clip_w = (col_right - x).max(0.0);
            ops.push("q".to_string());
            ops.push(format!(
                "{} {} {} {} re W n",
                fmt_f32(x),
                fmt_f32(0.0),
                fmt_f32(clip_w),
                fmt_f32(PAGE_H)
            ));
        }
        ops.push("BT".to_string());
        ops.push(format!("{} {} Tf", font.pdf_name(), fmt_f32(size)));
        ops.push(format!("{} {} Td", fmt_f32(x), fmt_f32(y)));
        ops.push(format!("({}) Tj", pdf_escape(text)));
        ops.push("ET".to_string());
        if clip {
            ops.push("Q".to_string());
        }
    }

    /// Emit a text string at absolute coordinates in white.
    ///
    /// Used for finger numbers inside filled dots in chord diagrams.
    /// In multi-column layouts, applies the same clipping as [`text_at`].
    fn white_text_at(&mut self, text: &str, font: Font, size: f32, x: f32, y: f32) {
        let clip = self.num_columns > 1;
        let col_right = if clip {
            self.margin_left() + self.column_width()
        } else {
            0.0
        };
        let ops = self.current_page_mut();
        if clip {
            let clip_w = (col_right - x).max(0.0);
            ops.push("q".to_string());
            ops.push(format!(
                "{} {} {} {} re W n",
                fmt_f32(x),
                fmt_f32(0.0),
                fmt_f32(clip_w),
                fmt_f32(PAGE_H)
            ));
        }
        ops.push("BT".to_string());
        ops.push("1 1 1 rg".to_string()); // white fill color
        ops.push(format!("{} {} Tf", font.pdf_name(), fmt_f32(size)));
        ops.push(format!("{} {} Td", fmt_f32(x), fmt_f32(y)));
        ops.push(format!("({}) Tj", pdf_escape(text)));
        ops.push("ET".to_string());
        ops.push("0 0 0 rg".to_string()); // reset to black
        if clip {
            ops.push("Q".to_string());
        }
    }

    /// Move the Y cursor down. May trigger a column/page break if past bottom margin.
    fn newline(&mut self, amount: f32) {
        self.y -= amount;
        if self.y < self.margin_bottom {
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
    ///
    /// Wraps the operation in `q`/`Q` (save/restore graphics state) so the
    /// line width does not leak to subsequent drawing operations.
    fn line_at(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, width: f32) {
        let ops = self.current_page_mut();
        ops.push("q".to_string());
        ops.push(format!("{} w", fmt_f32(width)));
        ops.push(format!(
            "{} {} m {} {} l S",
            fmt_f32(x1),
            fmt_f32(y1),
            fmt_f32(x2),
            fmt_f32(y2)
        ));
        ops.push("Q".to_string());
    }

    /// Draw a stroked (unfilled) rectangle.
    ///
    /// Wraps the operation in `q`/`Q` (save/restore graphics state) so the
    /// line width does not leak to subsequent drawing operations.
    fn rect_stroke(&mut self, x: f32, y: f32, w: f32, h: f32, line_width: f32) {
        let ops = self.current_page_mut();
        ops.push("q".to_string());
        ops.push(format!("{} w", fmt_f32(line_width)));
        ops.push(format!(
            "{} {} {} {} re S",
            fmt_f32(x),
            fmt_f32(y),
            fmt_f32(w),
            fmt_f32(h)
        ));
        ops.push("Q".to_string());
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
    fn embed_jpeg(&mut self, data: Vec<u8>, width: u32, height: u32, components: u8) -> usize {
        let idx = self.images.len();
        self.images.push(EmbeddedImage {
            width,
            height,
            format: ImageFormat::Jpeg { data, components },
        });
        idx
    }

    /// Store a PNG image and return its index for later drawing.
    ///
    /// The IDAT data is stored as zlib-compressed bytes; the PDF will use
    /// `/FlateDecode` with PNG predictor parameters.
    fn embed_png(&mut self, info: PngInfo) -> usize {
        let idx = self.images.len();
        self.images.push(EmbeddedImage {
            width: info.width,
            height: info.height,
            format: ImageFormat::Png {
                idat_data: info.idat_data,
                bit_depth: info.bit_depth,
                colors: info.colors,
                palette: info.palette,
                smask: info.smask,
            },
        });
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
        self.y = PAGE_H - self.margin_top;
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

        // Image XObject references for the Resources dict.
        // Each image is referenced as /Im{i+1}. We compute the actual object
        // number by accumulating num_pdf_objects() for each preceding image.
        let image_obj_base = 3 + FONTS.len(); // first image object number
        let xobject_refs = if num_images > 0 {
            let mut refs = Vec::new();
            let mut obj_offset = 0;
            for (i, img) in self.images.iter().enumerate() {
                refs.push(format!("/Im{} {} 0 R", i + 1, image_obj_base + obj_offset));
                obj_offset += img.num_pdf_objects();
            }
            format!(" /XObject << {} >>", refs.join(" "))
        } else {
            String::new()
        };

        let procset = if num_images > 0 {
            "/ProcSet [/PDF /Text /ImageB /ImageC]"
        } else {
            "/ProcSet [/PDF /Text]"
        };

        // Kids: page objects start after fonts + all image-related objects.
        let total_image_objects: usize = self.images.iter().map(|img| img.num_pdf_objects()).sum();
        let page_obj_start = 3 + FONTS.len() + total_image_objects;
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

        // Image XObject streams
        for img in &self.images {
            match &img.format {
                ImageFormat::Jpeg { data, components } => {
                    offsets.push(pdf.len());
                    let obj_num = offsets.len();
                    let color_space = match components {
                        1 => "/DeviceGray",
                        4 => "/DeviceCMYK",
                        _ => "/DeviceRGB",
                    };
                    let header = format!(
                        "{} 0 obj\n<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
                        obj_num,
                        img.width,
                        img.height,
                        color_space,
                        data.len()
                    );
                    pdf.extend_from_slice(header.as_bytes());
                    pdf.extend_from_slice(data);
                    pdf.extend_from_slice(b"\nendstream\nendobj\n");
                }
                ImageFormat::Png {
                    idat_data,
                    bit_depth,
                    colors,
                    palette,
                    smask,
                } => {
                    // If there is an SMask, write it first so we know its
                    // object number when writing the main image.
                    let smask_obj_num = if smask.is_some() {
                        offsets.push(pdf.len());
                        let sobj = offsets.len();
                        let smask_data = smask.as_ref().expect("checked above");
                        let smask_header = format!(
                            "{} 0 obj\n<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray /BitsPerComponent {} /Filter /FlateDecode /DecodeParms << /Predictor 15 /Colors 1 /BitsPerComponent {} /Columns {} >> /Length {} >>\nstream\n",
                            sobj,
                            img.width,
                            img.height,
                            bit_depth,
                            bit_depth,
                            img.width,
                            smask_data.len()
                        );
                        pdf.extend_from_slice(smask_header.as_bytes());
                        pdf.extend_from_slice(smask_data);
                        pdf.extend_from_slice(b"\nendstream\nendobj\n");
                        Some(sobj)
                    } else {
                        None
                    };

                    offsets.push(pdf.len());
                    let obj_num = offsets.len();

                    let color_space = match (colors, palette) {
                        (_, Some(pal)) => {
                            // Indexed color: /ColorSpace [/Indexed /DeviceRGB N <hex>]
                            let num_entries = pal.len() / 3;
                            let max_idx = if num_entries > 0 { num_entries - 1 } else { 0 };
                            let hex: String = pal.iter().map(|b| format!("{b:02x}")).collect();
                            format!("[/Indexed /DeviceRGB {} <{}>]", max_idx, hex)
                        }
                        (1, None) => "/DeviceGray".to_string(),
                        _ => "/DeviceRGB".to_string(),
                    };

                    let smask_ref = smask_obj_num
                        .map(|n| format!(" /SMask {} 0 R", n))
                        .unwrap_or_default();

                    let header = format!(
                        "{} 0 obj\n<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent {} /Filter /FlateDecode /DecodeParms << /Predictor 15 /Colors {} /BitsPerComponent {} /Columns {} >>{} /Length {} >>\nstream\n",
                        obj_num,
                        img.width,
                        img.height,
                        color_space,
                        bit_depth,
                        colors,
                        bit_depth,
                        img.width,
                        smask_ref,
                        idat_data.len()
                    );
                    pdf.extend_from_slice(header.as_bytes());
                    pdf.extend_from_slice(idat_data);
                    pdf.extend_from_slice(b"\nendstream\nendobj\n");
                }
            }
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
            // Per ISO 32000: /Length is the number of bytes between `stream\n`
            // and the EOL marker before `endstream`. The `\n` preceding
            // `endstream` is excluded from the length.
            let stream_obj = format!(
                "{} 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
                content_obj_num,
                content.len(),
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
///
/// Characters are handled as follows:
/// - ASCII (U+0000–U+007F): passed through (with `\`, `(`, `)` escaped).
/// - WinAnsiEncoding 0x80–0x9F: Unicode characters that map to bytes 0x80–0x9F
///   in WinAnsiEncoding (Euro sign, smart quotes, dashes, etc.).
/// - Latin-1 Supplement (U+00A0–U+00FF): encoded as PDF octal escapes
///   (`\NNN`) for WinAnsiEncoding compatibility. This covers most accented
///   European characters (é, ü, ñ, ß, etc.).
/// - All other non-ASCII characters: replaced with `?` because the built-in
///   Type1 fonts (Helvetica) only support WinAnsiEncoding.
fn pdf_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            _ if c.is_ascii() => out.push(c),
            // Latin-1 Supplement: WinAnsiEncoding byte equals the code point.
            '\u{00A0}'..='\u{00FF}' => {
                let byte = c as u32;
                out.push_str(&format!("\\{byte:03o}"));
            }
            // WinAnsiEncoding 0x80–0x9F: Unicode characters that don't match
            // their code points but have specific byte mappings.
            _ => {
                if let Some(byte) = winansi_byte(c) {
                    out.push_str(&format!("\\{byte:03o}"));
                } else {
                    out.push('?');
                }
            }
        }
    }
    out
}

/// Map Unicode characters to WinAnsiEncoding bytes in the 0x80–0x9F range.
///
/// These characters have code points outside the Latin-1 Supplement range but
/// are assigned specific byte values in WinAnsiEncoding. Common in song lyrics:
/// smart quotes, em/en dashes, Euro sign, etc.
fn winansi_byte(c: char) -> Option<u32> {
    match c {
        '\u{20AC}' => Some(0x80), // € Euro sign
        '\u{201A}' => Some(0x82), // ‚ Single low-9 quotation mark
        '\u{0192}' => Some(0x83), // ƒ Latin small f with hook
        '\u{201E}' => Some(0x84), // „ Double low-9 quotation mark
        '\u{2026}' => Some(0x85), // … Horizontal ellipsis
        '\u{2020}' => Some(0x86), // † Dagger
        '\u{2021}' => Some(0x87), // ‡ Double dagger
        '\u{02C6}' => Some(0x88), // ˆ Modifier letter circumflex accent
        '\u{2030}' => Some(0x89), // ‰ Per mille sign
        '\u{0160}' => Some(0x8A), // Š Latin capital S with caron
        '\u{2039}' => Some(0x8B), // ‹ Single left-pointing angle quotation
        '\u{0152}' => Some(0x8C), // Œ Latin capital ligature OE
        '\u{017D}' => Some(0x8E), // Ž Latin capital Z with caron
        '\u{2018}' => Some(0x91), // ' Left single quotation mark
        '\u{2019}' => Some(0x92), // ' Right single quotation mark
        '\u{201C}' => Some(0x93), // " Left double quotation mark
        '\u{201D}' => Some(0x94), // " Right double quotation mark
        '\u{2022}' => Some(0x95), // • Bullet
        '\u{2013}' => Some(0x96), // – En dash
        '\u{2014}' => Some(0x97), // — Em dash
        '\u{02DC}' => Some(0x98), // ˜ Small tilde
        '\u{2122}' => Some(0x99), // ™ Trade mark sign
        '\u{0161}' => Some(0x9A), // š Latin small s with caron
        '\u{203A}' => Some(0x9B), // › Single right-pointing angle quotation
        '\u{0153}' => Some(0x9C), // œ Latin small ligature oe
        '\u{017E}' => Some(0x9E), // ž Latin small z with caron
        '\u{0178}' => Some(0x9F), // Ÿ Latin capital Y with diaeresis
        _ => None,
    }
}

/// Format f32 without trailing zeros for compact PDF output.
///
/// Non-finite values (NaN, ±Infinity) are replaced with `"0"` to prevent
/// malformed PDF operators.
fn fmt_f32(v: f32) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
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
        let song = chordsketch_core::parse("{title: Test}\n[Am]Hello [G]world").unwrap();
        let bytes = render_song(&song);
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn test_empty_song() {
        let song = chordsketch_core::parse("").unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        // Should contain the title text in the content stream
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Amazing Grace"));
    }

    #[test]
    fn test_stream_length_matches_content() {
        let song = chordsketch_core::parse("{title: Test}\n[Am]Hello").unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);

        // Find all /Length N declarations and verify they match the actual
        // stream content between "stream\n" and "\nendstream".
        for length_match in content.match_indices("/Length ") {
            let after = &content[length_match.0 + 8..];
            let end = after.find(' ').or_else(|| after.find('>')).unwrap();
            let declared_len: usize = after[..end].trim().parse().unwrap();

            // Find the stream start after this /Length
            let stream_start_offset =
                length_match.0 + content[length_match.0..].find("stream\n").unwrap() + 7;
            let endstream_offset =
                length_match.0 + content[length_match.0..].find("\nendstream").unwrap();
            let actual_len = endstream_offset - stream_start_offset;
            assert_eq!(
                declared_len, actual_len,
                "/Length {declared_len} does not match actual stream size {actual_len}"
            );
        }
    }

    #[test]
    fn test_pdf_escape() {
        assert_eq!(pdf_escape("hello"), "hello");
        assert_eq!(pdf_escape("a(b)c"), "a\\(b\\)c");
        assert_eq!(pdf_escape("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_pdf_escape_latin1_accented() {
        // é (U+00E9) → octal 351
        assert_eq!(pdf_escape("café"), "caf\\351");
        // ü (U+00FC) → octal 374
        assert_eq!(pdf_escape("über"), "\\374ber");
        // ñ (U+00F1) → octal 361
        assert_eq!(pdf_escape("España"), "Espa\\361a");
        // ß (U+00DF) → octal 337
        assert_eq!(pdf_escape("Straße"), "Stra\\337e");
    }

    #[test]
    fn test_pdf_escape_non_latin1_replaced() {
        // CJK and emoji are outside WinAnsiEncoding
        assert_eq!(pdf_escape("日本語"), "???");
        assert_eq!(pdf_escape("hello 世界"), "hello ??");
    }

    #[test]
    fn test_pdf_escape_mixed_ascii_latin1() {
        assert_eq!(pdf_escape("résumé"), "r\\351sum\\351");
        // Non-breaking space (U+00A0) → octal 240
        assert_eq!(pdf_escape("a\u{00A0}b"), "a\\240b");
    }

    #[test]
    fn test_pdf_escape_winansi_0x80_range() {
        // Euro sign (U+20AC → 0x80)
        assert_eq!(pdf_escape("\u{20AC}"), "\\200");
        // Left single quotation mark (U+2018 → 0x91)
        assert_eq!(pdf_escape("\u{2018}"), "\\221");
        // Right single quotation mark (U+2019 → 0x92)
        assert_eq!(pdf_escape("\u{2019}"), "\\222");
        // Left double quotation mark (U+201C → 0x93)
        assert_eq!(pdf_escape("\u{201C}"), "\\223");
        // Right double quotation mark (U+201D → 0x94)
        assert_eq!(pdf_escape("\u{201D}"), "\\224");
        // En dash (U+2013 → 0x96)
        assert_eq!(pdf_escape("\u{2013}"), "\\226");
        // Em dash (U+2014 → 0x97)
        assert_eq!(pdf_escape("\u{2014}"), "\\227");
        // Horizontal ellipsis (U+2026 → 0x85)
        assert_eq!(pdf_escape("\u{2026}"), "\\205");
        // Trade mark sign (U+2122 → 0x99)
        assert_eq!(pdf_escape("\u{2122}"), "\\231");
        // Bullet (U+2022 → 0x95)
        assert_eq!(pdf_escape("\u{2022}"), "\\225");
    }

    #[test]
    fn test_pdf_escape_winansi_mixed() {
        // Smart quotes in lyrics: "Don't stop"
        assert_eq!(
            pdf_escape("\u{201C}Don\u{2019}t stop\u{201D}"),
            "\\223Don\\222t stop\\224"
        );
        // Price with Euro sign: €50
        assert_eq!(pdf_escape("\u{20AC}50"), "\\20050");
    }

    #[test]
    fn test_render_grid_section() {
        let input = "{start_of_grid}\n| Am . | C . |\n{end_of_grid}";
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Chorus: Repeat"));
    }

    #[test]
    fn test_chorus_recall_no_chorus_defined() {
        let input = "{chorus}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Chorus"));
    }

    #[test]
    fn test_chorus_recall_limit_exceeded() {
        let mut input = String::from("{start_of_chorus}\nChorus line\n{end_of_chorus}\n");
        for _ in 0..1005 {
            input.push_str("{chorus}\n");
        }
        let song = chordsketch_core::parse(&input).unwrap();
        let result = render_song_with_warnings(&song, 0, &Config::defaults());
        assert!(result.output.starts_with(b"%PDF"));
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("chorus recall limit")),
            "should warn when chorus recall limit is exceeded"
        );
    }

    #[test]
    fn test_chorus_recall_respects_diagrams_off() {
        // When {diagrams: off} is active, chorus recall should not render
        // chord diagrams from {define} directives inside the chorus body.
        let input = "\
{diagrams: off}
{start_of_chorus}
{define: Am base-fret 1 frets x 0 2 2 1 0}
[Am]Chorus line
{end_of_chorus}
{chorus}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // Chord diagrams use Bezier curves for finger dots. When diagrams are
        // off, no circle drawing operations should appear from the diagram renderer.
        // Compare against a render with diagrams on to ensure the assertion is meaningful.
        let input_on = "\
{start_of_chorus}
{define: Am base-fret 1 frets x 0 2 2 1 0}
[Am]Chorus line
{end_of_chorus}
{chorus}";
        let song_on = chordsketch_core::parse(input_on).unwrap();
        let bytes_on = render_song(&song_on);
        let content_on = String::from_utf8_lossy(&bytes_on);
        // "l S" counts PDF lineto (l) + stroke (S) operations emitted by
        // render_chord_diagram_pdf for fret grid lines, nut, and string lines.
        let diagram_lines_off = content.matches("l S").count();
        let diagram_lines_on = content_on.matches("l S").count();
        assert!(
            diagram_lines_on > diagram_lines_off,
            "diagrams=on should produce more line ops than diagrams=off"
        );
    }

    // --- Case-insensitive {diagrams} directive (#652) ---

    #[test]
    fn test_diagrams_off_case_insensitive_pdf() {
        let input = "{diagrams: Off}\n{define: Am base-fret 1 frets x 0 2 2 1 0}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // When diagrams are suppressed the chord-name title ("Am") written by
        // render_chord_diagram_pdf must not appear in the PDF content stream.
        // This input has no lyrics, so "Am" would only appear as the diagram title.
        assert!(
            !content.contains("Am"),
            "diagrams=Off should suppress diagrams in PDF (case-insensitive)"
        );
    }

    #[test]
    fn test_diagrams_off_uppercase_pdf() {
        let input = "{diagrams: OFF}\n{define: Am base-fret 1 frets x 0 2 2 1 0}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // When diagrams are suppressed the chord-name title ("Am") written by
        // render_chord_diagram_pdf must not appear in the PDF content stream.
        // This input has no lyrics, so "Am" would only appear as the diagram title.
        assert!(
            !content.contains("Am"),
            "diagrams=OFF should suppress diagrams in PDF (case-insensitive)"
        );
    }

    #[test]
    fn test_custom_section_solo_in_pdf() {
        let input = "{start_of_solo}\n[Em]Solo\n{end_of_solo}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Solo"));
    }

    #[test]
    fn test_render_grid_section_with_label() {
        let input = "{start_of_grid: Intro}\n| Am |\n{end_of_grid}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Grid: Intro"));
    }

    #[test]
    fn test_define_display_name_in_pdf_output() {
        let input = "{define: Am base-fret 1 frets x 0 2 2 1 0 display=\"A minor\"}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("A minor"),
            "display name should appear in rendered PDF output"
        );
    }

    #[test]
    fn test_define_with_fingers_in_pdf_output() {
        let input = "{define: C base-fret 1 frets x 3 2 0 1 0 fingers 0 3 2 0 1 0}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // PDF text streams should contain finger numbers
        assert!(
            content.contains("(3)"),
            "finger numbers should appear in rendered PDF output"
        );
    }
}

#[cfg(test)]
mod comment_style_tests {
    use super::*;

    #[test]
    fn test_comment_normal_renders_text() {
        let input = "{comment: This is normal}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("This is normal"),
            "normal comment text should appear in PDF"
        );
    }

    #[test]
    fn test_comment_italic_renders_text() {
        let input = "{comment_italic: Italic note}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("Italic note"),
            "italic comment text should appear in PDF"
        );
    }

    #[test]
    fn test_comment_box_renders_with_rect() {
        let input = "{comment_box: Boxed note}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("Boxed note"),
            "boxed comment text should appear in PDF"
        );
        // Boxed comments use "re S" (rect stroke) PDF operator.
        assert!(
            content.contains("re S"),
            "boxed comment should draw a rectangle border"
        );
    }

    #[test]
    fn test_comment_normal_no_rect() {
        let input = "{comment: No box here}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            !content.contains("re S"),
            "normal comment should not draw a rectangle"
        );
    }
}

#[cfg(test)]
mod transpose_tests {
    use super::*;

    #[test]
    fn test_transpose_directive_produces_pdf() {
        let input = "{transpose: 2}\n[G]Hello [C]world";
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song_with_transpose(&song, 3, &Config::defaults());
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
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("ABC: Melody"));
    }

    #[test]
    fn test_ly_section_in_pdf() {
        let input = "{start_of_ly}\nnotes\n{end_of_ly}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Lilypond"));
    }

    #[test]
    fn test_svg_section_in_pdf() {
        let input = "{start_of_svg}\n<svg/>\n{end_of_svg}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("SVG"));
    }

    #[test]
    fn test_textblock_section_in_pdf() {
        let input = "{start_of_textblock}\nText\n{end_of_textblock}";
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/F3")); // HelveticaOblique
        assert!(content.contains("italic"));
    }

    #[test]
    fn test_bold_italic_markup_uses_bold_oblique_font() {
        let input = "<b><i>bold italic</i></b>";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/F4")); // HelveticaBoldOblique
        assert!(content.contains("bold italic"));
    }

    #[test]
    fn test_markup_with_chords_produces_valid_pdf() {
        let input = "[Am]Hello <b>bold</b> world";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Am"));
        assert!(content.contains("bold"));
    }

    #[test]
    fn test_span_weight_bold_uses_bold_font() {
        let input = r#"<span weight="bold">weighted</span>"#;
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // The PDF should use 14pt for lyrics text
        assert!(content.contains("14"));
        assert!(content.contains("Hello world"));
    }

    #[test]
    fn test_chordsize_directive_changes_chord_size() {
        let input = "{chordsize: 16}\n[Am]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Am"));
    }

    #[test]
    fn test_formatting_directive_produces_valid_pdf() {
        let input = "{textsize: 14}\n{chordsize: 12}\n[Am]Hello <b>bold</b> world";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_textsize_clamped_to_max() {
        let input = "{textsize: 99999}\nHello";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // Font size must be clamped to MAX_FONT_SIZE (200), not 99999.
        assert!(!content.contains("99999"));
        assert!(content.contains("200"));
    }

    #[test]
    fn test_textsize_clamped_to_min() {
        let input = "{textsize: -5}\nHello";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // Negative size must be clamped to MIN_FONT_SIZE (0.5).
        assert!(content.contains("0.5"));
    }

    #[test]
    fn test_chordsize_clamped_to_max() {
        let input = "{chordsize: 500}\n[Am]Hello";
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        // Should have /Count 2 in the Pages object
        assert!(content.contains("/Count 2"));
        assert!(content.contains("Page one"));
        assert!(content.contains("Page two"));
    }

    #[test]
    fn test_new_physical_page_from_recto_inserts_blank() {
        // Page 1 (recto) -> {npp} -> blank page 2 (verso) -> page 3 (recto)
        let input = "Page one\n{new_physical_page}\nPage two";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("/Count 3"),
            "new_physical_page from recto should insert blank page to reach next recto"
        );
    }

    #[test]
    fn test_new_physical_page_from_verso_no_extra_blank() {
        // Page 1 (recto) -> {np} -> page 2 (verso) -> {npp} -> page 3 (recto)
        let input = "Page one\n{new_page}\nPage two\n{new_physical_page}\nPage three";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            content.contains("/Count 3"),
            "new_physical_page from verso should go directly to next recto (no extra blank)"
        );
    }

    #[test]
    fn test_single_page_has_count_one() {
        let input = "{title: Short Song}\n[Am]Hello";
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(&input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/Count 3"));
    }

    #[test]
    fn test_multipage_pdf_structure_valid() {
        let input = "First page\n{new_page}\nSecond page";
        let song = chordsketch_core::parse(input).unwrap();
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

    #[test]
    fn test_page_control_not_replayed_in_chorus_recall() {
        // {new_page} inside a chorus must NOT create an extra page on {chorus} recall.
        let input = "\
{start_of_chorus}\n\
{new_page}\n\
[G]La la la\n\
{end_of_chorus}\n\
Verse text\n\
{chorus}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // The initial chorus {new_page} creates page 2.
        // The chorus recall should NOT create another page break.
        // Expected: exactly 2 pages (initial page + one {new_page}).
        assert!(
            content.contains("/Count 2"),
            "chorus recall must not replay page breaks"
        );
    }
}

#[cfg(test)]
mod column_tests {
    use super::*;

    #[test]
    fn test_columns_directive_produces_valid_pdf() {
        let input = "{columns: 2}\nColumn one\n{column_break}\nColumn two";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Column one"));
        assert!(content.contains("Column two"));
    }

    #[test]
    fn test_column_break_in_single_column_creates_new_page() {
        let input = "Page one\n{column_break}\nPage two";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("/Count 2"));
    }

    #[test]
    fn test_columns_reset_to_one() {
        let input = "{columns: 2}\nTwo cols\n{columns: 1}\nOne col";
        let song = chordsketch_core::parse(input).unwrap();
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
    fn test_margin_left_all_column_counts_positive() {
        for n in 1..=MAX_COLUMNS {
            let mut doc = PdfDocument::new();
            doc.set_columns(n);
            for col in 0..n {
                doc.current_column = col;
                let m = doc.margin_left();
                assert!(
                    m >= 0.0 && m.is_finite(),
                    "margin_left() must be non-negative and finite for columns={n}, col={col}, got {m}"
                );
            }
        }
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

    #[test]
    fn test_columns_non_numeric_defaults_to_one() {
        let input = "{columns: abc}\n[Am]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // Non-numeric value defaults to 1 column — should still render.
        assert!(content.contains("Am"));
        assert!(content.contains("Hello"));
    }

    #[test]
    fn test_multi_column_text_clipped() {
        // In a 2-column layout, text_at should emit clipping operators
        // (q/re W n/Q) to prevent overflow into adjacent columns.
        let input = "{columns: 2}\n[Am]Hello world this is a very long line of lyrics";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        // Multi-column layout should include clipping rectangle operator.
        assert!(
            content.contains("re W n"),
            "multi-column PDF should contain clipping rectangle operator"
        );
        // Verify clipping rectangle has reasonable column width.
        // Default 2-column: usable = 595 - 56 - 56 = 483, col_w = (483-20)/2 = 231.5
        // The clip rect should contain a width value around 231.
        let clip_line = content
            .lines()
            .find(|l| l.contains("re W n"))
            .expect("should find clip rect line");
        let parts: Vec<&str> = clip_line.split_whitespace().collect();
        // Format: "{x} {y} {w} {h} re W n"
        assert!(parts.len() >= 6, "clip rect should have x y w h re W n");
        let w: f32 = parts[2].parse().expect("width should be a number");
        assert!(
            w > 100.0 && w < 300.0,
            "clip width {w} should be a reasonable column width"
        );
    }

    #[test]
    fn test_single_column_no_clipping() {
        // Single-column layout should NOT emit clipping operators.
        let input = "[Am]Hello world";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(
            !content.contains("re W n"),
            "single-column PDF should not contain clipping operator"
        );
    }

    #[test]
    fn test_multi_column_inline_markup_single_clip_per_line() {
        // A lyrics line with inline markup (no chords) in 2-column mode
        // should produce exactly 1 clip rect from render_lyrics_spans,
        // not one per markup segment.
        let input = "{columns: 2}\nHello <b>bold</b> and <i>italic</i> text";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        let clip_count = content.matches("re W n").count();
        assert_eq!(
            clip_count, 1,
            "inline markup line should produce exactly 1 clip rect (got {clip_count})"
        );
    }

    // --- Multi-song rendering ---

    #[test]
    fn test_render_songs_single() {
        let songs = chordsketch_core::parse_multi("{title: Only}\n[Am]Hello").unwrap();
        let bytes = render_songs(&songs);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        // Single song: output should match render_song
        assert_eq!(bytes, render_song(&songs[0]));
    }

    #[test]
    fn test_render_songs_two_songs_multi_page() {
        let songs = chordsketch_core::parse_multi(
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
            chordsketch_core::parse_multi("{title: S1}\n[C]Do\n{new_song}\n{title: S2}\n[G]Re")
                .unwrap();
        let bytes = render_songs_with_transpose(&songs, 2, &Config::defaults());
        let content = String::from_utf8_lossy(&bytes);
        // C+2=D, G+2=A — both transposed chords should appear
        assert!(content.contains("(D)"));
        assert!(content.contains("(A)"));
    }

    #[test]
    fn test_render_song_into_doc_helper() {
        let song = chordsketch_core::parse("{title: Test}\n[Am]Hello").unwrap();
        let mut doc = PdfDocument::new();
        let mut warnings = Vec::new();
        render_song_into_doc(&song, 0, &Config::defaults(), &mut doc, &mut warnings);
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
        let songs = chordsketch_core::parse_multi(
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
        let song = chordsketch_core::parse("{title: Only Song}\nLyrics").unwrap();
        let bytes = render_song(&song);
        let content = String::from_utf8_lossy(&bytes);
        assert!(!content.contains("Table of Contents"));
    }

    #[test]
    fn test_toc_page_numbers_present() {
        let songs = chordsketch_core::parse_multi(
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
        let songs = chordsketch_core::parse_multi("{title: A}\nText\n{new_song}\n{title: B}\nText")
            .unwrap();
        let bytes = render_songs(&songs);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn test_toc_with_custom_margins_produces_valid_pdf() {
        use chordsketch_core::config::Config;
        let songs =
            chordsketch_core::parse_multi("{title: Song A}\nA\n{new_song}\n{title: Song B}\nB")
                .unwrap();
        // Use large custom margins to exercise the from_config path in the ToC rebuild.
        let config = Config::parse(
            r#"{ "pdf": { "margintop": 100, "marginbottom": 100, "marginleft": 100, "marginright": 100 } }"#,
        )
        .unwrap();
        let bytes = render_songs_with_transpose(&songs, 0, &config);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("Table of Contents"));
    }
}

#[cfg(test)]
mod chord_diagram_pdf_tests {
    use super::*;

    #[test]
    fn test_define_renders_diagram_in_pdf() {
        let input = "{define: Am base-fret 1 frets x 0 2 2 1 0}\n[Am]Hello";
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        // Should still be valid
    }

    #[test]
    fn test_define_diagram_valid_pdf() {
        let input = "{define: F base-fret 1 frets 1 1 2 3 3 1}\n[F]Lyrics";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn test_define_ukulele_diagram_in_pdf() {
        let input = "{define: C frets 0 0 0 3}\n[C]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
        let content = String::from_utf8_lossy(&bytes);
        assert!(content.contains("C"));
    }

    #[test]
    fn test_define_banjo_diagram_in_pdf() {
        let input = "{define: G frets 0 0 0 0 0}\n[G]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_diagrams_frets_config_affects_pdf_output() {
        let input = "{define: Am base-fret 1 frets x 0 2 2 1 0}\n[Am]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let config_4 = chordsketch_core::config::Config::defaults()
            .with_define("diagrams.frets=4")
            .unwrap();
        let config_7 = chordsketch_core::config::Config::defaults()
            .with_define("diagrams.frets=7")
            .unwrap();
        let bytes_4 = render_song_with_transpose(&song, 0, &config_4);
        let bytes_7 = render_song_with_transpose(&song, 0, &config_7);
        let content_4 = String::from_utf8_lossy(&bytes_4);
        let content_7 = String::from_utf8_lossy(&bytes_7);
        // Each line_at() emits "m ... l S". Count line-drawing operations:
        // frets=4 → 5 horizontal + 6 vertical + 1 nut = 12 lines
        // frets=7 → 8 horizontal + 6 vertical + 1 nut = 15 lines
        // The difference of 3 corresponds to the 3 extra fret lines.
        let lines_4 = content_4.matches("l S").count();
        let lines_7 = content_7.matches("l S").count();
        assert!(
            lines_7 >= lines_4,
            "frets=7 ({lines_7}) should have at least as many line ops as frets=4 ({lines_4})"
        );
        assert_eq!(
            lines_7 - lines_4,
            3,
            "frets=7 should produce exactly 3 more line-drawing ops than frets=4 \
             (got {lines_7} vs {lines_4})"
        );
    }

    #[test]
    fn test_render_chord_diagram_pdf_single_string_no_panic() {
        // Direct construction with strings=1 (below MIN_STRINGS) should
        // return early without panicking.
        let data = chordsketch_core::chord_diagram::DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: 1,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![0],
            fingers: vec![],
        };
        let mut doc = PdfDocument::new();
        render_chord_diagram_pdf(&data, &mut doc);
        // No panic = pass. The guard returned early.
    }

    #[test]
    fn test_render_chord_diagram_pdf_zero_strings_no_panic() {
        let data = chordsketch_core::chord_diagram::DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: 0,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![],
            fingers: vec![],
        };
        let mut doc = PdfDocument::new();
        render_chord_diagram_pdf(&data, &mut doc);
        // No panic = pass. The guard returned early.
    }

    #[test]
    fn test_render_chord_diagram_pdf_exceeding_max_strings_no_panic() {
        let data = chordsketch_core::chord_diagram::DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: chordsketch_core::chord_diagram::MAX_STRINGS + 1,
            frets_shown: 5,
            base_fret: 1,
            frets: vec![0; chordsketch_core::chord_diagram::MAX_STRINGS + 1],
            fingers: vec![],
        };
        let mut doc = PdfDocument::new();
        render_chord_diagram_pdf(&data, &mut doc);
        // No panic = pass. The guard returned early.
    }

    #[test]
    fn test_render_chord_diagram_pdf_zero_frets_shown_no_panic() {
        let data = chordsketch_core::chord_diagram::DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: 0,
            base_fret: 1,
            frets: vec![0; 6],
            fingers: vec![],
        };
        let mut doc = PdfDocument::new();
        render_chord_diagram_pdf(&data, &mut doc);
        // No panic = pass. The guard returned early.
    }

    #[test]
    fn test_render_chord_diagram_pdf_exceeding_max_frets_shown_no_panic() {
        let data = chordsketch_core::chord_diagram::DiagramData {
            name: "X".to_string(),
            display_name: None,
            strings: 6,
            frets_shown: chordsketch_core::chord_diagram::MAX_FRETS_SHOWN + 1,
            base_fret: 1,
            frets: vec![0; 6],
            fingers: vec![],
        };
        let mut doc = PdfDocument::new();
        render_chord_diagram_pdf(&data, &mut doc);
        // No panic = pass. The guard returned early.
    }
}

#[cfg(test)]
mod jpeg_tests {
    use super::*;

    /// Build a minimal valid JPEG byte sequence with a SOF0 marker.
    ///
    /// This is not a displayable image but contains a structurally correct
    /// JPEG header that `parse_jpeg_dimensions` can parse.
    /// Uses 3 color components (RGB) by default.
    fn minimal_jpeg(width: u16, height: u16) -> Vec<u8> {
        minimal_jpeg_with_components(width, height, 3)
    }

    /// Build a minimal JPEG with a specific number of color components.
    fn minimal_jpeg_with_components(width: u16, height: u16, components: u8) -> Vec<u8> {
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
        // Number of components
        data.push(components);
        data
    }

    #[test]
    fn test_parse_jpeg_dimensions_basic() {
        let jpeg = minimal_jpeg(640, 480);
        let dims = parse_jpeg_dimensions(&jpeg);
        assert_eq!(dims, Some((640, 480, 3)));
    }

    #[test]
    fn test_parse_jpeg_dimensions_square() {
        let jpeg = minimal_jpeg(100, 100);
        let dims = parse_jpeg_dimensions(&jpeg);
        assert_eq!(dims, Some((100, 100, 3)));
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
    fn test_parse_jpeg_dimensions_exceeds_scan_limit() {
        // Build a JPEG with valid SOI, then >64 KB of padding before the SOF.
        // The parser should bail out before reaching the SOF marker.
        let mut data = vec![0xFF, 0xD8]; // SOI
        // Fill with non-marker bytes (not 0xFF) to force byte-by-byte scanning
        data.resize(70_000, 0x00);
        // Append a valid SOF0 marker well past the 64 KB scan limit
        data.extend_from_slice(&[0xFF, 0xC0]);
        data.extend_from_slice(&[0x00, 0x08]);
        data.push(0x08);
        data.extend_from_slice(&100_u16.to_be_bytes());
        data.extend_from_slice(&200_u16.to_be_bytes());
        data.push(3);
        assert_eq!(
            parse_jpeg_dimensions(&data),
            None,
            "SOF beyond scan limit should not be found"
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
        data.push(0x00); // components
        let dims = parse_jpeg_dimensions(&data);
        assert_eq!(dims, Some((400, 300, 0)));
    }

    /// Build a minimal valid JPEG byte sequence with an arbitrary SOF marker.
    fn minimal_jpeg_with_sof(sof_marker: u8, width: u16, height: u16) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&[0xFF, 0xD8]); // SOI
        data.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x02]); // APP0
        data.extend_from_slice(&[0xFF, sof_marker]); // SOF marker
        data.extend_from_slice(&[0x00, 0x08]); // length
        data.push(0x08); // precision
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&width.to_be_bytes());
        data.push(0x00); // components
        data
    }

    #[test]
    fn test_parse_jpeg_dimensions_sof1_extended_sequential() {
        let data = minimal_jpeg_with_sof(0xC1, 800, 600);
        assert_eq!(parse_jpeg_dimensions(&data), Some((800, 600, 0)));
    }

    #[test]
    fn test_parse_jpeg_dimensions_sof3_lossless() {
        let data = minimal_jpeg_with_sof(0xC3, 1024, 768);
        assert_eq!(parse_jpeg_dimensions(&data), Some((1024, 768, 0)));
    }

    #[test]
    fn test_parse_jpeg_dimensions_sof9_arithmetic_sequential() {
        let data = minimal_jpeg_with_sof(0xC9, 320, 240);
        assert_eq!(parse_jpeg_dimensions(&data), Some((320, 240, 0)));
    }

    #[test]
    fn test_parse_jpeg_dimensions_sof10_arithmetic_progressive() {
        let data = minimal_jpeg_with_sof(0xCA, 1920, 1080);
        assert_eq!(parse_jpeg_dimensions(&data), Some((1920, 1080, 0)));
    }

    #[test]
    fn test_parse_jpeg_dimensions_sof11_arithmetic_lossless() {
        let data = minimal_jpeg_with_sof(0xCB, 256, 256);
        assert_eq!(parse_jpeg_dimensions(&data), Some((256, 256, 0)));
    }

    #[test]
    fn test_parse_jpeg_dimensions_all_sof_markers() {
        let sof_markers = [
            0xC0, 0xC1, 0xC2, 0xC3, // SOF0–SOF3
            0xC5, 0xC6, 0xC7, // SOF5–SOF7
            0xC9, 0xCA, 0xCB, // SOF9–SOF11
            0xCD, 0xCE, 0xCF, // SOF13–SOF15
        ];
        for marker in sof_markers {
            let data = minimal_jpeg_with_sof(marker, 500, 400);
            assert_eq!(
                parse_jpeg_dimensions(&data),
                Some((500, 400, 0)),
                "SOF marker 0x{marker:02X} should be recognized"
            );
        }
    }

    #[test]
    fn test_parse_jpeg_dimensions_non_sof_markers_not_matched() {
        // 0xC4 (DHT), 0xC8 (reserved), 0xCC (DAC) must NOT be treated as SOF.
        for marker in [0xC4, 0xC8, 0xCC] {
            let data = minimal_jpeg_with_sof(marker, 500, 400);
            assert_eq!(
                parse_jpeg_dimensions(&data),
                None,
                "Marker 0x{marker:02X} should NOT be recognized as SOF"
            );
        }
    }

    #[test]
    fn test_image_directive_nonexistent_file_no_crash() {
        let input = "{image: src=nonexistent_file_that_does_not_exist.jpg}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        // Should produce a valid PDF without crashing
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn test_image_directive_non_jpeg_skipped() {
        let input = "{image: src=photo.png}";
        let song = chordsketch_core::parse(input).unwrap();
        let bytes = render_song(&song);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        assert!(bytes.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn test_embed_jpeg_produces_xobject() {
        let jpeg = minimal_jpeg(320, 240);
        let mut doc = PdfDocument::new();
        let idx = doc.embed_jpeg(jpeg, 320, 240, 3);
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
    fn test_xobject_uses_actual_pixel_dimensions() {
        // Even if pixel dimensions exceed MAX_IMAGE_PIXELS, the XObject
        // /Width and /Height must reflect the actual JPEG stream data.
        let large_w: u32 = 20_000;
        let large_h: u32 = 15_000;
        let jpeg = minimal_jpeg_with_components(large_w as u16, large_h as u16, 3);
        let mut doc = PdfDocument::new();
        let idx = doc.embed_jpeg(jpeg, large_w, large_h, 3);
        doc.draw_image(idx, 56.0, 700.0, 100.0, 75.0);
        let pdf = doc.build_pdf();
        let content = String::from_utf8_lossy(&pdf);
        let width_str = format!("/Width {large_w}");
        let height_str = format!("/Height {large_h}");
        assert!(
            content.contains(&width_str),
            "XObject must contain actual width {large_w}"
        );
        assert!(
            content.contains(&height_str),
            "XObject must contain actual height {large_h}"
        );
    }

    #[test]
    fn test_embed_multiple_jpegs() {
        let jpeg1 = minimal_jpeg(100, 50);
        let jpeg2 = minimal_jpeg(200, 150);
        let mut doc = PdfDocument::new();
        let idx1 = doc.embed_jpeg(jpeg1, 100, 50, 3);
        let idx2 = doc.embed_jpeg(jpeg2, 200, 150, 3);
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
    fn test_embed_jpeg_grayscale_uses_device_gray() {
        let jpeg = minimal_jpeg_with_components(100, 100, 1);
        let mut doc = PdfDocument::new();
        let idx = doc.embed_jpeg(jpeg, 100, 100, 1);
        doc.draw_image(idx, 56.0, 700.0, 100.0, 100.0);
        let pdf = doc.build_pdf();
        let content = String::from_utf8_lossy(&pdf);
        assert!(
            content.contains("/ColorSpace /DeviceGray"),
            "grayscale JPEG should use /DeviceGray"
        );
        assert!(
            !content.contains("/ColorSpace /DeviceRGB"),
            "grayscale JPEG should not use /DeviceRGB"
        );
    }

    #[test]
    fn test_embed_jpeg_rgb_uses_device_rgb() {
        let jpeg = minimal_jpeg_with_components(100, 100, 3);
        let mut doc = PdfDocument::new();
        let idx = doc.embed_jpeg(jpeg, 100, 100, 3);
        doc.draw_image(idx, 56.0, 700.0, 100.0, 100.0);
        let pdf = doc.build_pdf();
        let content = String::from_utf8_lossy(&pdf);
        assert!(
            content.contains("/ColorSpace /DeviceRGB"),
            "RGB JPEG should use /DeviceRGB"
        );
    }

    #[test]
    fn test_embed_jpeg_cmyk_uses_device_cmyk() {
        let jpeg = minimal_jpeg_with_components(100, 100, 4);
        let mut doc = PdfDocument::new();
        let idx = doc.embed_jpeg(jpeg, 100, 100, 4);
        doc.draw_image(idx, 56.0, 700.0, 100.0, 100.0);
        let pdf = doc.build_pdf();
        let content = String::from_utf8_lossy(&pdf);
        assert!(
            content.contains("/ColorSpace /DeviceCMYK"),
            "CMYK JPEG should use /DeviceCMYK"
        );
    }

    #[test]
    fn test_parse_jpeg_dimensions_grayscale_component_count() {
        let jpeg = minimal_jpeg_with_components(200, 150, 1);
        let dims = parse_jpeg_dimensions(&jpeg);
        assert_eq!(dims, Some((200, 150, 1)));
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
        let idx = doc.embed_jpeg(jpeg, 50, 50, 3);
        doc.draw_image(idx, 100.0, 200.0, 50.0, 50.0);
        let ops = &doc.pages[0];
        assert!(ops.iter().any(|op| op == "q"));
        assert!(ops.iter().any(|op| op.contains("cm")));
        assert!(ops.iter().any(|op| op.contains("/Im1 Do")));
        assert!(ops.iter().any(|op| op == "Q"));
    }

    #[test]
    fn test_anchor_line_uses_margin_left() {
        // anchor=line (default) should place image at margin_left.
        let mut doc = PdfDocument::new();
        let jpeg = minimal_jpeg(100, 100);
        let idx = doc.embed_jpeg(jpeg, 100, 100, 3);
        let x = doc.margin_left();
        doc.draw_image(idx, x, 500.0, 100.0, 100.0);
        let cm_op = doc.pages[0]
            .iter()
            .find(|op| op.contains("cm"))
            .expect("cm operator");
        // The cm matrix puts tx at position 5 (0-indexed): "w 0 0 h tx ty cm"
        let tx: f32 = cm_op.split_whitespace().nth(4).unwrap().parse().unwrap();
        assert!(
            (tx - MARGIN_LEFT).abs() < 0.01,
            "expected tx ~{MARGIN_LEFT}, got {tx}"
        );
    }

    #[test]
    fn test_anchor_paper_centers_on_page() {
        // anchor=paper should place image centered on the full page width.
        let render_w: f32 = 200.0;
        let expected_x = (PAGE_W - render_w) / 2.0;
        let mut doc = PdfDocument::new();
        let jpeg = minimal_jpeg(200, 100);
        let idx = doc.embed_jpeg(jpeg, 200, 100, 3);
        doc.draw_image(idx, expected_x, 500.0, render_w, 100.0);
        let cm_op = doc.pages[0]
            .iter()
            .find(|op| op.contains("cm"))
            .expect("cm operator");
        let tx: f32 = cm_op.split_whitespace().nth(4).unwrap().parse().unwrap();
        assert!(
            (tx - expected_x).abs() < 0.01,
            "expected tx ~{expected_x}, got {tx}"
        );
    }

    #[test]
    fn test_anchor_column_centers_in_column_multicolumn() {
        // In a 2-column layout, anchor=column should center the image within
        // the column width that accounts for COLUMN_GAP, matching the formula
        // used by margin_left() and column_width().
        let mut doc = PdfDocument::new();
        doc.set_columns(2);
        // Column 0
        let col_w = doc.column_width();
        let col_left = doc.margin_left();

        let render_w: f32 = 100.0;
        let expected_x = col_left + (col_w - render_w) / 2.0;

        let jpeg = minimal_jpeg(100, 100);
        let idx = doc.embed_jpeg(jpeg, 100, 100, 3);
        doc.draw_image(idx, expected_x, 500.0, render_w, 100.0);
        let cm_op = doc.pages[0]
            .iter()
            .find(|op| op.contains("cm"))
            .expect("cm operator");
        let tx: f32 = cm_op.split_whitespace().nth(4).unwrap().parse().unwrap();
        assert!(
            (tx - expected_x).abs() < 0.01,
            "expected tx ~{expected_x}, got {tx}"
        );
    }

    #[test]
    fn test_column_width_single_column() {
        let doc = PdfDocument::new();
        let expected = PAGE_W - doc.margin_left - doc.margin_right;
        assert!((doc.column_width() - expected).abs() < 0.01);
    }

    #[test]
    fn test_column_width_multi_column() {
        let mut doc = PdfDocument::new();
        doc.set_columns(2);
        let usable = PAGE_W - doc.margin_left - doc.margin_right;
        let expected = (usable - COLUMN_GAP) / 2.0;
        assert!((doc.column_width() - expected).abs() < 0.01);
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

    #[test]
    fn test_compute_image_dimensions_percentage_width() {
        let attrs = ImageAttributes {
            width: Some("50%".to_string()),
            ..Default::default()
        };
        // 50% of 400 = 200, height derived from aspect ratio.
        let (w, h) = compute_image_dimensions(&attrs, 400.0, 300.0, 400.0 / 300.0);
        assert!((w - 200.0).abs() < 0.01);
        assert!((h - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_image_dimensions_percentage_height() {
        let attrs = ImageAttributes {
            height: Some("50%".to_string()),
            ..Default::default()
        };
        // 50% of 300 = 150, width derived from aspect ratio.
        let (w, h) = compute_image_dimensions(&attrs, 400.0, 300.0, 400.0 / 300.0);
        assert!((w - 200.0).abs() < 0.01);
        assert!((h - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_image_dimensions_percentage_both() {
        let attrs = ImageAttributes {
            width: Some("75%".to_string()),
            height: Some("50%".to_string()),
            ..Default::default()
        };
        // 75% of 400 = 300, 50% of 300 = 150
        let (w, h) = compute_image_dimensions(&attrs, 400.0, 300.0, 400.0 / 300.0);
        assert!((w - 300.0).abs() < 0.01);
        assert!((h - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_dimension_absolute() {
        assert!((parse_dimension("200", 400.0).unwrap() - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_dimension_percentage() {
        assert!((parse_dimension("50%", 400.0).unwrap() - 200.0).abs() < 0.01);
        assert!((parse_dimension(" 25% ", 800.0).unwrap() - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_dimension_invalid() {
        assert!(parse_dimension("", 400.0).is_none());
        assert!(parse_dimension("abc", 400.0).is_none());
        assert!(parse_dimension("-10", 400.0).is_none());
        assert!(parse_dimension("0%", 400.0).is_none());
        assert!(parse_dimension("-5%", 400.0).is_none());
    }

    #[test]
    fn test_parse_dimension_rejects_non_finite() {
        // Infinity via str::parse::<f32>
        assert!(parse_dimension("inf", 400.0).is_none());
        assert!(parse_dimension("infinity", 400.0).is_none());
        assert!(parse_dimension("Infinity", 400.0).is_none());
        // NaN
        assert!(parse_dimension("NaN", 400.0).is_none());
        // Infinity percentage
        assert!(parse_dimension("inf%", 400.0).is_none());
    }

    #[test]
    fn test_compute_image_dimensions_infinite_scale_rejected() {
        let attrs = ImageAttributes {
            src: String::new(),
            width: None,
            height: None,
            scale: Some("inf".to_string()),
            title: None,
            anchor: None,
        };
        // With infinite scale rejected, should fall back to native dimensions.
        let (w, h) = compute_image_dimensions(&attrs, 100.0, 200.0, 0.5);
        assert!((w - 100.0).abs() < 0.01);
        assert!((h - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_image_dimensions_nan_scale_rejected() {
        let attrs = ImageAttributes {
            src: String::new(),
            width: None,
            height: None,
            scale: Some("NaN".to_string()),
            title: None,
            anchor: None,
        };
        let (w, h) = compute_image_dimensions(&attrs, 100.0, 200.0, 0.5);
        assert!((w - 100.0).abs() < 0.01);
        assert!((h - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_oversized_image_file_is_skipped() {
        // Create a sparse file that exceeds MAX_IMAGE_FILE_SIZE using a
        // relative path so that is_safe_image_path() passes and the size
        // limit code path is actually exercised.
        //
        // Use a unique subdirectory name (PID + thread name) so parallel
        // test threads never collide on the same directory.
        let thread_name = std::thread::current()
            .name()
            .unwrap_or("main")
            .replace("::", "_");
        let subdir = format!("_test_oversized_img_{}_{}", std::process::id(), thread_name);
        let _ = std::fs::remove_dir_all(&subdir);
        std::fs::create_dir_all(&subdir).expect("create test dir");
        let rel_path = format!("{subdir}/huge.jpg");

        // Write a file that is exactly 1 byte over the limit.
        let f = std::fs::File::create(&rel_path).unwrap();
        f.set_len(MAX_IMAGE_FILE_SIZE + 1).unwrap();
        drop(f);

        let input = format!("{{image: src={rel_path}}}");
        let song = chordsketch_core::parse(&input).unwrap();
        // Should not panic or crash — the oversized image is silently skipped.
        let pdf = render_song(&song);
        let content = String::from_utf8_lossy(&pdf);
        // The PDF must not contain an image XObject.
        assert!(
            !content.contains("/Subtype /Image"),
            "oversized image must be rejected"
        );

        let _ = std::fs::remove_dir_all(subdir);
    }

    #[test]
    fn test_negative_scale_falls_back_to_native() {
        let attrs = ImageAttributes {
            scale: Some("-1".to_string()),
            ..Default::default()
        };
        let (w, h) = compute_image_dimensions(&attrs, 400.0, 300.0, 400.0 / 300.0);
        // Negative scale is rejected; native dimensions are used.
        assert!((w - 400.0).abs() < 0.01);
        assert!((h - 300.0).abs() < 0.01);
    }

    #[test]
    fn test_negative_width_falls_back_to_native() {
        let attrs = ImageAttributes {
            width: Some("-200".to_string()),
            ..Default::default()
        };
        let (w, h) = compute_image_dimensions(&attrs, 400.0, 300.0, 400.0 / 300.0);
        assert!((w - 400.0).abs() < 0.01);
        assert!((h - 300.0).abs() < 0.01);
    }

    #[test]
    fn test_negative_height_falls_back_to_native() {
        let attrs = ImageAttributes {
            height: Some("-150".to_string()),
            ..Default::default()
        };
        let (w, h) = compute_image_dimensions(&attrs, 400.0, 300.0, 400.0 / 300.0);
        assert!((w - 400.0).abs() < 0.01);
        assert!((h - 300.0).abs() < 0.01);
    }

    #[test]
    fn test_zero_scale_falls_back_to_native() {
        let attrs = ImageAttributes {
            scale: Some("0".to_string()),
            ..Default::default()
        };
        let (w, h) = compute_image_dimensions(&attrs, 400.0, 300.0, 400.0 / 300.0);
        assert!((w - 400.0).abs() < 0.01);
        assert!((h - 300.0).abs() < 0.01);
    }

    #[test]
    fn test_clamp_to_printable_no_clamping_needed() {
        let (w, h) = clamp_to_printable_area(200.0, 150.0, 500.0, 700.0, 200.0 / 150.0);
        assert!((w - 200.0).abs() < 0.01);
        assert!((h - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_clamp_to_printable_width_exceeds() {
        // 800x200 image, max 500x700 area, aspect 4.0
        let (w, h) = clamp_to_printable_area(800.0, 200.0, 500.0, 700.0, 4.0);
        assert!((w - 500.0).abs() < 0.01);
        assert!((h - 125.0).abs() < 0.01); // 500 / 4.0
    }

    #[test]
    fn test_clamp_to_printable_height_exceeds() {
        // 200x800 image, max 500x700 area, aspect 0.25
        let (w, h) = clamp_to_printable_area(200.0, 800.0, 500.0, 700.0, 0.25);
        assert!((w - 175.0).abs() < 0.01); // 700 * 0.25
        assert!((h - 700.0).abs() < 0.01); // 175 / 0.25
    }

    #[test]
    fn test_clamp_to_printable_height_exceeds_extreme_aspect_reclamps_width() {
        // Extreme wide image: aspect 4.0, height clamped to 700 would produce
        // width 2800 which exceeds max_w 500. Width must be re-clamped.
        let (w, h) = clamp_to_printable_area(2800.0, 700.0, 500.0, 700.0, 4.0);
        // Width-clamping branch fires first (2800 > 500)
        assert!((w - 500.0).abs() < 0.01);
        assert!((h - 125.0).abs() < 0.01); // 500 / 4.0
    }

    #[test]
    fn test_clamp_to_printable_height_clamp_triggers_width_reclamp() {
        // Image that only exceeds height: 400x800 in 500x700 area, aspect 4.0
        // height 800 > max_h 700, so height-clamping branch runs.
        // max_h * aspect = 700 * 4.0 = 2800, which exceeds max_w 500.
        // Width must be re-clamped to 500, height adjusted to 500 / 4.0 = 125.
        let (w, h) = clamp_to_printable_area(400.0, 800.0, 500.0, 700.0, 4.0);
        assert!(w <= 500.0, "width {} must not exceed max_w 500", w);
        assert!((w - 500.0).abs() < 0.01);
        assert!((h - 125.0).abs() < 0.01); // 500 / 4.0
    }

    #[test]
    fn test_clamp_to_printable_width_exceeds_then_height_reclamps() {
        // Square image (aspect=1.0) in a very short printable area.
        // Width-clamping branch fires first (2000 > 500), computing
        // clamped_h = 500/1.0 = 500, but max_h is only 50.
        // Height must be clamped to 50, then width re-adjusted to 50.
        let (w, h) = clamp_to_printable_area(2000.0, 2000.0, 500.0, 50.0, 1.0);
        assert!((w - 50.0).abs() < 0.01, "width {} should be 50.0", w);
        assert!((h - 50.0).abs() < 0.01, "height {} should be 50.0", h);
    }

    #[test]
    fn test_safe_image_path_relative() {
        assert!(is_safe_image_path("photo.jpg"));
        assert!(is_safe_image_path("images/photo.jpg"));
        assert!(is_safe_image_path("sub/dir/photo.jpg"));
    }

    #[test]
    fn test_safe_image_path_rejects_empty() {
        assert!(!is_safe_image_path(""));
    }

    #[test]
    fn test_safe_image_path_rejects_null_bytes() {
        assert!(!is_safe_image_path("photo\0.jpg"));
        assert!(!is_safe_image_path("images/photo.jpg\0../../etc/shadow"));
    }

    #[test]
    fn test_safe_image_path_rejects_absolute() {
        assert!(!is_safe_image_path("/etc/shadow.jpeg"));
        assert!(!is_safe_image_path("/home/user/photo.jpg"));
    }

    #[test]
    fn test_safe_image_path_rejects_traversal() {
        assert!(!is_safe_image_path("../photo.jpg"));
        assert!(!is_safe_image_path("images/../../etc/shadow.jpeg"));
        assert!(!is_safe_image_path("sub/../../../photo.jpg"));
    }

    #[test]
    fn test_safe_image_path_windows_style_strings() {
        // Platform-agnostic string checks for Windows-style paths.
        // On Unix, `Path::is_absolute()` doesn't flag `C:\` as absolute,
        // but `C:\` starts with a prefix that std::path::Component::Prefix
        // detects on Windows. We test the string patterns here to ensure
        // coverage regardless of platform.

        // Backslash-separated relative paths: allowed (treated as a single
        // filename component on Unix, or valid relative on Windows).
        assert!(is_safe_image_path(r"images\photo.jpg"));

        // Unix-style absolute path with leading slash: always rejected.
        assert!(!is_safe_image_path("/images/photo.jpg"));
    }

    #[test]
    fn test_safe_image_path_windows_absolute_rejected() {
        // String-level checks reject Windows-style absolute paths on all platforms.
        assert!(!is_safe_image_path(r"C:\photo.jpg"));
        assert!(!is_safe_image_path(r"D:\Users\photo.jpg"));
        assert!(!is_safe_image_path(r"\\server\share\photo.jpg"));
        assert!(!is_safe_image_path("C:/photo.jpg"));
    }

    #[test]
    fn test_safe_image_path_backslash_traversal_rejected() {
        // The shared `has_traversal` helper splits on both `/` and `\`,
        // so backslash-based traversal is now detected on all platforms.
        assert!(!is_safe_image_path(r"..\photo.jpg"));
        assert!(!is_safe_image_path(r"images\..\..\photo.jpg"));
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_image_is_rejected() {
        use std::os::unix::fs::symlink;

        // Use a unique subdirectory name (PID + thread name) so parallel test
        // threads never collide on the same directory.  The path must be
        // relative because `is_safe_image_path()` rejects absolute paths.
        let subdir = format!(
            "_test_symlink_img_{}_{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("main")
        );
        let _ = std::fs::remove_dir_all(&subdir);
        std::fs::create_dir_all(&subdir).expect("create test dir");

        let target = format!("{subdir}/real.jpg");
        std::fs::write(&target, b"\xFF\xD8\xFF").expect("write target");
        let link = format!("{subdir}/link.jpg");
        symlink(&target, &link).expect("create symlink");

        let input = format!("{{title: T}}\n{{image: src={link}}}");
        let song = chordsketch_core::parse(&input).expect("parse");
        let pdf = render_song(&song);
        let content = String::from_utf8_lossy(&pdf);
        // Image must NOT be embedded because src is a symlink.
        assert!(
            !content.contains("/Subtype /Image"),
            "symlink images must be rejected"
        );

        let _ = std::fs::remove_dir_all(&subdir);
    }

    #[test]
    fn test_custom_margins_from_config() {
        let config = Config::defaults()
            .with_define("pdf.margins.top=100")
            .unwrap();
        let doc = PdfDocument::from_config(&config);
        assert!((doc.margin_top - 100.0).abs() < 0.01);
        // Other margins should keep defaults.
        assert!((doc.margin_bottom - MARGIN_BOTTOM).abs() < 0.01);
        assert!((doc.margin_left - MARGIN_LEFT).abs() < 0.01);
        assert!((doc.margin_right - MARGIN_RIGHT).abs() < 0.01);
    }

    #[test]
    fn test_negative_margin_falls_back_to_default() {
        let config = Config::defaults()
            .with_define("pdf.margins.top=-100")
            .unwrap();
        let doc = PdfDocument::from_config(&config);
        assert!((doc.margin_top - MARGIN_TOP).abs() < 0.01);
    }

    #[test]
    fn test_zero_margin_is_valid() {
        let config = Config::defaults().with_define("pdf.margins.top=0").unwrap();
        let doc = PdfDocument::from_config(&config);
        assert!(doc.margin_top.abs() < 0.01);
    }

    #[test]
    fn test_excessive_margin_falls_back_to_default() {
        let config = Config::defaults()
            .with_define("pdf.margins.left=1000")
            .unwrap();
        let doc = PdfDocument::from_config(&config);
        assert!((doc.margin_left - MARGIN_LEFT).abs() < 0.01);
    }

    #[test]
    fn test_custom_margins_affect_output() {
        let song = chordsketch_core::parse("{title: Test}\nHello").unwrap();
        let default_pdf = render_song(&song);
        let config = Config::defaults()
            .with_define("pdf.margins.top=200")
            .unwrap();
        let custom_pdf = render_song_with_transpose(&song, 0, &config);
        // Different margins produce different PDF output.
        assert_ne!(default_pdf, custom_pdf);
    }

    #[test]
    fn test_fmt_f32_nan_produces_zero() {
        assert_eq!(fmt_f32(f32::NAN), "0");
    }

    #[test]
    fn test_fmt_f32_infinity_produces_zero() {
        assert_eq!(fmt_f32(f32::INFINITY), "0");
        assert_eq!(fmt_f32(f32::NEG_INFINITY), "0");
    }

    #[test]
    fn test_fmt_f32_normal_values() {
        assert_eq!(fmt_f32(1.0), "1");
        assert_eq!(fmt_f32(3.25), "3.25");
        assert_eq!(fmt_f32(0.0), "0");
        assert_eq!(fmt_f32(-5.5), "-5.5");
    }
}

#[cfg(test)]
mod png_tests {
    use super::*;

    /// Build a minimal valid PNG file with the given pixel data.
    ///
    /// `color_type`: 0=gray, 2=RGB, 4=gray+alpha, 6=RGBA
    /// `pixels`: raw pixel data (row-major, no filter bytes — filter 0 is added).
    fn build_png(width: u32, height: u32, bit_depth: u8, color_type: u8, pixels: &[u8]) -> Vec<u8> {
        let channels: usize = match color_type {
            0 => 1,
            2 => 3,
            4 => 2,
            6 => 4,
            _ => panic!("unsupported color type"),
        };
        let bytes_per_sample = if bit_depth == 16 { 2 } else { 1 };
        let row_bytes = width as usize * channels * bytes_per_sample;

        // Build raw data with filter byte 0 (None) per row.
        let mut raw = Vec::new();
        for row in 0..height as usize {
            raw.push(0); // filter None
            let start = row * row_bytes;
            raw.extend_from_slice(&pixels[start..start + row_bytes]);
        }

        // Compress with zlib.
        let idat_payload = zlib_compress(&raw).expect("compression should succeed");

        let mut png = Vec::new();
        png.extend_from_slice(&PNG_SIGNATURE);

        // IHDR
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.push(bit_depth);
        ihdr.push(color_type);
        ihdr.push(0); // compression
        ihdr.push(0); // filter
        ihdr.push(0); // interlace
        write_png_chunk(&mut png, b"IHDR", &ihdr);

        // IDAT
        write_png_chunk(&mut png, b"IDAT", &idat_payload);

        // IEND
        write_png_chunk(&mut png, b"IEND", &[]);

        png
    }

    /// Build a minimal indexed PNG (color type 3) with a PLTE chunk.
    fn build_indexed_png(width: u32, height: u32, palette: &[u8], indices: &[u8]) -> Vec<u8> {
        let row_bytes = width as usize;

        let mut raw = Vec::new();
        for row in 0..height as usize {
            raw.push(0); // filter None
            let start = row * row_bytes;
            raw.extend_from_slice(&indices[start..start + row_bytes]);
        }

        let idat_payload = zlib_compress(&raw).expect("compression should succeed");

        let mut png = Vec::new();
        png.extend_from_slice(&PNG_SIGNATURE);

        // IHDR
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.push(8); // bit depth
        ihdr.push(3); // color type indexed
        ihdr.push(0);
        ihdr.push(0);
        ihdr.push(0);
        write_png_chunk(&mut png, b"IHDR", &ihdr);

        // PLTE
        write_png_chunk(&mut png, b"PLTE", palette);

        // IDAT
        write_png_chunk(&mut png, b"IDAT", &idat_payload);

        // IEND
        write_png_chunk(&mut png, b"IEND", &[]);

        png
    }

    fn write_png_chunk(out: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(chunk_type);
        out.extend_from_slice(data);
        // CRC32 over type + data
        let mut crc_data = Vec::new();
        crc_data.extend_from_slice(chunk_type);
        crc_data.extend_from_slice(data);
        let crc = crc32(&crc_data);
        out.extend_from_slice(&crc.to_be_bytes());
    }

    /// Simple CRC-32 (PNG uses CRC-32/ISO-3309).
    fn crc32(data: &[u8]) -> u32 {
        let mut crc: u32 = 0xFFFF_FFFF;
        for &byte in data {
            crc ^= byte as u32;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0xEDB8_8320;
                } else {
                    crc >>= 1;
                }
            }
        }
        !crc
    }

    #[test]
    fn test_parse_png_rgb() {
        // 2x2 RGB image (color type 2)
        let pixels = vec![
            255, 0, 0, 0, 255, 0, // row 0: red, green
            0, 0, 255, 255, 255, 255, // row 1: blue, white
        ];
        let png = build_png(2, 2, 8, 2, &pixels);
        let info = parse_png(&png).expect("should parse");
        assert_eq!(info.width, 2);
        assert_eq!(info.height, 2);
        assert_eq!(info.bit_depth, 8);
        assert_eq!(info.colors, 3);
        assert!(info.palette.is_none());
        assert!(info.smask.is_none());
    }

    #[test]
    fn test_parse_png_grayscale() {
        // 3x1 grayscale image (color type 0)
        let pixels = vec![0, 128, 255];
        let png = build_png(3, 1, 8, 0, &pixels);
        let info = parse_png(&png).expect("should parse");
        assert_eq!(info.width, 3);
        assert_eq!(info.height, 1);
        assert_eq!(info.colors, 1);
        assert!(info.smask.is_none());
    }

    #[test]
    fn test_parse_png_rgba_separates_alpha() {
        // 2x1 RGBA image (color type 6)
        let pixels = vec![
            255, 0, 0, 128, // red, alpha 128
            0, 255, 0, 255, // green, alpha 255
        ];
        let png = build_png(2, 1, 8, 6, &pixels);
        let info = parse_png(&png).expect("should parse RGBA");
        assert_eq!(info.width, 2);
        assert_eq!(info.height, 1);
        assert_eq!(info.colors, 3); // RGB after alpha removal
        assert!(info.smask.is_some());

        // Verify the color data by decompressing the IDAT.
        let mut decoder = ZlibDecoder::new(info.idat_data.as_slice());
        let mut color = Vec::new();
        decoder.read_to_end(&mut color).unwrap();
        // Row: filter(0) + R G B R G B
        assert_eq!(color, vec![0, 255, 0, 0, 0, 255, 0]);

        // Verify alpha data.
        let mut decoder = ZlibDecoder::new(info.smask.as_ref().unwrap().as_slice());
        let mut alpha = Vec::new();
        decoder.read_to_end(&mut alpha).unwrap();
        // Row: filter(0) + alpha alpha
        assert_eq!(alpha, vec![0, 128, 255]);
    }

    #[test]
    fn test_parse_png_gray_alpha() {
        // 2x1 gray+alpha image (color type 4)
        let pixels = vec![
            100, 200, // gray 100, alpha 200
            50, 100, // gray 50, alpha 100
        ];
        let png = build_png(2, 1, 8, 4, &pixels);
        let info = parse_png(&png).expect("should parse gray+alpha");
        assert_eq!(info.colors, 1); // grayscale after alpha removal
        assert!(info.smask.is_some());

        let mut decoder = ZlibDecoder::new(info.idat_data.as_slice());
        let mut color = Vec::new();
        decoder.read_to_end(&mut color).unwrap();
        assert_eq!(color, vec![0, 100, 50]);

        let mut decoder = ZlibDecoder::new(info.smask.as_ref().unwrap().as_slice());
        let mut alpha = Vec::new();
        decoder.read_to_end(&mut alpha).unwrap();
        assert_eq!(alpha, vec![0, 200, 100]);
    }

    #[test]
    fn test_parse_png_indexed() {
        // 2x1 indexed image with 2-color palette
        let palette = vec![255, 0, 0, 0, 0, 255]; // red, blue
        let indices = vec![0, 1]; // pixel 0 = red, pixel 1 = blue
        let png = build_indexed_png(2, 1, &palette, &indices);
        let info = parse_png(&png).expect("should parse indexed");
        assert_eq!(info.colors, 3); // presented as RGB via palette
        assert!(info.palette.is_some());
        assert_eq!(info.palette.as_ref().unwrap(), &palette);
    }

    #[test]
    fn test_parse_png_invalid_signature() {
        assert!(parse_png(b"not a png").is_none());
        assert!(parse_png(&[]).is_none());
    }

    #[test]
    fn test_parse_png_no_idat() {
        let mut png = Vec::new();
        png.extend_from_slice(&PNG_SIGNATURE);
        // IHDR only, no IDAT
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.extend_from_slice(&2u32.to_be_bytes());
        ihdr.push(8);
        ihdr.push(2); // RGB
        ihdr.extend_from_slice(&[0, 0, 0]);
        write_png_chunk(&mut png, b"IHDR", &ihdr);
        write_png_chunk(&mut png, b"IEND", &[]);
        assert!(parse_png(&png).is_none());
    }

    #[test]
    fn test_embedded_image_num_objects_jpeg() {
        let img = EmbeddedImage {
            width: 10,
            height: 10,
            format: ImageFormat::Jpeg {
                data: vec![],
                components: 3,
            },
        };
        assert_eq!(img.num_pdf_objects(), 1);
    }

    #[test]
    fn test_embedded_image_num_objects_png_no_alpha() {
        let img = EmbeddedImage {
            width: 10,
            height: 10,
            format: ImageFormat::Png {
                idat_data: vec![],
                bit_depth: 8,
                colors: 3,
                palette: None,
                smask: None,
            },
        };
        assert_eq!(img.num_pdf_objects(), 1);
    }

    #[test]
    fn test_embedded_image_num_objects_png_with_alpha() {
        let img = EmbeddedImage {
            width: 10,
            height: 10,
            format: ImageFormat::Png {
                idat_data: vec![],
                bit_depth: 8,
                colors: 3,
                palette: None,
                smask: Some(vec![1, 2, 3]),
            },
        };
        assert_eq!(img.num_pdf_objects(), 2);
    }

    #[test]
    fn test_paeth_predictor() {
        // Standard cases from the PNG specification.
        // Paeth(a, b, c): p = a + b - c
        assert_eq!(paeth_predictor(0, 0, 0), 0);
        // a=10, b=20, c=15: p=15, pa=5, pb=5, pc=0 → c (pc smallest)
        assert_eq!(paeth_predictor(10, 20, 15), 15);
        // a=10, b=10, c=10: p=10, pa=0, pb=0, pc=0 → a (pa<=pb and pa<=pc)
        assert_eq!(paeth_predictor(10, 10, 10), 10);
    }

    #[test]
    fn test_render_songs_with_warnings_empty_slice() {
        let songs: Vec<chordsketch_core::ast::Song> = Vec::new();
        let result = render_songs_with_warnings(&songs, 0, &Config::defaults());
        // Should not panic and should return empty output.
        assert!(result.output.is_empty());
    }
}
