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
//! sections contain verbatim markup (e.g., inline SVG).
//!
//! SVG sections are sanitized by default: `<script>` elements and event
//! handler attributes (`onload`, `onerror`, etc.) are stripped to prevent
//! XSS. When rendering untrusted ChordPro input, consumers should still
//! apply Content Security Policy (CSP) headers as additional defense.

use std::fmt::Write;

use chordsketch_core::ast::{CommentStyle, DirectiveKind, Line, LyricsLine, Song};
use chordsketch_core::canonical_chord_name;
use chordsketch_core::config::Config;
use chordsketch_core::escape::escape_xml as escape;
use chordsketch_core::inline_markup::{SpanAttributes, TextSpan};
use chordsketch_core::render_result::{RenderResult, push_warning, validate_capo};
use chordsketch_core::resolve_diagrams_instrument;
use chordsketch_core::transpose::transpose_chord;

/// Maximum number of chorus recall directives allowed per song.
/// Prevents output amplification from malicious inputs with many `{chorus}` lines.
const MAX_CHORUS_RECALLS: usize = 1000;

/// Maximum number of warnings the renderer accumulates per render pass.
/// Re-exported from `chordsketch-core::render_result` so callers can
/// keep importing `chordsketch_render_html::MAX_WARNINGS` unchanged
/// (issue #1874).
pub use chordsketch_core::render_result::MAX_WARNINGS;

/// Maximum number of CSS columns allowed.
/// Matches `MAX_COLUMNS` in the PDF renderer.
const MAX_COLUMNS: u32 = 32;

/// Minimum font size (in points) accepted from user directives.
/// Matches `MIN_FONT_SIZE` in the PDF renderer.
const MIN_FONT_SIZE: f32 = 0.5;
/// Maximum font size (in points) accepted from user directives.
/// Matches `MAX_FONT_SIZE` in the PDF renderer.
const MAX_FONT_SIZE: f32 = 200.0;

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
            let _ = write!(css, "font-family: {};", sanitize_css_value(font));
        }
        if let Some(ref size) = self.size {
            let safe = sanitize_css_value(size);
            if safe.chars().all(|c| c.is_ascii_digit()) {
                let _ = write!(css, "font-size: {safe}pt;");
            } else {
                let _ = write!(css, "font-size: {safe};");
            }
        }
        if let Some(ref colour) = self.colour {
            let _ = write!(css, "color: {};", sanitize_css_value(colour));
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
    ///
    /// Font size values are clamped to `[MIN_FONT_SIZE, MAX_FONT_SIZE]` to
    /// prevent degenerate CSS output from extreme values. This matches the
    /// clamping applied in the PDF renderer per `renderer-parity.md`.
    fn apply(&mut self, kind: &DirectiveKind, value: &Option<String>) {
        let val = value.clone();
        let clamped_size = || -> Option<String> {
            value
                .as_deref()
                .and_then(|v| v.parse::<f32>().ok())
                .map(|s| s.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE).to_string())
        };
        match kind {
            DirectiveKind::TextFont => self.text.font = val,
            DirectiveKind::TextSize => self.text.size = clamped_size(),
            DirectiveKind::TextColour => self.text.colour = val,
            DirectiveKind::ChordFont => self.chord.font = val,
            DirectiveKind::ChordSize => self.chord.size = clamped_size(),
            DirectiveKind::ChordColour => self.chord.colour = val,
            DirectiveKind::TabFont => self.tab.font = val,
            DirectiveKind::TabSize => self.tab.size = clamped_size(),
            DirectiveKind::TabColour => self.tab.colour = val,
            DirectiveKind::TitleFont => self.title.font = val,
            DirectiveKind::TitleSize => self.title.size = clamped_size(),
            DirectiveKind::TitleColour => self.title.colour = val,
            DirectiveKind::ChorusFont => self.chorus.font = val,
            DirectiveKind::ChorusSize => self.chorus.size = clamped_size(),
            DirectiveKind::ChorusColour => self.chorus.colour = val,
            DirectiveKind::LabelFont => self.label.font = val,
            DirectiveKind::LabelSize => self.label.size = clamped_size(),
            DirectiveKind::LabelColour => self.label.colour = val,
            DirectiveKind::GridFont => self.grid.font = val,
            DirectiveKind::GridSize => self.grid.size = clamped_size(),
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

/// Render a [`Song`] AST to an HTML5 document, returning warnings programmatically.
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
    let title = song.metadata.title.as_deref().unwrap_or("Untitled");
    let mut html = String::new();
    let _ = write!(
        html,
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n",
        escape(title)
    );
    html.push_str("<style>\n");
    html.push_str(CSS);
    html.push_str("</style>\n</head>\n<body>\n");
    render_song_body(song, cli_transpose, config, &mut html, &mut warnings);
    html.push_str("</body>\n</html>\n");
    RenderResult::with_warnings(html, warnings)
}

/// Render the `<div class="song">...</div>` body for a single song into `html`.
///
/// This is the shared implementation used by both single-song and multi-song
/// rendering. It appends directly to the provided buffer without any document
/// wrapper (`<html>`, `<head>`, etc.).
fn render_song_body(
    song: &Song,
    cli_transpose: i8,
    config: &Config,
    html: &mut String,
    warnings: &mut Vec<String>,
) {
    // Apply song-level config overrides ({+config.KEY: VALUE} directives).
    let song_overrides = song.config_overrides();
    let song_config;
    let config = if song_overrides.is_empty() {
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
    let (combined_transpose, _) =
        chordsketch_core::transpose::combine_transpose(cli_transpose, song_transpose_delta);
    let mut transpose_offset: i8 = combined_transpose;
    let mut fmt_state = FormattingState::default();
    html.push_str("<div class=\"song\">\n");

    validate_capo(&song.metadata, warnings);
    render_metadata(&song.metadata, html);

    // Tracks whether a multi-column div is currently open.
    let mut columns_open = false;
    // Buffer for collecting SVG section content. Content is sanitized as a
    // single string on EndOfSvg to prevent multi-line tag splitting bypasses.
    let mut svg_buf: Option<String> = None;
    // Delegate tool availability: Some(true) = force enable, Some(false) = force
    // disable, None = auto-detect on first encounter. The auto-detect value is
    // lazily resolved (via `get_or_insert_with`) so that subprocess checks only
    // run when a delegate section is actually present in the input.
    let mut abc2svg_resolved: Option<bool> = config.get_path("delegates.abc2svg").as_bool();
    let mut lilypond_resolved: Option<bool> = config.get_path("delegates.lilypond").as_bool();
    let mut musescore_resolved: Option<bool> = config.get_path("delegates.musescore").as_bool();
    let mut abc_buf: Option<String> = None;
    let mut abc_label: Option<String> = None;
    let mut ly_buf: Option<String> = None;
    let mut ly_label: Option<String> = None;
    let mut musicxml_buf: Option<String> = None;
    let mut musicxml_label: Option<String> = None;

    // Controls whether chord diagrams are rendered. Set by {diagrams: off/on}.
    let mut show_diagrams = true;

    // Read configurable frets_shown for chord diagrams.
    let diagram_frets = config
        .get_path("diagrams.frets")
        .as_f64()
        .map_or(chordsketch_core::chord_diagram::DEFAULT_FRETS_SHOWN, |n| {
            (n as usize).max(1)
        });

    // Instrument for the auto-inject diagram block at end of song.
    // Set by {diagrams: guitar/ukulele/on}; cleared by {diagrams: off} / {no_diagrams}.
    // None means no auto-inject grid is rendered.
    let default_instrument = config
        .get_path("diagrams.instrument")
        .as_str()
        .map(str::to_ascii_lowercase)
        .unwrap_or_else(|| "guitar".to_string());
    let mut auto_diagrams_instrument: Option<String> = None;
    // Canonical chord names (sharp form) that were actually rendered inline via
    // {define} while show_diagrams was true.  Used to exclude them from the
    // auto-inject grid and avoid duplicates.
    let mut inline_defined: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Stores the AST lines of the most recently defined chorus body.
    // Re-rendered at recall time so the current transpose offset is applied.
    let mut chorus_body: Vec<Line> = Vec::new();
    // Temporary buffer for collecting chorus AST lines.
    let mut chorus_buf: Option<Vec<Line>> = None;
    // Saved fmt_state before entering a chorus, restored on EndOfChorus
    // to prevent in-chorus formatting directives from leaking outward.
    let mut saved_fmt_state: Option<FormattingState> = None;
    let mut chorus_recall_count: usize = 0;

    for line in &song.lines {
        match line {
            Line::Lyrics(lyrics_line) => {
                if let Some(ref mut buf) = svg_buf {
                    // Inside SVG section: collect content into buffer.
                    // Sanitization is deferred to EndOfSvg so that multi-line
                    // tags cannot bypass dangerous element detection.
                    let raw = lyrics_line.text();
                    buf.push_str(&raw);
                    buf.push('\n');
                } else if let Some(ref mut buf) = abc_buf {
                    // Inside ABC section with abc2svg enabled: collect content.
                    let raw = lyrics_line.text();
                    buf.push_str(&raw);
                    buf.push('\n');
                } else if let Some(ref mut buf) = ly_buf {
                    // Inside Lilypond section with lilypond enabled: collect content.
                    let raw = lyrics_line.text();
                    buf.push_str(&raw);
                    buf.push('\n');
                } else if let Some(ref mut buf) = musicxml_buf {
                    // Inside MusicXML section with musescore enabled: collect content.
                    let raw = lyrics_line.text();
                    buf.push_str(&raw);
                    buf.push('\n');
                } else {
                    if let Some(buf) = chorus_buf.as_mut() {
                        buf.push(line.clone());
                    }
                    render_lyrics(lyrics_line, transpose_offset, &fmt_state, html);
                }
            }
            Line::Directive(directive) => {
                if directive.kind.is_metadata() {
                    continue;
                }
                if directive.kind == DirectiveKind::Diagrams {
                    auto_diagrams_instrument = resolve_diagrams_instrument(
                        directive.value.as_deref(),
                        &default_instrument,
                    );
                    show_diagrams = auto_diagrams_instrument.is_some();
                    continue;
                }
                if directive.kind == DirectiveKind::NoDiagrams {
                    show_diagrams = false;
                    auto_diagrams_instrument = None;
                    continue;
                }
                if directive.kind == DirectiveKind::Transpose {
                    // A missing or empty value silently resets to 0; only a
                    // non-empty value that cannot be parsed as i8 emits a warning.
                    let file_offset: i8 = match directive.value.as_deref() {
                        None | Some("") => 0,
                        Some(raw) => match raw.parse() {
                            Ok(v) => v,
                            Err(_) => {
                                push_warning(
                                    warnings,
                                    format!(
                                        "{{transpose}} value {raw:?} cannot be \
                                         parsed as i8, ignored (using 0)"
                                    ),
                                );
                                0
                            }
                        },
                    };
                    let (combined, saturated) =
                        chordsketch_core::transpose::combine_transpose(file_offset, cli_transpose);
                    if saturated {
                        push_warning(
                            warnings,
                            format!(
                                "transpose offset {file_offset} + {cli_transpose} \
                                 exceeds i8 range, clamped to {combined}"
                            ),
                        );
                    }
                    transpose_offset = combined;
                    continue;
                }
                if directive.kind.is_font_size_color() {
                    if let Some(buf) = chorus_buf.as_mut() {
                        buf.push(line.clone());
                    }
                    fmt_state.apply(&directive.kind, &directive.value);
                    continue;
                }
                match &directive.kind {
                    DirectiveKind::StartOfChorus => {
                        render_section_open("chorus", "Chorus", &directive.value, html);
                        chorus_buf = Some(Vec::new());
                        // Save fmt_state so in-chorus formatting directives
                        // do not leak into sections after the chorus.
                        saved_fmt_state = Some(fmt_state.clone());
                    }
                    DirectiveKind::EndOfChorus => {
                        html.push_str("</section>\n");
                        if let Some(buf) = chorus_buf.take() {
                            chorus_body = buf;
                        }
                        // Restore fmt_state to pre-chorus value.
                        if let Some(saved) = saved_fmt_state.take() {
                            fmt_state = saved;
                        }
                    }
                    DirectiveKind::Chorus => {
                        if chorus_recall_count < MAX_CHORUS_RECALLS {
                            render_chorus_recall(
                                &directive.value,
                                &chorus_body,
                                transpose_offset,
                                &fmt_state,
                                show_diagrams,
                                diagram_frets,
                                html,
                            );
                            chorus_recall_count += 1;
                        } else if chorus_recall_count == MAX_CHORUS_RECALLS {
                            push_warning(
                                warnings,
                                format!(
                                    "chorus recall limit ({MAX_CHORUS_RECALLS}) reached, \
                                     further recalls suppressed"
                                ),
                            );
                            chorus_recall_count += 1;
                        }
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
                            let _ = writeln!(
                                html,
                                "<div style=\"column-count: {n};column-gap: 2em;\">"
                            );
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
                    DirectiveKind::NewPage => {
                        html.push_str("<div style=\"break-before: page;\"></div>\n");
                    }
                    DirectiveKind::NewPhysicalPage => {
                        // Use CSS `break-before: recto` so the browser inserts
                        // a blank page when needed to start on a right-hand page
                        // in duplex printing.
                        html.push_str("<div style=\"break-before: recto;\"></div>\n");
                    }
                    DirectiveKind::StartOfAbc => {
                        #[cfg(not(target_arch = "wasm32"))]
                        let enabled = *abc2svg_resolved
                            .get_or_insert_with(chordsketch_core::external_tool::has_abc2svg);
                        #[cfg(target_arch = "wasm32")]
                        let enabled = *abc2svg_resolved.get_or_insert(false);
                        if enabled {
                            abc_buf = Some(String::new());
                            abc_label = directive.value.clone();
                        } else {
                            if let Some(buf) = chorus_buf.as_mut() {
                                buf.push(line.clone());
                            }
                            render_directive_inner(directive, show_diagrams, diagram_frets, html);
                        }
                    }
                    DirectiveKind::EndOfAbc if abc_buf.is_some() => {
                        if let Some(abc_content) = abc_buf.take() {
                            render_abc_with_fallback(&abc_content, &abc_label, html, warnings);
                            abc_label = None;
                        }
                    }
                    DirectiveKind::StartOfLy => {
                        #[cfg(not(target_arch = "wasm32"))]
                        let enabled = *lilypond_resolved
                            .get_or_insert_with(chordsketch_core::external_tool::has_lilypond);
                        #[cfg(target_arch = "wasm32")]
                        let enabled = *lilypond_resolved.get_or_insert(false);
                        if enabled {
                            ly_buf = Some(String::new());
                            ly_label = directive.value.clone();
                        } else {
                            if let Some(buf) = chorus_buf.as_mut() {
                                buf.push(line.clone());
                            }
                            render_directive_inner(directive, show_diagrams, diagram_frets, html);
                        }
                    }
                    DirectiveKind::EndOfLy if ly_buf.is_some() => {
                        if let Some(ly_content) = ly_buf.take() {
                            render_ly_with_fallback(&ly_content, &ly_label, html, warnings);
                            ly_label = None;
                        }
                    }
                    DirectiveKind::StartOfMusicxml => {
                        #[cfg(not(target_arch = "wasm32"))]
                        let enabled = *musescore_resolved
                            .get_or_insert_with(chordsketch_core::external_tool::has_musescore);
                        #[cfg(target_arch = "wasm32")]
                        let enabled = *musescore_resolved.get_or_insert(false);
                        if enabled {
                            musicxml_buf = Some(String::new());
                            musicxml_label = directive.value.clone();
                        } else {
                            if let Some(buf) = chorus_buf.as_mut() {
                                buf.push(line.clone());
                            }
                            render_directive_inner(directive, show_diagrams, diagram_frets, html);
                        }
                    }
                    DirectiveKind::EndOfMusicxml if musicxml_buf.is_some() => {
                        if let Some(musicxml_content) = musicxml_buf.take() {
                            render_musicxml_with_fallback(
                                &musicxml_content,
                                &musicxml_label,
                                html,
                                warnings,
                            );
                            musicxml_label = None;
                        }
                    }
                    DirectiveKind::StartOfSvg => {
                        svg_buf = Some(String::new());
                    }
                    DirectiveKind::EndOfSvg if svg_buf.is_some() => {
                        if let Some(svg_content) = svg_buf.take() {
                            html.push_str("<div class=\"svg-section\">\n");
                            html.push_str(&sanitize_svg_content(&svg_content));
                            html.push('\n');
                            html.push_str("</div>\n");
                        }
                    }
                    _ => {
                        if let Some(buf) = chorus_buf.as_mut() {
                            buf.push(line.clone());
                        }
                        // Track {define} chords that are rendered inline so the
                        // auto-inject grid can skip them (dedup for #1211/#1245/#1246).
                        if directive.kind == DirectiveKind::Define && show_diagrams {
                            if let Some(ref val) = directive.value {
                                let name =
                                    chordsketch_core::ast::ChordDefinition::parse_value(val).name;
                                if !name.is_empty() {
                                    inline_defined.insert(canonical_chord_name(&name));
                                }
                            }
                        }
                        render_directive_inner(directive, show_diagrams, diagram_frets, html);
                    }
                }
            }
            Line::Comment(style, text) => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                render_comment(*style, text, html);
            }
            Line::Empty => {
                if let Some(buf) = chorus_buf.as_mut() {
                    buf.push(line.clone());
                }
                html.push_str("<div class=\"empty-line\"></div>\n");
            }
        }
    }

    // Close any open multi-column div.
    if columns_open {
        html.push_str("</div>\n");
    }

    // Auto-inject diagram grid when {diagrams} (or {diagrams: guitar/ukulele/piano/on}) was seen.
    if let Some(ref instrument) = auto_diagrams_instrument {
        // Skip chords that were actually rendered inline via {define} (i.e., show_diagrams
        // was true at the time).  Compare in canonical sharp form to catch enharmonic
        // pairs like {define: Bb …} vs [A#] in lyrics.
        let chord_names: Vec<String> = song
            .used_chord_names()
            .into_iter()
            .filter(|name| !inline_defined.contains(&canonical_chord_name(name)))
            .collect();

        if instrument == "piano" {
            // Keyboard instrument: use the piano voicing database.
            let kbd_defines = song.keyboard_defines();
            let voicings: Vec<_> = chord_names
                .into_iter()
                .filter_map(|name| chordsketch_core::lookup_keyboard_voicing(&name, &kbd_defines))
                .collect();
            if !voicings.is_empty() {
                html.push_str("<section class=\"chord-diagrams\">\n");
                html.push_str("<div class=\"section-label\">Chord Diagrams</div>\n");
                html.push_str("<div class=\"chord-diagrams-grid\">\n");
                for voicing in &voicings {
                    html.push_str("<div class=\"chord-diagram-container\">");
                    html.push_str(&chordsketch_core::chord_diagram::render_keyboard_svg(
                        voicing,
                    ));
                    html.push_str("</div>\n");
                }
                html.push_str("</div>\n");
                html.push_str("</section>\n");
            }
        } else {
            // Fretted instruments (guitar, ukulele, etc.).
            let defines = song.fretted_defines();
            let diagrams: Vec<_> = chord_names
                .into_iter()
                .filter_map(|name| {
                    chordsketch_core::lookup_diagram(&name, &defines, instrument, diagram_frets)
                })
                .collect();
            if !diagrams.is_empty() {
                html.push_str("<section class=\"chord-diagrams\">\n");
                html.push_str("<div class=\"section-label\">Chord Diagrams</div>\n");
                html.push_str("<div class=\"chord-diagrams-grid\">\n");
                for diagram in &diagrams {
                    html.push_str("<div class=\"chord-diagram-container\">");
                    html.push_str(&chordsketch_core::chord_diagram::render_svg(diagram));
                    html.push_str("</div>\n");
                }
                html.push_str("</div>\n");
                html.push_str("</section>\n");
            }
        }
    }

    html.push_str("</div>\n");
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

/// Render multiple [`Song`]s into a single HTML5 document, returning warnings
/// programmatically.
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
    if songs.len() <= 1 {
        let output = songs
            .first()
            .map(|s| {
                let r = render_song_with_warnings(s, cli_transpose, config);
                warnings = r.warnings;
                r.output
            })
            .unwrap_or_default();
        return RenderResult::with_warnings(output, warnings);
    }
    // Use the first song's title for the document
    let mut html = String::new();
    let title = songs
        .first()
        .and_then(|s| s.metadata.title.as_deref())
        .unwrap_or("Untitled");
    let _ = write!(
        html,
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>{}</title>\n",
        escape(title)
    );
    html.push_str("<style>\n");
    html.push_str(CSS);
    html.push_str("</style>\n</head>\n<body>\n");

    for (i, song) in songs.iter().enumerate() {
        if i > 0 {
            html.push_str("<hr class=\"song-separator\">\n");
        }
        render_song_body(song, cli_transpose, config, &mut html, &mut warnings);
    }

    html.push_str("</body>\n</html>\n");
    RenderResult::with_warnings(html, warnings)
}

/// Parse a ChordPro source string and render it to HTML.
///
/// Returns `Ok(html)` on success, or the [`chordsketch_core::ParseError`] if
/// the input cannot be parsed.
#[must_use = "parse errors should be handled"]
pub fn try_render(input: &str) -> Result<String, chordsketch_core::ParseError> {
    let song = chordsketch_core::parse(input)?;
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
img { max-width: 100%; height: auto; }
.chord-diagrams-grid { display: flex; flex-wrap: wrap; gap: 0.5em; margin: 0.5em 0; }
.chord-diagram-container { display: inline-block; vertical-align: top; }
.chord-diagram { display: block; }
";

// ---------------------------------------------------------------------------
// Escape
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

/// Render song metadata (title, subtitle) as HTML header elements.
fn render_metadata(metadata: &chordsketch_core::ast::Metadata, html: &mut String) {
    if let Some(title) = &metadata.title {
        let _ = writeln!(html, "<h1>{}</h1>", escape(title));
    }
    for subtitle in &metadata.subtitles {
        let _ = writeln!(html, "<h2>{}</h2>", escape(subtitle));
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
                let _ = write!(
                    html,
                    "<span class=\"chord\">{}</span>",
                    escape(&display_name)
                );
            } else {
                let _ = write!(
                    html,
                    "<span class=\"chord\" style=\"{}\">{}</span>",
                    escape(&chord_css),
                    escape(&display_name)
                );
            }
        } else if lyrics_line.has_chords() {
            // Empty chord placeholder to maintain vertical alignment.
            html.push_str("<span class=\"chord\"></span>");
        }

        let text_css = fmt_state.text.to_css();
        if text_css.is_empty() {
            html.push_str("<span class=\"lyrics\">");
        } else {
            let _ = write!(
                html,
                "<span class=\"lyrics\" style=\"{}\">",
                escape(&text_css)
            );
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
                    let _ = write!(html, "<span style=\"{}\">", escape(&css));
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
        let _ = write!(css, "font-family: {};", sanitize_css_value(font_family));
    }
    if let Some(ref size) = attrs.size {
        let safe = sanitize_css_value(size);
        // If the size is a plain number, treat it as pt; otherwise pass through.
        if safe.chars().all(|c| c.is_ascii_digit()) {
            let _ = write!(css, "font-size: {safe}pt;");
        } else {
            let _ = write!(css, "font-size: {safe};");
        }
    }
    if let Some(ref fg) = attrs.foreground {
        let _ = write!(css, "color: {};", sanitize_css_value(fg));
    }
    if let Some(ref bg) = attrs.background {
        let _ = write!(css, "background-color: {};", sanitize_css_value(bg));
    }
    if let Some(ref weight) = attrs.weight {
        let _ = write!(css, "font-weight: {};", sanitize_css_value(weight));
    }
    if let Some(ref style) = attrs.style {
        let _ = write!(css, "font-style: {};", sanitize_css_value(style));
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

/// Sanitize SVG/HTML content by removing `<script>` elements and event handler
/// attributes (`onload`, `onerror`, `onclick`, etc.).
///
/// This provides defense-in-depth against XSS when rendering untrusted `.cho`
/// files. The ChordPro specification allows raw SVG passthrough, but script
/// injection is never legitimate in music notation.
fn sanitize_svg_content(input: &str) -> String {
    // Dangerous elements that are stripped entirely (opening tag through closing tag).
    // Per sanitizer-security.md §SVG tag blocklists, this MUST cover all SVG/HTML
    // elements that can load external resources: script, feImage, image, iframe,
    // embed, object, and foreign-content containers.
    // Note: <use> elements are NOT stripped here; their href/xlink:href attributes are
    // restricted by sanitize_tag_attrs to fragment-only references (^#...). External
    // URIs — including https — are stripped entirely to prevent tracking-pixel,
    // cross-origin-referer, and timing-based exfiltration attacks (see #1828).
    const DANGEROUS_TAGS: &[&str] = &[
        "script",
        "foreignobject",
        "iframe",
        "object",
        "embed",
        "math",
        // feImage is an SVG filter primitive that loads external content via href.
        // Stripping by URI scheme alone is insufficient: <feImage href="https://attacker.com/"/>
        // would survive since https: is allowed. The element must be stripped entirely.
        "feimage",
        // SVG <image> element loads external raster/vector images. Not needed in
        // music notation SVG; strip entirely to prevent tracking-pixel and
        // cross-origin leaks even over https:.
        "image",
        "set",
        "animate",
        "animatetransform",
        "animatemotion",
    ];

    let mut result = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();
    let bytes = input.as_bytes();

    while let Some((i, c)) = chars.next() {
        if c == '<' {
            let rest = &input[i..];
            // Use a safe UTF-8 boundary for the prefix check. All tag names
            // are ASCII, so 30 bytes is more than enough for matching.
            let limit = rest
                .char_indices()
                .map(|(idx, _)| idx)
                .find(|&idx| idx >= 30)
                .unwrap_or(rest.len());
            let rest_upper = &rest[..limit];

            // Check for opening dangerous tags: <tag or <tag> or <tag ...>
            let mut matched = false;
            for tag in DANGEROUS_TAGS {
                let prefix = format!("<{tag}");
                if starts_with_ignore_case(rest_upper, &prefix)
                    && rest.len() > prefix.len()
                    && bytes
                        .get(i + prefix.len())
                        .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>' || *b == b'/')
                {
                    // Check if this opening tag is self-closing (ends with />).
                    // Skips `>` inside quoted attribute values to handle
                    // cases like `<set to="a>b"/>`.
                    let is_self_closing = {
                        let tag_bytes = rest.as_bytes();
                        let mut in_quote: Option<u8> = None;
                        let mut gt_pos = None;
                        for (idx, &b) in tag_bytes.iter().enumerate() {
                            match in_quote {
                                Some(q) if b == q => in_quote = None,
                                Some(_) => {}
                                None if b == b'"' || b == b'\'' => in_quote = Some(b),
                                None if b == b'>' => {
                                    gt_pos = Some(idx);
                                    break;
                                }
                                _ => {}
                            }
                        }
                        gt_pos.is_some_and(|gt| gt > 0 && tag_bytes[gt - 1] == b'/')
                    };

                    if is_self_closing {
                        // Self-closing tag — skip past the closing >.
                        // Use quote-aware scanning to avoid stopping at >
                        // inside attribute values.
                        let mut skip_quote: Option<char> = None;
                        while let Some(&(_, ch)) = chars.peek() {
                            chars.next();
                            match skip_quote {
                                Some(q) if ch == q => skip_quote = None,
                                Some(_) => {}
                                None if ch == '"' || ch == '\'' => {
                                    skip_quote = Some(ch);
                                }
                                None if ch == '>' => break,
                                _ => {}
                            }
                        }
                    } else if let Some(end) = find_end_tag_ignore_case(input, i, tag) {
                        // Skip until after </tag>.
                        while let Some(&(j, _)) = chars.peek() {
                            if j >= end {
                                break;
                            }
                            chars.next();
                        }
                    } else {
                        // No closing tag — skip to end of input.
                        return result;
                    }
                    matched = true;
                    break;
                }
            }
            if matched {
                continue;
            }

            // Check for stray closing dangerous tags: </tag>
            for tag in DANGEROUS_TAGS {
                let prefix = format!("</{tag}");
                if starts_with_ignore_case(rest_upper, &prefix)
                    && rest.len() > prefix.len()
                    && bytes
                        .get(i + prefix.len())
                        .is_none_or(|b| b.is_ascii_whitespace() || *b == b'>')
                {
                    // Skip past the closing >.
                    while let Some(&(_, ch)) = chars.peek() {
                        chars.next();
                        if ch == '>' {
                            break;
                        }
                    }
                    matched = true;
                    break;
                }
            }
            if matched {
                continue;
            }

            result.push(c);
        } else {
            result.push(c);
        }
    }

    // Strip event handler attributes and dangerous URI schemes.
    strip_dangerous_attrs(&result)
}

/// Check if `s` starts with `prefix` (ASCII case-insensitive).
fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
    if s.len() < prefix.len() {
        return false;
    }
    s.as_bytes()[..prefix.len()]
        .iter()
        .zip(prefix.as_bytes())
        .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// Find the byte offset just past the closing `</tag>` for the given tag name,
/// starting the search from position `start`. Returns `None` if not found.
fn find_end_tag_ignore_case(input: &str, start: usize, tag: &str) -> Option<usize> {
    let search = &input.as_bytes()[start..];
    let tag_bytes = tag.as_bytes();
    let close_prefix_len = 2 + tag_bytes.len(); // "</" + tag

    for i in 0..search.len() {
        if search[i] == b'<'
            && i + 1 < search.len()
            && search[i + 1] == b'/'
            && i + close_prefix_len <= search.len()
        {
            let candidate = &search[i + 2..i + close_prefix_len];
            if candidate
                .iter()
                .zip(tag_bytes)
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
            {
                // Find the closing '>'.
                if let Some(gt) = search[i + close_prefix_len..]
                    .iter()
                    .position(|&b| b == b'>')
                {
                    return Some(start + i + close_prefix_len + gt + 1);
                }
            }
        }
    }
    None
}

/// Strip dangerous attributes from HTML/SVG tags: event handlers (`on*`) and
/// URI attributes (see [`is_uri_attr`]) with dangerous schemes
/// (see [`has_dangerous_uri_scheme`]). Only operates inside `<...>`
/// delimiters to avoid false positives in text content.
fn strip_dangerous_attrs(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        if bytes[pos] == b'<' && pos + 1 < bytes.len() && bytes[pos + 1] != b'/' {
            // Inside an opening tag — find the closing `>` using
            // quote-aware scanning so that `>` inside attribute values
            // (e.g. title=">") does not prematurely end the tag.
            if let Some(gt) = find_tag_end(&bytes[pos..]) {
                let tag_end = pos + gt + 1;
                let tag_content = &input[pos..tag_end];
                result.push_str(&sanitize_tag_attrs(tag_content));
                pos = tag_end;
            } else {
                result.push_str(&input[pos..]);
                break;
            }
        } else {
            // Outside a tag — advance one UTF-8 character at a time to
            // preserve multi-byte characters (CJK, emoji, accented, etc.).
            let ch = &input[pos..];
            let c = ch.chars().next().expect("pos is within bounds");
            result.push(c);
            pos += c.len_utf8();
        }
    }
    result
}

/// Find the index of the closing `>` of an opening tag, skipping `>` characters
/// inside quoted attribute values (`"..."` or `'...'`).
fn find_tag_end(bytes: &[u8]) -> Option<usize> {
    let mut i = 0;
    let mut in_quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = in_quote {
            if b == q {
                in_quote = None;
            }
        } else if b == b'"' || b == b'\'' {
            in_quote = Some(b);
        } else if b == b'>' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Check if a URI value starts with a dangerous scheme (`javascript:`,
/// `vbscript:`, `data:`), ignoring leading whitespace and case.
fn has_dangerous_uri_scheme(value: &str) -> bool {
    // Strip leading whitespace, then remove embedded ASCII control characters
    // and whitespace within the scheme portion to defend against obfuscation
    // like `java\tscript:` which some older browsers tolerated.
    // Filter runs before take(30) so the cap applies to meaningful characters,
    // preventing bypass via 20+ embedded whitespace/control characters.
    let lower: String = value
        .trim_start()
        .chars()
        .filter(|c| !c.is_ascii_whitespace() && !c.is_ascii_control())
        .take(30)
        .flat_map(|c| c.to_lowercase())
        .collect();
    // Blocked schemes — parity with is_safe_image_src which uses an allowlist approach:
    //   javascript/vbscript: code execution
    //   data:               content injection
    //   file:/blob:         local file access when HTML is opened as a local file
    //   mhtml:              MIME HTML (IE-era; blocked by is_safe_image_src via allowlist)
    // See OWASP XSS Prevention Cheat Sheet for further rationale.
    lower.starts_with("javascript:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("data:")
        || lower.starts_with("file:")
        || lower.starts_with("blob:")
        || lower.starts_with("mhtml:")
}

/// Check if an attribute name is a URI-bearing attribute that needs scheme
/// validation.
///
/// Covers the minimum set required by `.claude/rules/sanitizer-security.md`:
/// `href`, `xlink:href`, `src`, `action`, `formaction`, `poster`, `background`,
/// `ping`, `to`, `values`, `from`, `by`.
fn is_uri_attr(name: &str) -> bool {
    let lower: String = name.chars().flat_map(|c| c.to_lowercase()).collect();
    lower == "href"
        || lower == "src"
        || lower == "xlink:href"
        // SVG animation attributes that carry path/URI values
        || lower == "to"
        || lower == "values"
        || lower == "from"
        || lower == "by"
        // HTML form / navigation attributes that can carry executable URIs
        || lower == "action"
        || lower == "formaction"
        // Media / embed attributes
        || lower == "poster"
        || lower == "background"
        // Ping sends requests to the listed URLs on link click
        || lower == "ping"
}

/// Sanitize attributes in a single HTML/SVG tag string.
///
/// Removes event handler attributes (`on*`) entirely and strips URI attributes
/// (see [`is_uri_attr`]) that use dangerous schemes (see [`has_dangerous_uri_scheme`]).
///
/// This function operates at the byte level for performance. This is safe
/// because HTML/SVG tag names, attribute names, and structural characters
/// (`<`, `>`, `=`, `"`, `'`, `/`, whitespace) are all ASCII. Attribute
/// *values* are extracted via string slicing on the original `&str`, which
/// preserves UTF-8 correctness for non-ASCII content.
fn sanitize_tag_attrs(tag: &str) -> String {
    let mut result = String::with_capacity(tag.len());
    let bytes = tag.as_bytes();
    let mut i = 0;

    // Copy the '<' and tag name (always ASCII in valid HTML/SVG).
    while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b'>' && bytes[i] != b'/' {
        result.push(bytes[i] as char);
        i += 1;
    }

    // Remember the tag name (without the leading '<') for tag-specific
    // attribute rules such as the `<use>` fragment-only policy below.
    let tag_name = &result[1..];
    let is_use_tag =
        tag_name.eq_ignore_ascii_case("use") || tag_name.eq_ignore_ascii_case("svg:use");

    while i < bytes.len() {
        // Skip whitespace.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            result.push(bytes[i] as char);
            i += 1;
        }

        if i >= bytes.len() || bytes[i] == b'>' || bytes[i] == b'/' {
            result.push_str(&tag[i..]);
            return result;
        }

        // Read attribute name.
        let attr_start = i;
        while i < bytes.len()
            && bytes[i] != b'='
            && bytes[i] != b' '
            && bytes[i] != b'>'
            && bytes[i] != b'/'
        {
            i += 1;
        }
        let attr_name = &tag[attr_start..i];

        let is_event_handler = attr_name.len() > 2
            && attr_name.as_bytes()[..2].eq_ignore_ascii_case(b"on")
            && attr_name.as_bytes()[2].is_ascii_alphabetic();

        // Extract the attribute value (if any) without copying yet.
        let value_start = i;
        let mut attr_value: Option<String> = None;
        if i < bytes.len() && bytes[i] == b'=' {
            i += 1; // skip '='
            if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                let quote = bytes[i];
                i += 1;
                let val_start = i;
                while i < bytes.len() && bytes[i] != quote {
                    i += 1;
                }
                attr_value = Some(tag[val_start..i].to_string());
                if i < bytes.len() {
                    i += 1; // skip closing quote
                }
            } else {
                // Unquoted value.
                let val_start = i;
                while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                    i += 1;
                }
                attr_value = Some(tag[val_start..i].to_string());
            }
        }

        if is_event_handler {
            // Strip event handler attributes entirely.
            continue;
        }

        if is_uri_attr(attr_name) {
            if let Some(ref val) = attr_value {
                if has_dangerous_uri_scheme(val) {
                    // Strip the attribute if it uses a dangerous URI scheme.
                    continue;
                }
                // <use href="..."> / <use xlink:href="..."> must be
                // fragment-only (^#...). External URIs (even over https)
                // allow cross-origin tracking, referer leakage, and
                // timing-based exfiltration from rendered ChordPro
                // content. See issue #1828 and sanitizer-security.md
                // §SVG tag blocklists.
                if is_use_tag
                    && (attr_name.eq_ignore_ascii_case("href")
                        || attr_name.eq_ignore_ascii_case("xlink:href"))
                    && !val.trim_start().starts_with('#')
                {
                    continue;
                }
            }
        }

        // Strip style attributes that contain url() or expression() to
        // prevent CSS-based data exfiltration via network requests.
        if attr_name.eq_ignore_ascii_case("style") {
            if let Some(ref val) = attr_value {
                let lower_val: String = val.chars().flat_map(|c| c.to_lowercase()).collect();
                if lower_val.contains("url(")
                    || lower_val.contains("expression(")
                    || lower_val.contains("@import")
                {
                    continue;
                }
            }
        }

        // Copy the attribute as-is.
        result.push_str(&tag[attr_start..value_start]);
        if attr_value.is_some() {
            result.push_str(&tag[value_start..i]);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Directives
// ---------------------------------------------------------------------------

/// Render a directive as HTML (dispatches to section open/close/other).
///
/// StartOfChorus, EndOfChorus, and Chorus are handled directly in
/// `render_song` for chorus-recall state tracking.
fn render_directive_inner(
    directive: &chordsketch_core::ast::Directive,
    show_diagrams: bool,
    diagram_frets: usize,
    html: &mut String,
) {
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
        DirectiveKind::StartOfMusicxml => {
            render_section_open("musicxml", "MusicXML", &directive.value, html);
        }
        DirectiveKind::StartOfSection(section_name) => {
            let class = format!("section-{}", sanitize_css_class(section_name));
            let label = escape(&chordsketch_core::capitalize(section_name));
            render_section_open(&class, &label, &directive.value, html);
        }
        DirectiveKind::EndOfChorus
        | DirectiveKind::EndOfVerse
        | DirectiveKind::EndOfBridge
        | DirectiveKind::EndOfTab
        | DirectiveKind::EndOfGrid
        | DirectiveKind::EndOfAbc
        | DirectiveKind::EndOfLy
        | DirectiveKind::EndOfMusicxml
        | DirectiveKind::EndOfSvg
        | DirectiveKind::EndOfTextblock
        | DirectiveKind::EndOfSection(_) => {
            html.push_str("</section>\n");
        }
        DirectiveKind::Image(attrs) => {
            render_image(attrs, html);
        }
        DirectiveKind::Define if show_diagrams => {
            if let Some(ref value) = directive.value {
                let def = chordsketch_core::ast::ChordDefinition::parse_value(value);
                // Keyboard defines: render a piano keyboard SVG.
                if let Some(ref keys_raw) = def.keys {
                    let keys_u8: Vec<u8> = keys_raw
                        .iter()
                        .filter_map(|&k| {
                            if (0i32..=127).contains(&k) {
                                Some(k as u8)
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !keys_u8.is_empty() {
                        let root = keys_u8[0];
                        let voicing = chordsketch_core::chord_diagram::KeyboardVoicing {
                            name: def.name.clone(),
                            display_name: def.display.clone(),
                            keys: keys_u8,
                            root_key: root,
                        };
                        html.push_str("<div class=\"chord-diagram-container\">");
                        html.push_str(&chordsketch_core::chord_diagram::render_keyboard_svg(
                            &voicing,
                        ));
                        html.push_str("</div>\n");
                    }
                } else if let Some(ref raw) = def.raw {
                    // Fretted defines: render the standard fret-grid SVG.
                    if let Some(mut diagram) =
                        chordsketch_core::chord_diagram::DiagramData::from_raw_infer_frets(
                            &def.name,
                            raw,
                            diagram_frets,
                        )
                    {
                        diagram.display_name = def.display.clone();
                        html.push_str("<div class=\"chord-diagram-container\">");
                        html.push_str(&chordsketch_core::chord_diagram::render_svg(&diagram));
                        html.push_str("</div>\n");
                    }
                }
            }
        }
        DirectiveKind::Define => {}
        _ => {}
    }
}

/// Render ABC notation content using abc2svg, falling back to preformatted text.
///
/// When abc2svg is available and produces valid output, the SVG fragment is
/// embedded inside a `<section class="abc">` element. When abc2svg is
/// unavailable or fails, the raw ABC notation is rendered as preformatted text.
#[cfg(not(target_arch = "wasm32"))]
fn render_abc_with_fallback(
    abc_content: &str,
    label: &Option<String>,
    html: &mut String,
    warnings: &mut Vec<String>,
) {
    match chordsketch_core::external_tool::invoke_abc2svg(abc_content) {
        Ok(svg_fragment) => {
            render_section_open("abc", "ABC", label, html);
            html.push_str(&sanitize_svg_content(&svg_fragment));
            html.push('\n');
            html.push_str("</section>\n");
        }
        Err(e) => {
            push_warning(warnings, format!("abc2svg invocation failed: {e}"));
            render_section_open("abc", "ABC", label, html);
            html.push_str("<pre>");
            html.push_str(&escape(abc_content));
            html.push_str("</pre>\n");
            html.push_str("</section>\n");
        }
    }
}

/// Fallback for wasm32: external tools are never available, so render as
/// preformatted text. This function is unreachable in practice because
/// `has_abc2svg()` always returns false on wasm32, but the compiler needs it.
#[cfg(target_arch = "wasm32")]
fn render_abc_with_fallback(
    abc_content: &str,
    label: &Option<String>,
    html: &mut String,
    _warnings: &mut Vec<String>,
) {
    render_section_open("abc", "ABC", label, html);
    html.push_str("<pre>");
    html.push_str(&escape(abc_content));
    html.push_str("</pre>\n");
    html.push_str("</section>\n");
}

/// Check whether an image `src` value is safe to emit in HTML.
///
/// Re-export shared image-src validation from `chordsketch-core`.
///
/// The actual allowlist logic lives in `chordsketch_core::image_path::is_safe_image_src`
/// so every renderer (text, HTML, PDF) applies the same check — see
/// `.claude/rules/renderer-parity.md` §Validation Parity.
use chordsketch_core::image_path::is_safe_image_src;

/// Render Lilypond notation content using lilypond, falling back to preformatted text.
///
/// When lilypond is available and produces valid output, the SVG is embedded
/// inside a `<section class="ly">` element. When lilypond is unavailable or
/// fails, the raw notation is rendered as preformatted text.
#[cfg(not(target_arch = "wasm32"))]
fn render_ly_with_fallback(
    ly_content: &str,
    label: &Option<String>,
    html: &mut String,
    warnings: &mut Vec<String>,
) {
    match chordsketch_core::external_tool::invoke_lilypond(ly_content) {
        Ok(svg) => {
            render_section_open("ly", "Lilypond", label, html);
            html.push_str(&sanitize_svg_content(&svg));
            html.push('\n');
            html.push_str("</section>\n");
        }
        Err(e) => {
            push_warning(warnings, format!("lilypond invocation failed: {e}"));
            render_section_open("ly", "Lilypond", label, html);
            html.push_str("<pre>");
            html.push_str(&escape(ly_content));
            html.push_str("</pre>\n");
            html.push_str("</section>\n");
        }
    }
}

/// Fallback for wasm32: external tools are never available, so render as
/// preformatted text. Unreachable in practice because `has_lilypond()` always
/// returns false on wasm32.
#[cfg(target_arch = "wasm32")]
fn render_ly_with_fallback(
    ly_content: &str,
    label: &Option<String>,
    html: &mut String,
    _warnings: &mut Vec<String>,
) {
    render_section_open("ly", "Lilypond", label, html);
    html.push_str("<pre>");
    html.push_str(&escape(ly_content));
    html.push_str("</pre>\n");
    html.push_str("</section>\n");
}

/// Render MusicXML content using MuseScore, falling back to preformatted text.
///
/// When MuseScore is available and produces valid output, the SVG is embedded
/// inside a `<section class="musicxml">` element. When MuseScore is unavailable
/// or fails, the raw MusicXML is rendered as preformatted text.
#[cfg(not(target_arch = "wasm32"))]
fn render_musicxml_with_fallback(
    musicxml_content: &str,
    label: &Option<String>,
    html: &mut String,
    warnings: &mut Vec<String>,
) {
    match chordsketch_core::external_tool::invoke_musescore(musicxml_content) {
        Ok(svg) => {
            render_section_open("musicxml", "MusicXML", label, html);
            html.push_str(&sanitize_svg_content(&svg));
            html.push('\n');
            html.push_str("</section>\n");
        }
        Err(e) => {
            push_warning(warnings, format!("musescore invocation failed: {e}"));
            render_section_open("musicxml", "MusicXML", label, html);
            html.push_str("<pre>");
            html.push_str(&escape(musicxml_content));
            html.push_str("</pre>\n");
            html.push_str("</section>\n");
        }
    }
}

/// Fallback for wasm32: external tools are never available, so render as
/// preformatted text. Unreachable in practice because `has_musescore()` always
/// returns false on wasm32.
#[cfg(target_arch = "wasm32")]
fn render_musicxml_with_fallback(
    musicxml_content: &str,
    label: &Option<String>,
    html: &mut String,
    _warnings: &mut Vec<String>,
) {
    render_section_open("musicxml", "MusicXML", label, html);
    html.push_str("<pre>");
    html.push_str(&escape(musicxml_content));
    html.push_str("</pre>\n");
    html.push_str("</section>\n");
}

/// Render an `{image}` directive as an HTML `<img>` element.
///
/// Generates a `<div>` wrapper (with optional alignment from the `anchor`
/// attribute) containing an `<img>` tag.  The `src`, `width`, `height`, and
/// `title` (as `alt`) attributes are forwarded.  A `scale` value is applied
/// via a CSS `transform: scale(…)` style.
///
/// Paths that fail [`is_safe_image_src`] are silently skipped.
fn render_image(attrs: &chordsketch_core::ast::ImageAttributes, html: &mut String) {
    if !is_safe_image_src(&attrs.src) {
        return;
    }

    let mut style = String::new();
    let mut img_attrs = format!("src=\"{}\"", escape(&attrs.src));

    if let Some(ref title) = attrs.title {
        let _ = write!(img_attrs, " alt=\"{}\"", escape(title));
    }

    if let Some(ref width) = attrs.width {
        let _ = write!(img_attrs, " width=\"{}\"", escape(width));
    }
    if let Some(ref height) = attrs.height {
        let _ = write!(img_attrs, " height=\"{}\"", escape(height));
    }
    if let Some(ref scale) = attrs.scale {
        // Scale as a CSS transform
        let _ = write!(
            style,
            "transform: scale({});transform-origin: top left;",
            sanitize_css_value(scale)
        );
    }

    // Determine wrapper alignment
    let align_css = match attrs.anchor.as_deref() {
        Some("column") | Some("paper") => "text-align: center;",
        _ => "",
    };

    if !align_css.is_empty() {
        let _ = write!(html, "<div style=\"{align_css}\">");
    } else {
        html.push_str("<div>");
    }

    let _ = write!(html, "<img {img_attrs}");
    if !style.is_empty() {
        // The style string is first sanitised (sanitize_css_value removes
        // dangerous characters) and then HTML-escaped here.  The double
        // processing is intentional: sanitisation makes the CSS value safe,
        // while escape() ensures the surrounding attribute context is safe
        // (e.g. a `"` in the style cannot break out of the attribute).
        let _ = write!(html, " style=\"{}\"", escape(&style));
    }
    html.push_str("></div>\n");
}

/// Open a `<section>` with a class and optional label.
fn render_section_open(class: &str, label: &str, value: &Option<String>, html: &mut String) {
    let safe_class = sanitize_css_class(class);
    let _ = writeln!(html, "<section class=\"{safe_class}\">");
    let display_label = match value {
        Some(v) if !v.is_empty() => format!("{label}: {}", escape(v)),
        _ => label.to_string(),
    };
    let _ = writeln!(html, "<div class=\"section-label\">{display_label}</div>");
}

/// Render a `{chorus}` recall directive as HTML.
///
/// Re-renders the stored chorus AST lines with the current transpose offset,
/// so chords are transposed correctly even if `{transpose}` changed after
/// the chorus was defined.
fn render_chorus_recall(
    value: &Option<String>,
    chorus_body: &[Line],
    transpose_offset: i8,
    fmt_state: &FormattingState,
    show_diagrams: bool,
    diagram_frets: usize,
    html: &mut String,
) {
    html.push_str("<div class=\"chorus-recall\">\n");
    let display_label = match value {
        Some(v) if !v.is_empty() => format!("Chorus: {}", escape(v)),
        _ => "Chorus".to_string(),
    };
    let _ = writeln!(html, "<div class=\"section-label\">{display_label}</div>");
    // Use a local copy of fmt_state so in-chorus formatting directives
    // (e.g. {size}, {bold}) are applied during recall without mutating
    // the caller's state.
    let mut local_fmt = fmt_state.clone();
    for line in chorus_body {
        match line {
            Line::Lyrics(lyrics) => render_lyrics(lyrics, transpose_offset, &local_fmt, html),
            Line::Comment(style, text) => render_comment(*style, text, html),
            Line::Empty => html.push_str("<div class=\"empty-line\"></div>\n"),
            Line::Directive(d) if d.kind.is_font_size_color() => {
                local_fmt.apply(&d.kind, &d.value);
            }
            Line::Directive(d) if !d.kind.is_metadata() => {
                render_directive_inner(d, show_diagrams, diagram_frets, html);
            }
            _ => {}
        }
    }
    html.push_str("</div>\n");
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

/// Render a comment as HTML.
fn render_comment(style: CommentStyle, text: &str, html: &mut String) {
    match style {
        CommentStyle::Normal => {
            let _ = writeln!(html, "<p class=\"comment\">{}</p>", escape(text));
        }
        CommentStyle::Italic => {
            let _ = writeln!(html, "<p class=\"comment\"><em>{}</em></p>", escape(text));
        }
        CommentStyle::Boxed => {
            let _ = writeln!(html, "<div class=\"comment-box\">{}</div>", escape(text));
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod sanitize_tag_attrs_tests {
    use super::*;

    #[test]
    fn test_preserves_normal_attrs() {
        let tag = "<svg width=\"100\" height=\"50\">";
        assert_eq!(sanitize_tag_attrs(tag), tag);
    }

    #[test]
    fn test_strips_event_handler() {
        let tag = "<svg onclick=\"alert(1)\" width=\"100\">";
        let result = sanitize_tag_attrs(tag);
        assert!(!result.contains("onclick"));
        assert!(result.contains("width"));
    }

    #[test]
    fn test_non_ascii_in_attr_value_preserved() {
        let tag = "<text title=\"日本語テスト\" x=\"10\">";
        let result = sanitize_tag_attrs(tag);
        assert!(result.contains("日本語テスト"));
        assert!(result.contains("x=\"10\""));
    }

    // --- Case-insensitive event handler detection (#663) ---

    #[test]
    fn test_strips_mixed_case_event_handler() {
        let tag = "<svg OnClick=\"alert(1)\" width=\"100\">";
        let result = sanitize_tag_attrs(tag);
        assert!(!result.contains("OnClick"));
        assert!(result.contains("width"));
    }

    #[test]
    fn test_strips_uppercase_event_handler() {
        let tag = "<svg ONLOAD=\"alert(1)\">";
        let result = sanitize_tag_attrs(tag);
        assert!(!result.contains("ONLOAD"));
    }

    // --- Style attribute sanitization (#654) ---

    #[test]
    fn test_strips_style_with_url() {
        let tag =
            "<rect style=\"background-image: url('https://attacker.com/exfil')\" width=\"10\">";
        let result = sanitize_tag_attrs(tag);
        assert!(!result.contains("style"));
        assert!(result.contains("width"));
    }

    #[test]
    fn test_strips_style_with_expression() {
        let tag = "<rect style=\"width: expression(alert(1))\">";
        let result = sanitize_tag_attrs(tag);
        assert!(!result.contains("style"));
    }

    #[test]
    fn test_strips_style_with_import() {
        let tag = "<rect style=\"@import url(evil.css)\">";
        let result = sanitize_tag_attrs(tag);
        assert!(!result.contains("style"));
    }

    #[test]
    fn test_preserves_safe_style() {
        let tag = "<rect style=\"fill: red; stroke: blue\" width=\"10\">";
        let result = sanitize_tag_attrs(tag);
        assert!(result.contains("style"));
        assert!(result.contains("fill: red"));
    }

    #[test]
    fn test_use_strips_relative_url_href() {
        // `<use>` only allows fragment-only (`#foo`) hrefs (policy added in
        // PR #1857, which this test extends; see also #1828 for the broader
        // SVG attack surface tracker). A relative URL like
        // `sprites.svg#icon` is NOT fragment-only — it resolves against the
        // document's base URL and could fetch an external SVG sprite sheet.
        // The fragment-only check strips it.
        let tag = "<use href=\"sprites.svg#icon\">";
        let result = sanitize_tag_attrs(tag);
        assert!(
            !result.contains("href="),
            "relative URL must be stripped for <use>; got {result:?}"
        );
    }

    #[test]
    fn test_use_preserves_whitespace_prefixed_fragment_href() {
        // Some serializers emit `href=" #symbol"` with leading whitespace
        // around the value. The fragment-only check uses `trim_start` so
        // whitespace-padded fragments are still accepted.
        let tag = "<use href=\" #myShape\">";
        let result = sanitize_tag_attrs(tag);
        // Match `href=` (with the `=`) specifically: the bare `href`
        // substring would also match the tag name `<use>` plus the word
        // `href` appearing in unrelated sanitized output.
        assert!(
            result.contains("href="),
            "whitespace-prefixed fragment href must be preserved; got {result:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_empty() {
        let song = chordsketch_core::parse("").unwrap();
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
        // The `'` must be escaped so single-quote attribute boundaries
        // cannot be broken. Both `&#39;` and `&apos;` are acceptable.
        assert!(html.contains("&apos;") || html.contains("&#39;"));
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

    #[test]
    fn test_custom_section_capitalize_before_escape() {
        // Section name starting with "&" — capitalize must run on the
        // original text, then escape the result. If escape runs first,
        // capitalize would operate on "&amp;" instead.
        let html = render("{start_of_&test}\ntext\n{end_of_&test}");
        // Should capitalize the "&" (no-op) then escape -> "&amp;test"
        // NOT capitalize "&amp;" -> "&Amp;test"
        assert!(html.contains("&amp;test"));
        assert!(!html.contains("&Amp;"));
    }

    #[test]
    fn test_define_display_name_in_html_output() {
        let html = render("{define: Am base-fret 1 frets x 0 2 2 1 0 display=\"A minor\"}");
        assert!(
            html.contains("A minor"),
            "display name should appear in rendered HTML output"
        );
    }
}

#[cfg(test)]
mod transpose_tests {
    use super::*;

    #[test]
    fn test_transpose_directive_up_2() {
        let input = "{transpose: 2}\n[G]Hello [C]world";
        let song = chordsketch_core::parse(input).unwrap();
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
        let song = chordsketch_core::parse(input).unwrap();
        let html = render_song(&song);
        // First G transposed +2 = A, second G at 0 = G
        assert!(html.contains("<span class=\"chord\">A</span>"));
        assert!(html.contains("<span class=\"chord\">G</span>"));
    }

    #[test]
    fn test_transpose_directive_with_cli_offset() {
        let input = "{transpose: 2}\n[C]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let html = render_song_with_transpose(&song, 3, &Config::defaults());
        // 2 + 3 = 5, C+5=F
        assert!(html.contains("<span class=\"chord\">F</span>"));
    }

    #[test]
    fn test_transpose_out_of_i8_range_emits_warning() {
        // 999 cannot be represented as i8; should fall back to 0 with a warning
        let input = "{transpose: 999}\n[G]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let result = render_song_with_warnings(&song, 0, &Config::defaults());
        assert!(
            result.output.contains("<span class=\"chord\">G</span>"),
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
        // {transpose} with no value should silently reset to 0, no warning.
        let input = "{transpose}\n[G]Hello";
        let song = chordsketch_core::parse(input).unwrap();
        let result = render_song_with_warnings(&song, 0, &Config::defaults());
        assert!(
            result.output.contains("<span class=\"chord\">G</span>"),
            "chord should be untransposed"
        );
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
        let result = render_song_with_warnings(&song, 0, &Config::defaults());
        assert!(
            result.output.contains("<span class=\"chord\">G</span>"),
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

    #[test]
    fn test_chorus_recall_applies_current_transpose() {
        // Chorus defined with no transpose, recalled after {transpose: 2}.
        // G should become A in the recalled chorus.
        let html = render("{start_of_chorus}\n[G]La la\n{end_of_chorus}\n{transpose: 2}\n{chorus}");
        // Original chorus has chord "G"
        assert!(
            html.contains("<span class=\"chord\">G</span>"),
            "original chorus should have G"
        );
        // Recalled chorus should have transposed chord "A"
        assert!(
            html.contains("<span class=\"chord\">A</span>"),
            "recalled chorus should have transposed chord A, got:\n{html}"
        );
    }

    #[test]
    fn test_chorus_recall_preserves_formatting_directives() {
        // A {textsize: 20} inside the chorus should be applied at recall time.
        let html =
            render("{start_of_chorus}\n{textsize: 20}\n[Am]Big text\n{end_of_chorus}\n{chorus}");
        // The recall section should contain the font-size style.
        let recall_start = html.find("chorus-recall").expect("should have recall");
        let recall_section = &html[recall_start..];
        assert!(
            recall_section.contains("font-size"),
            "recalled chorus should apply in-chorus formatting directives"
        );
    }

    #[test]
    fn test_chorus_formatting_does_not_leak_to_outer_scope() {
        // {textsize: 20} inside chorus must not affect text after the chorus.
        let html =
            render("{start_of_chorus}\n{textsize: 20}\n[Am]Big\n{end_of_chorus}\n[G]Normal text");
        // Find content after </section> (end of chorus)
        let after_chorus = html
            .rfind("Normal text")
            .expect("should have post-chorus text");
        // Look backward from "Normal text" for the nearest <div class="line">
        let line_start = html[..after_chorus].rfind("<div class=\"line\"").unwrap();
        let line_end = html[line_start..]
            .find("</div>")
            .map_or(html.len(), |i| line_start + i + 6);
        let post_chorus_line = &html[line_start..line_end];
        assert!(
            !post_chorus_line.contains("font-size"),
            "in-chorus {{textsize}} should not leak to post-chorus content: {post_chorus_line}"
        );
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
    fn test_new_physical_page_generates_recto_break() {
        let html = render("Page 1\n{new_physical_page}\nPage 2");
        assert!(
            html.contains("break-before: recto;"),
            "new_physical_page should use break-before: recto for duplex printing"
        );
        assert!(
            !html.contains("break-before: page;"),
            "new_physical_page should not emit generic page break"
        );
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

    #[test]
    fn test_image_empty_src_skipped() {
        let html = render("{image: src=}");
        assert!(
            !html.contains("<img"),
            "empty src should not produce an img element"
        );
    }

    #[test]
    fn test_image_javascript_uri_rejected() {
        let html = render("{image: src=javascript:alert(1)}");
        assert!(!html.contains("<img"), "javascript: URI must be rejected");
    }

    #[test]
    fn test_image_data_uri_rejected() {
        let html = render("{image: src=data:text/html,<script>alert(1)</script>}");
        assert!(!html.contains("<img"), "data: URI must be rejected");
    }

    #[test]
    fn test_image_vbscript_uri_rejected() {
        let html = render("{image: src=vbscript:MsgBox}");
        assert!(!html.contains("<img"), "vbscript: URI must be rejected");
    }

    #[test]
    fn test_image_javascript_uri_case_insensitive() {
        let html = render("{image: src=JaVaScRiPt:alert(1)}");
        assert!(
            !html.contains("<img"),
            "scheme check must be case-insensitive"
        );
    }

    #[test]
    fn test_image_safe_relative_path_allowed() {
        let html = render("{image: src=images/photo.jpg}");
        assert!(html.contains("<img src=\"images/photo.jpg\""));
    }

    // -- {capo} validation parity (#1834) ---------------------------------

    #[test]
    fn test_capo_out_of_range_emits_warning() {
        let song = chordsketch_core::parse("{title: T}\n{capo: 999}").unwrap();
        let result = render_song_with_warnings(&song, 0, &Config::defaults());
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("capo") && w.contains("999")),
            "expected out-of-range {{capo}} warning; got {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_capo_non_numeric_emits_warning() {
        let song = chordsketch_core::parse("{title: T}\n{capo: foo}").unwrap();
        let result = render_song_with_warnings(&song, 0, &Config::defaults());
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("capo") && w.contains("foo")),
            "expected non-integer {{capo}} warning; got {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_capo_in_range_is_silent() {
        let song = chordsketch_core::parse("{title: T}\n{capo: 5}").unwrap();
        let result = render_song_with_warnings(&song, 0, &Config::defaults());
        assert!(
            !result.warnings.iter().any(|w| w.contains("capo")),
            "valid {{capo: 5}} should not warn; got {:?}",
            result.warnings
        );
    }

    // -- MAX_WARNINGS cap (#1833) -----------------------------------------

    #[test]
    fn test_max_warnings_truncates() {
        let mut input = String::from("{title: T}\n");
        for _ in 0..(MAX_WARNINGS + 50) {
            input.push_str("{transpose: not-a-number}\n");
        }
        let song = chordsketch_core::parse(&input).unwrap();
        let result = render_song_with_warnings(&song, 0, &Config::defaults());
        assert_eq!(
            result.warnings.len(),
            MAX_WARNINGS + 1,
            "expected exactly MAX_WARNINGS warnings plus one truncation marker"
        );
        assert!(
            result.warnings.last().unwrap().contains("MAX_WARNINGS"),
            "last entry must be the truncation marker; got {:?}",
            result.warnings.last()
        );
    }

    #[test]
    fn test_is_safe_image_src() {
        // Allowed: relative paths
        assert!(is_safe_image_src("photo.jpg"));
        assert!(is_safe_image_src("images/photo.jpg"));
        assert!(is_safe_image_src("path/to:file.jpg")); // colon after slash is not a scheme

        // Allowed: http/https
        assert!(is_safe_image_src("http://example.com/photo.jpg"));
        assert!(is_safe_image_src("https://example.com/photo.jpg"));
        assert!(is_safe_image_src("HTTP://EXAMPLE.COM/PHOTO.JPG"));

        // Rejected: empty
        assert!(!is_safe_image_src(""));

        // Rejected: dangerous schemes (denylist is now implicit via allowlist)
        assert!(!is_safe_image_src("javascript:alert(1)"));
        assert!(!is_safe_image_src("JAVASCRIPT:alert(1)"));
        assert!(!is_safe_image_src("  javascript:alert(1)"));
        assert!(!is_safe_image_src("data:image/png;base64,abc"));
        assert!(!is_safe_image_src("vbscript:MsgBox"));

        // Rejected: file/blob/mhtml schemes (previously allowed)
        assert!(!is_safe_image_src("file:///etc/passwd"));
        assert!(!is_safe_image_src("FILE:///etc/passwd"));
        assert!(!is_safe_image_src("blob:https://example.com/uuid"));
        assert!(!is_safe_image_src("mhtml:file://C:/page.mhtml"));

        // Rejected: absolute filesystem paths
        assert!(!is_safe_image_src("/etc/passwd"));
        assert!(!is_safe_image_src("/home/user/photo.jpg"));

        // Rejected: null bytes
        assert!(!is_safe_image_src("photo\0.jpg"));
        assert!(!is_safe_image_src("\0"));

        // Rejected: directory traversal
        assert!(!is_safe_image_src("../photo.jpg"));
        assert!(!is_safe_image_src("images/../../etc/passwd"));
        assert!(!is_safe_image_src(r"..\photo.jpg"));
        assert!(!is_safe_image_src(r"images\..\..\photo.jpg"));

        // Rejected: Windows-style absolute paths (all platforms)
        assert!(!is_safe_image_src(r"C:\photo.jpg"));
        assert!(!is_safe_image_src(r"D:\Users\photo.jpg"));
        assert!(!is_safe_image_src(r"\\server\share\photo.jpg"));
        assert!(!is_safe_image_src("C:/photo.jpg"));
    }

    #[test]
    fn test_image_anchor_column_centers() {
        let html = render("{image: src=photo.jpg anchor=column}");
        assert!(
            html.contains("<div style=\"text-align: center;\">"),
            "anchor=column should produce centered div"
        );
    }

    #[test]
    fn test_image_anchor_paper_centers() {
        let html = render("{image: src=photo.jpg anchor=paper}");
        assert!(
            html.contains("<div style=\"text-align: center;\">"),
            "anchor=paper should produce centered div"
        );
    }

    #[test]
    fn test_image_anchor_line_no_style() {
        let html = render("{image: src=photo.jpg anchor=line}");
        // anchor=line should produce a bare <div> without style
        assert!(html.contains("<div><img"));
        assert!(!html.contains("text-align"));
    }

    #[test]
    fn test_image_no_anchor_no_style() {
        let html = render("{image: src=photo.jpg}");
        // No anchor should produce a bare <div> without style
        assert!(html.contains("<div><img"));
        assert!(!html.contains("text-align"));
    }

    #[test]
    fn test_image_max_width_css_present() {
        let html = render("{image: src=photo.jpg}");
        assert!(
            html.contains("img { max-width: 100%; height: auto; }"),
            "CSS should include img max-width rule to prevent overflow"
        );
    }

    #[test]
    fn test_chord_diagram_css_rules_present() {
        let html = render("{define: Am base-fret 1 frets x 0 2 2 1 0}");
        assert!(
            html.contains(".chord-diagram-container"),
            "CSS should include .chord-diagram-container rule"
        );
        assert!(
            html.contains(".chord-diagram {"),
            "CSS should include .chord-diagram rule"
        );
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
    fn test_define_keyboard_renders_keyboard_svg() {
        // {define: Am keys 0 3 7} should now render a keyboard diagram SVG.
        let html = render("{define: Am keys 0 3 7}");
        assert!(
            html.contains("<svg"),
            "keyboard define should produce an SVG"
        );
        assert!(
            html.contains("keyboard-diagram"),
            "should use keyboard-diagram CSS class"
        );
        assert!(html.contains("Am"), "chord name should appear in SVG");
    }

    #[test]
    fn test_define_keyboard_absolute_midi_renders_svg() {
        // Absolute MIDI note numbers (as in the issue spec example).
        let html = render("{define: Cmaj7 keys 60 64 67 71}");
        assert!(html.contains("<svg"));
        assert!(html.contains("keyboard-diagram"));
        assert!(html.contains("Cmaj7"));
    }

    #[test]
    fn test_diagrams_piano_auto_inject() {
        let input = "{diagrams: piano}\n[Am]Hello [C]world";
        let html = render(input);
        // Should auto-inject keyboard diagrams for Am and C
        assert!(
            html.contains("keyboard-diagram"),
            "piano instrument should use keyboard diagrams"
        );
        assert!(
            html.contains("chord-diagrams"),
            "diagram section should be present"
        );
    }

    #[test]
    fn test_define_ukulele_diagram() {
        let html = render("{define: C frets 0 0 0 3}");
        assert!(html.contains("<svg"));
        assert!(html.contains("chord-diagram"));
        // 4 strings: SVG width = (4-1)*16 + 20*2 = 88
        assert!(
            html.contains("width=\"88\""),
            "Expected 4-string SVG width (88)"
        );
    }

    #[test]
    fn test_define_banjo_diagram() {
        let html = render("{define: G frets 0 0 0 0 0}");
        assert!(html.contains("<svg"));
        // 5 strings: SVG width = (5-1)*16 + 20*2 = 104
        assert!(
            html.contains("width=\"104\""),
            "Expected 5-string SVG width (104)"
        );
    }

    #[test]
    fn test_diagrams_frets_config_controls_svg_height() {
        let input = "{define: Am base-fret 1 frets x 0 2 2 1 0}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("diagrams.frets=4")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        // 4 frets: grid_h = 4*20 = 80, total_h = 80 + 30 + 30 = 140
        assert!(
            html.contains("height=\"140\""),
            "SVG height should reflect diagrams.frets=4 (expected 140)"
        );
    }

    // -- {diagrams} directive tests -----------------------------------------------

    #[test]
    fn test_diagrams_off_suppresses_chord_diagrams() {
        let html = render("{diagrams: off}\n{define: Am base-fret 1 frets x 0 2 2 1 0}");
        assert!(
            !html.contains("<svg"),
            "chord diagram SVG should be suppressed when diagrams=off"
        );
    }

    #[test]
    fn test_diagrams_on_shows_chord_diagrams() {
        let html = render("{diagrams: on}\n{define: Am base-fret 1 frets x 0 2 2 1 0}");
        assert!(
            html.contains("<svg"),
            "chord diagram SVG should be shown when diagrams=on"
        );
    }

    #[test]
    fn test_diagrams_default_shows_chord_diagrams() {
        let html = render("{define: Am base-fret 1 frets x 0 2 2 1 0}");
        assert!(
            html.contains("<svg"),
            "chord diagram SVG should be shown by default"
        );
    }

    #[test]
    fn test_diagrams_off_then_on_restores() {
        let html = render(
            "{diagrams: off}\n{define: Am base-fret 1 frets x 0 2 2 1 0}\n{diagrams: on}\n{define: G base-fret 1 frets 3 2 0 0 0 3}",
        );
        // Am should be suppressed, G should be shown
        assert!(!html.contains(">Am<"), "Am diagram should be suppressed");
        assert!(html.contains(">G<"), "G diagram should be rendered");
    }

    #[test]
    fn test_diagrams_parsed_as_known_directive() {
        let song = chordsketch_core::parse("{diagrams: off}").unwrap();
        if let chordsketch_core::ast::Line::Directive(d) = &song.lines[0] {
            assert_eq!(
                d.kind,
                chordsketch_core::ast::DirectiveKind::Diagrams,
                "diagrams should parse as DirectiveKind::Diagrams"
            );
            assert_eq!(d.value, Some("off".to_string()));
        } else {
            panic!("expected a directive line, got: {:?}", &song.lines[0]);
        }
    }

    // --- Case-insensitive {diagrams} directive (#652) ---

    #[test]
    fn test_diagrams_off_case_insensitive() {
        let html = render("{diagrams: Off}\n{define: Am base-fret 1 frets x 0 2 2 1 0}");
        assert!(
            !html.contains("<svg"),
            "diagrams=Off should suppress diagrams (case-insensitive)"
        );
    }

    #[test]
    fn test_diagrams_off_uppercase() {
        let html = render("{diagrams: OFF}\n{define: Am base-fret 1 frets x 0 2 2 1 0}");
        assert!(
            !html.contains("<svg"),
            "diagrams=OFF should suppress diagrams (case-insensitive)"
        );
    }

    // -- auto-inject diagram grid (issue #1140) -----------------------------------

    #[test]
    fn test_diagrams_auto_inject_from_builtin_db() {
        // {diagrams} with known chords should append a grid section
        let html = render("{diagrams}\n[Am]Hello [G]World");
        assert!(
            html.contains("class=\"chord-diagrams\""),
            "should render chord-diagrams section"
        );
        // Both Am and G are in the built-in guitar DB
        assert!(html.contains(">Am<"), "Am diagram expected");
        assert!(html.contains(">G<"), "G diagram expected");
    }

    #[test]
    fn test_diagrams_auto_inject_unknown_chord_skipped() {
        // Unknown chords (not in DB, no {define}) should be silently skipped
        let html = render("{diagrams}\n[Xyzzy]Hello");
        // No chord-diagrams section because no known chords
        assert!(
            !html.contains("class=\"chord-diagrams\""),
            "no diagram section for unknown chord"
        );
    }

    #[test]
    fn test_no_diagrams_suppresses_auto_inject() {
        let html = render("{no_diagrams}\n[Am]Hello");
        assert!(
            !html.contains("class=\"chord-diagrams\""),
            "{{no_diagrams}} should suppress auto-inject"
        );
    }

    #[test]
    fn test_diagrams_define_takes_priority_over_builtin() {
        // Chords with a {define} entry are rendered inline at the directive position
        // and excluded from the auto-inject grid (dedup).  When all used chords are
        // defined, the auto-inject section is absent entirely.
        let html = render("{diagrams}\n{define: Am base-fret 1 frets x 0 2 2 1 0}\n[Am]Hello");
        // Am is rendered inline (at the {define} position).
        assert!(
            html.contains("font-weight=\"bold\">Am</text>"),
            "Am diagram should appear inline at the {{define}} position"
        );
        // All used chords have {define} entries → grid is not rendered.
        assert!(
            !html.contains("class=\"chord-diagrams\""),
            "auto-inject section should be absent when all used chords are defined"
        );
    }

    #[test]
    fn test_diagrams_off_suppresses_auto_inject() {
        let html = render("{diagrams: off}\n[Am]Hello");
        assert!(
            !html.contains("class=\"chord-diagrams\""),
            "{{diagrams: off}} should suppress auto-inject grid"
        );
    }

    #[test]
    fn test_diagrams_ukulele_instrument() {
        let html = render("{diagrams: ukulele}\n[Am]Hello");
        assert!(
            html.contains("class=\"chord-diagrams\""),
            "ukulele diagrams section expected"
        );
        // Ukulele Am has 4 strings so the SVG will differ from guitar
        assert!(html.contains(">Am<"), "Am diagram expected");
    }

    #[test]
    fn test_diagrams_guitar_explicit_overrides_config_default() {
        // Even when config could default to ukulele, {diagrams: guitar} should
        // use guitar (6-string Am) not ukulele (4-string Am).
        let song = chordsketch_core::parse("{diagrams: guitar}\n[Am]Hello").unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("diagrams.instrument=ukulele")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(
            html.contains("class=\"chord-diagrams\""),
            "guitar diagrams section expected"
        );
        assert!(html.contains(">Am<"), "Am diagram expected");
        let guitar_am_html = render_song_with_transpose(
            &chordsketch_core::parse("{diagrams: guitar}\n[Am]Hello").unwrap(),
            0,
            &chordsketch_core::config::Config::defaults(),
        );
        let uke_am_html = render_song_with_transpose(
            &chordsketch_core::parse("{diagrams: ukulele}\n[Am]Hello").unwrap(),
            0,
            &chordsketch_core::config::Config::defaults(),
        );
        // Guitar and ukulele diagrams must differ in their SVG content.
        assert_ne!(
            guitar_am_html, uke_am_html,
            "guitar and ukulele Am diagrams should differ"
        );
        // With config defaulting to ukulele, {diagrams: guitar} must produce
        // the same output as the guitar default.
        assert_eq!(
            html, guitar_am_html,
            "{{diagrams: guitar}} must select guitar regardless of config default"
        );
    }

    #[test]
    fn test_no_diagrams_suppresses_inline_define_diagrams() {
        // {no_diagrams} should suppress inline {define} diagram rendering
        // (show_diagrams = false), not just the auto-inject grid.
        let html = render("{no_diagrams}\n{define: Am base-fret 1 frets x 0 2 2 1 0}\n[Am]Hello");
        assert!(
            !html.contains("<svg"),
            "{{no_diagrams}} should suppress inline define diagram SVG"
        );
    }

    #[test]
    fn test_define_chord_not_duplicated_in_auto_inject_grid() {
        // When a chord has a {define} entry (rendered inline) and also appears in
        // lyrics with {diagrams} active, the auto-inject grid must NOT include it
        // again. Regression test for #1211.
        let html =
            render("{define: Am base-fret 1 frets x 0 2 2 1 0}\n{diagrams}\n[Am]Hello [G]world\n");
        // Am was rendered inline at the {define} position; count SVG occurrences.
        let am_svg_count = html.match_indices("font-weight=\"bold\">Am</text>").count();
        assert_eq!(
            am_svg_count, 1,
            "Am diagram should appear exactly once (inline via {{define}}), not also in auto-inject grid"
        );
        // G has no {define} and should appear in the auto-inject grid.
        assert!(
            html.contains("font-weight=\"bold\">G</text>"),
            "G diagram should appear in the auto-inject grid"
        );
    }

    #[test]
    fn test_define_after_nodiagrams_appears_in_grid() {
        // {define} encountered while show_diagrams=false must NOT be tracked as
        // inline-rendered; the chord should appear in the auto-inject grid.
        // Regression test for #1245.
        let html = render(
            "{no_diagrams}\n{define: Am base-fret 1 frets x 0 2 2 1 0}\n{diagrams}\n[Am]Hello\n",
        );
        // Am was NOT rendered inline ({no_diagrams} was active at {define} time).
        // It should appear in the auto-inject grid.
        assert!(
            html.contains("class=\"chord-diagrams\""),
            "auto-inject grid should appear since Am was not rendered inline"
        );
        assert!(
            html.contains("font-weight=\"bold\">Am</text>"),
            "Am should appear in the auto-inject grid"
        );
    }

    #[test]
    fn test_enharmonic_define_dedup() {
        // {define: Bb …} + [A#] in lyrics: the flat/sharp pair must be treated as
        // the same chord so A# is excluded from the auto-inject grid.
        // Regression test for #1246.
        let html = render("{define: Bb base-fret 1 frets x 1 3 3 3 1}\n{diagrams}\n[A#]Hello\n");
        // Bb was rendered inline (as Bb); A# is the same chord enharmonically.
        let bb_count = html.match_indices("font-weight=\"bold\">Bb</text>").count();
        let as_count = html.match_indices("font-weight=\"bold\">A#</text>").count();
        assert_eq!(bb_count, 1, "Bb should appear once (inline)");
        assert_eq!(
            as_count, 0,
            "A# should NOT appear in the auto-inject grid (same chord as Bb)"
        );
    }

    #[test]
    fn test_chord_directive_appears_in_auto_inject_grid() {
        // {chord} (DirectiveKind::ChordDirective) does not render inline — it must
        // always appear in the auto-inject grid.  Regression test for #1250.
        let html = render("{chord: Am base-fret 1 frets x 0 2 2 1 0}\n{diagrams}\n[Am]Hello\n");
        // Am has a {chord} entry but no inline diagram was rendered.
        // It should appear in the auto-inject grid.
        assert!(
            html.contains("class=\"chord-diagrams\""),
            "auto-inject grid should appear since {{chord}} does not render inline"
        );
        assert!(
            html.contains("font-weight=\"bold\">Am</text>"),
            "Am should appear in the auto-inject grid via {{chord}} voicing"
        );
    }

    // -- abc2svg delegate rendering tests -----------------------------------------

    #[test]
    fn test_abc_section_disabled_by_config() {
        // With delegates.abc2svg explicitly disabled, ABC renders as text
        let input = "{start_of_abc}\nX:1\n{end_of_abc}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("delegates.abc2svg=false")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(html.contains("<section class=\"abc\">"));
        assert!(html.contains("ABC"));
        assert!(html.contains("</section>"));
    }

    #[test]
    fn test_abc_section_null_config_auto_detect_disabled() {
        // Default config has delegates.abc2svg=null (auto-detect).
        // When abc2svg is not installed, sections render as plain text.
        if chordsketch_core::external_tool::has_abc2svg() {
            return; // Skip on machines with abc2svg installed
        }
        let input = "{start_of_abc}\nX:1\n{end_of_abc}";
        let song = chordsketch_core::parse(input).unwrap();
        // Use defaults — delegates.abc2svg is null (auto-detect)
        let config = chordsketch_core::config::Config::defaults();
        assert!(
            config.get_path("delegates.abc2svg").is_null(),
            "default config should have null delegates.abc2svg"
        );
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(
            html.contains("<section class=\"abc\">"),
            "null auto-detect with no abc2svg should render as text section"
        );
    }

    #[test]
    fn test_abc_section_fallback_preformatted() {
        // With delegate enabled but abc2svg not available, falls back to <pre>
        if chordsketch_core::external_tool::has_abc2svg() {
            return; // Skip on machines with abc2svg installed
        }
        let input = "{start_of_abc}\nX:1\nT:Test\nK:C\n{end_of_abc}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("delegates.abc2svg=true")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(html.contains("<section class=\"abc\">"));
        assert!(html.contains("<pre>"));
        assert!(html.contains("X:1"));
        assert!(html.contains("</pre>"));
    }

    #[test]
    fn test_abc_section_with_label_delegate_fallback() {
        if chordsketch_core::external_tool::has_abc2svg() {
            return;
        }
        let input = "{start_of_abc: Melody}\nX:1\n{end_of_abc}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("delegates.abc2svg=true")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(html.contains("ABC: Melody"));
        assert!(html.contains("<pre>"));
    }

    #[test]
    #[ignore]
    fn test_abc_section_renders_svg_with_abc2svg() {
        // Requires abc2svg installed. Run with: cargo test -- --ignored
        let input = "{start_of_abc}\nX:1\nT:Test\nM:4/4\nK:C\nCDEF|GABc|\n{end_of_abc}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("delegates.abc2svg=true")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(html.contains("<section class=\"abc\">"));
        assert!(
            html.contains("<svg"),
            "should contain rendered SVG from abc2svg"
        );
        assert!(html.contains("</section>"));
    }

    #[test]
    fn test_abc_section_auto_detect_default_config() {
        // Default config has delegates.abc2svg=null (auto-detect).
        // When the tool is not found, auto-detect resolves to false and the
        // section renders with raw content as regular text (no SVG, no <pre>).
        let input = "{start_of_abc}\nX:1\nT:Test\nK:C\n{end_of_abc}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(
            html.contains("<section class=\"abc\">"),
            "auto-detect should produce abc section"
        );
        if !chordsketch_core::external_tool::has_abc2svg() {
            assert!(
                html.contains("X:1"),
                "raw ABC content should be present without tool"
            );
            assert!(
                !html.contains("<svg"),
                "no SVG should be generated without abc2svg"
            );
        }
    }

    // -- lilypond delegate rendering tests ----------------------------------------

    #[test]
    fn test_ly_section_auto_detect_default_config() {
        // Same as ABC: auto-detect renders a section regardless of tool availability.
        let input = "{start_of_ly}\n\\relative c' { c4 }\n{end_of_ly}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(
            html.contains("<section class=\"ly\">"),
            "auto-detect should produce ly section"
        );
        if !chordsketch_core::external_tool::has_lilypond() {
            assert!(
                html.contains("\\relative"),
                "raw Lilypond content should be present without tool"
            );
            assert!(
                !html.contains("<svg"),
                "no SVG should be generated without lilypond"
            );
        }
    }

    #[test]
    fn test_ly_section_disabled_by_config() {
        // With delegates.lilypond explicitly disabled, Ly renders as text
        let input = "{start_of_ly}\n\\relative c' { c4 }\n{end_of_ly}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("delegates.lilypond=false")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(html.contains("<section class=\"ly\">"));
        assert!(html.contains("Lilypond"));
        assert!(html.contains("</section>"));
    }

    #[test]
    fn test_ly_section_fallback_preformatted() {
        if chordsketch_core::external_tool::has_lilypond() {
            return;
        }
        let input = "{start_of_ly}\n\\relative c' { c4 }\n{end_of_ly}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("delegates.lilypond=true")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(html.contains("<section class=\"ly\">"));
        assert!(html.contains("<pre>"));
        assert!(html.contains("</pre>"));
    }

    #[test]
    #[ignore]
    fn test_ly_section_renders_svg_with_lilypond() {
        // Requires lilypond installed. Run with: cargo test -- --ignored
        let input = "{start_of_ly}\n\\relative c' { c4 d e f | g2 g | }\n{end_of_ly}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("delegates.lilypond=true")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(html.contains("<section class=\"ly\">"));
        assert!(
            html.contains("<svg"),
            "should contain rendered SVG from lilypond"
        );
        assert!(html.contains("</section>"));
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

    // -- MusicXML delegate rendering tests ----------------------------------

    #[test]
    fn test_render_musicxml_section_disabled() {
        // With delegates.musescore explicitly disabled, MusicXML renders as text.
        let input = "{start_of_musicxml}\n<score-partwise/>\n{end_of_musicxml}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("delegates.musescore=false")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(
            html.contains("<section class=\"musicxml\">"),
            "fallback section should render when musescore is disabled: {html}"
        );
        assert!(html.contains("MusicXML"), "section label should appear");
        assert!(html.contains("</section>"), "section should be closed");
    }

    #[test]
    fn test_render_musicxml_section_no_musescore_installed() {
        // Default config has delegates.musescore=null (auto-detect).
        // When musescore is not installed, sections render as plain text.
        if chordsketch_core::external_tool::has_musescore() {
            return; // Skip on machines with musescore installed
        }

        let input = "{start_of_musicxml}\n<score-partwise/>\n{end_of_musicxml}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults();
        assert!(
            config.get_path("delegates.musescore").is_null(),
            "default config should have null delegates.musescore"
        );
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(
            html.contains("<section class=\"musicxml\">"),
            "null auto-detect with no musescore should render as text section"
        );
    }

    #[test]
    fn test_render_musicxml_section_with_label() {
        let input = "{start_of_musicxml: Score}\n<score-partwise/>\n{end_of_musicxml}";
        let song = chordsketch_core::parse(input).unwrap();
        let config = chordsketch_core::config::Config::defaults()
            .with_define("delegates.musescore=false")
            .unwrap();
        let html = render_song_with_transpose(&song, 0, &config);
        assert!(
            html.contains("Score"),
            "label should appear in section header"
        );
    }

    #[test]
    fn test_abc_fallback_sanitizes_would_be_script_in_svg() {
        // Even though abc2svg is not installed, verify the sanitization path
        // by directly calling the helper with a mocked SVG containing a
        // script tag.  The sanitize_svg_content call must strip it.
        let malicious_svg = "<svg><script>alert(1)</script><circle r=\"5\"/></svg>";
        let sanitized = sanitize_svg_content(malicious_svg);
        assert!(
            !sanitized.contains("<script>"),
            "script tags must be stripped from delegate SVG output"
        );
        assert!(sanitized.contains("<circle"));
    }

    #[test]
    fn test_sanitize_svg_strips_event_handlers_from_delegate_output() {
        let svg_with_handler = "<svg><rect onmouseover=\"alert(1)\" width=\"10\"/></svg>";
        let sanitized = sanitize_svg_content(svg_with_handler);
        assert!(
            !sanitized.contains("onmouseover"),
            "event handlers must be stripped from delegate SVG output"
        );
        assert!(sanitized.contains("<rect"));
    }

    #[test]
    fn test_sanitize_svg_strips_foreignobject_from_delegate_output() {
        let svg = "<svg><foreignObject><body xmlns=\"http://www.w3.org/1999/xhtml\"><script>alert(1)</script></body></foreignObject></svg>";
        let sanitized = sanitize_svg_content(svg);
        assert!(
            !sanitized.contains("<foreignObject"),
            "foreignObject must be stripped from delegate SVG output"
        );
    }

    #[test]
    fn test_sanitize_svg_strips_math_element() {
        let svg = "<svg><math><mi>x</mi></math></svg>";
        let sanitized = sanitize_svg_content(svg);
        assert!(
            !sanitized.contains("<math"),
            "math element must be stripped from delegate SVG output"
        );
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
    fn test_svg_section_strips_script_tags() {
        let input = "{start_of_svg}\n<svg><script>alert('xss')</script><circle r=\"10\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(!html.contains("<script>"), "script tags must be stripped");
        assert!(!html.contains("alert"), "script content must be stripped");
        assert!(
            html.contains("<circle r=\"10\"/>"),
            "safe SVG content must be preserved"
        );
    }

    #[test]
    fn test_svg_section_strips_event_handlers() {
        let input = "{start_of_svg}\n<svg onload=\"alert(1)\"><rect width=\"10\" onerror=\"hack()\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(!html.contains("onload"), "onload handler must be stripped");
        assert!(
            !html.contains("onerror"),
            "onerror handler must be stripped"
        );
        assert!(
            html.contains("width=\"10\""),
            "safe attributes must be preserved"
        );
    }

    #[test]
    fn test_svg_section_preserves_safe_content() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200"><text x="10" y="20">Hello</text></svg>"#;
        let input = format!("{{start_of_svg}}\n{svg}\n{{end_of_svg}}");
        let html = render(&input);
        assert!(html.contains("xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(html.contains("<text x=\"10\" y=\"20\">Hello</text>"));
    }

    #[test]
    fn test_svg_section_strips_case_insensitive_script() {
        let input = "{start_of_svg}\n<SCRIPT>alert(1)</SCRIPT><svg/>\n{end_of_svg}";
        let html = render(input);
        assert!(!html.contains("SCRIPT"), "case-insensitive script removal");
        assert!(!html.contains("alert"));
        assert!(html.contains("<svg/>"));
    }

    #[test]
    fn test_svg_section_strips_foreignobject() {
        let input = "{start_of_svg}\n<svg><foreignObject><body onload=\"alert(1)\"></body></foreignObject><rect width=\"10\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("foreignObject"),
            "foreignObject must be stripped"
        );
        assert!(
            !html.contains("foreignobject"),
            "foreignObject (lowercase) must be stripped"
        );
        assert!(
            html.contains("<rect width=\"10\"/>"),
            "safe content must be preserved"
        );
    }

    #[test]
    fn test_svg_section_strips_iframe() {
        let input = "{start_of_svg}\n<svg><iframe src=\"javascript:alert(1)\"></iframe><circle r=\"5\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(!html.contains("iframe"), "iframe must be stripped");
        assert!(html.contains("<circle r=\"5\"/>"));
    }

    #[test]
    fn test_svg_section_strips_object_and_embed() {
        let input = "{start_of_svg}\n<svg><object data=\"evil.swf\"></object><embed src=\"evil.swf\"></embed><rect/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(!html.contains("object"), "object must be stripped");
        assert!(!html.contains("embed"), "embed must be stripped");
        assert!(html.contains("<rect/>"));
    }

    #[test]
    fn test_svg_section_strips_javascript_uri_in_href() {
        let input = "{start_of_svg}\n<svg><a href=\"javascript:alert(1)\"><text>Click</text></a></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("javascript:"),
            "javascript: URI must be stripped from href"
        );
        assert!(html.contains("<text>Click</text>"));
    }

    #[test]
    fn test_svg_section_strips_vbscript_uri() {
        let input = "{start_of_svg}\n<svg><a href=\"vbscript:MsgBox\"><text>Click</text></a></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("vbscript:"),
            "vbscript: URI must be stripped"
        );
    }

    #[test]
    fn test_svg_section_strips_data_uri_in_use() {
        let input = "{start_of_svg}\n<svg><use href=\"data:image/svg+xml;base64,PHN2Zy8+\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("data:"),
            "data: URI must be stripped from use href"
        );
    }

    #[test]
    fn test_svg_section_strips_javascript_uri_case_insensitive() {
        let input = "{start_of_svg}\n<svg><a href=\"JaVaScRiPt:alert(1)\"><text>X</text></a></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.to_lowercase().contains("javascript:"),
            "case-insensitive javascript: URI must be stripped"
        );
    }

    #[test]
    fn test_svg_section_strips_xlink_href_dangerous_uri() {
        let input =
            "{start_of_svg}\n<svg><use xlink:href=\"javascript:alert(1)\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("javascript:"),
            "javascript: URI in xlink:href must be stripped"
        );
    }

    #[test]
    fn test_svg_section_preserves_safe_href() {
        let input = "{start_of_svg}\n<svg><a href=\"https://example.com\"><text>Link</text></a></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            html.contains("href=\"https://example.com\""),
            "safe https: href must be preserved"
        );
    }

    #[test]
    fn test_svg_section_preserves_fragment_href() {
        let input = "{start_of_svg}\n<svg><use href=\"#myShape\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            html.contains("href=\"#myShape\""),
            "fragment-only href must be preserved"
        );
    }

    #[test]
    fn test_svg_section_strips_use_external_https() {
        // Per #1828, <use href="https://..."> is a tracker/exfiltration
        // vector even over safe schemes (referer leakage, cross-origin
        // tracking pixel). Only fragment-only references ^# are allowed.
        let input = "{start_of_svg}\n<svg><use href=\"https://attacker.example.com/x.svg#sym\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("attacker.example.com"),
            "external https: URI in <use href> must be stripped; got: {html}"
        );
    }

    #[test]
    fn test_svg_section_strips_use_external_xlink_href() {
        // Same policy for the legacy xlink:href attribute.
        let input = "{start_of_svg}\n<svg><use xlink:href=\"https://tracker.example/pixel.svg\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("tracker.example"),
            "external https: URI in <use xlink:href> must be stripped; got: {html}"
        );
    }

    #[test]
    fn test_svg_section_preserves_fragment_xlink_href() {
        let input = "{start_of_svg}\n<svg><use xlink:href=\"#mySymbol\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            html.contains("xlink:href=\"#mySymbol\""),
            "fragment-only xlink:href must be preserved"
        );
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
        let songs = chordsketch_core::parse_multi("{title: Only}").unwrap();
        let html = render_songs(&songs);
        // Single song: should be identical to render_song
        assert_eq!(html, render_song(&songs[0]));
    }

    #[test]
    fn test_render_songs_two_songs_with_hr_separator() {
        let songs = chordsketch_core::parse_multi(
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
            chordsketch_core::parse_multi("{title: S1}\n[C]Do\n{new_song}\n{title: S2}\n[G]Re")
                .unwrap();
        let html = render_songs_with_transpose(&songs, 2, &Config::defaults());
        // C+2=D, G+2=A
        assert!(html.contains(">D<"));
        assert!(html.contains(">A<"));
    }

    // --- SVG animation XSS prevention (#572) ---

    #[test]
    fn test_sanitize_svg_strips_set_element() {
        let svg = r##"<svg><a href="#"><set attributeName="href" to="javascript:alert(1)"/><text>Click</text></a></svg>"##;
        let sanitized = sanitize_svg_content(svg);
        assert!(
            !sanitized.contains("<set"),
            "set element must be stripped to prevent SVG animation XSS"
        );
        assert!(sanitized.contains("<text>Click</text>"));
    }

    #[test]
    fn test_sanitize_svg_strips_animate_element() {
        let svg =
            r#"<svg><animate attributeName="href" values="javascript:alert(1)"/><rect/></svg>"#;
        let sanitized = sanitize_svg_content(svg);
        assert!(
            !sanitized.contains("<animate"),
            "animate element must be stripped"
        );
        assert!(sanitized.contains("<rect/>"));
    }

    #[test]
    fn test_sanitize_svg_strips_animatetransform() {
        let svg =
            "<svg><animateTransform attributeName=\"transform\" type=\"rotate\"/><rect/></svg>";
        let sanitized = sanitize_svg_content(svg);
        assert!(
            !sanitized.contains("animateTransform"),
            "animateTransform must be stripped"
        );
        assert!(
            !sanitized.contains("animatetransform"),
            "animatetransform (lowercase) must be stripped"
        );
    }

    #[test]
    fn test_sanitize_svg_strips_animatemotion() {
        let svg = "<svg><animateMotion path=\"M0,0 L100,100\"/><rect/></svg>";
        let sanitized = sanitize_svg_content(svg);
        assert!(
            !sanitized.contains("animateMotion"),
            "animateMotion must be stripped"
        );
    }

    #[test]
    fn test_sanitize_svg_strips_to_attr_with_dangerous_uri() {
        let svg = r#"<svg><a to="javascript:alert(1)"><text>X</text></a></svg>"#;
        let sanitized = sanitize_svg_content(svg);
        assert!(
            !sanitized.contains("javascript:"),
            "dangerous URI in 'to' attr must be stripped"
        );
    }

    #[test]
    fn test_sanitize_svg_strips_values_attr_with_dangerous_uri() {
        let svg = r#"<svg><a values="javascript:alert(1)"><text>X</text></a></svg>"#;
        let sanitized = sanitize_svg_content(svg);
        assert!(
            !sanitized.contains("javascript:"),
            "dangerous URI in 'values' attr must be stripped"
        );
    }

    // --- UTF-8 preservation in strip_dangerous_attrs (#578) ---

    #[test]
    fn test_strip_dangerous_attrs_preserves_cjk_text() {
        let input = "<svg><text x=\"10\">日本語テスト</text></svg>";
        let result = strip_dangerous_attrs(input);
        assert!(
            result.contains("日本語テスト"),
            "CJK characters must not be corrupted"
        );
    }

    #[test]
    fn test_strip_dangerous_attrs_preserves_emoji() {
        let input = "<svg><text>🎵🎸🎹</text></svg>";
        let result = strip_dangerous_attrs(input);
        assert!(result.contains("🎵🎸🎹"), "emoji must not be corrupted");
    }

    #[test]
    fn test_strip_dangerous_attrs_preserves_accented_chars() {
        let input = "<svg><text>café résumé naïve</text></svg>";
        let result = strip_dangerous_attrs(input);
        assert!(
            result.contains("café résumé naïve"),
            "accented characters must not be corrupted"
        );
    }

    #[test]
    fn test_sanitize_svg_full_roundtrip_with_non_ascii() {
        let input = "<svg><text x=\"10\">コード譜 🎵</text><rect width=\"100\"/></svg>";
        let sanitized = sanitize_svg_content(input);
        assert!(sanitized.contains("コード譜 🎵"));
        assert!(sanitized.contains("<rect width=\"100\"/>"));
    }

    #[test]
    fn test_sanitize_svg_self_closing_with_gt_in_attr_value() {
        // The `>` inside the attribute value should not confuse self-closing detection.
        let svg = r#"<svg><set to="a>b"/><text>safe</text></svg>"#;
        let sanitized = sanitize_svg_content(svg);
        assert!(
            !sanitized.contains("<set"),
            "dangerous <set> element must be stripped"
        );
        assert!(
            sanitized.contains("<text>safe</text>"),
            "content after stripped self-closing element must be preserved"
        );
    }

    // --- Quote-aware tag boundary scan (#646) ---

    #[test]
    fn test_strip_dangerous_attrs_gt_in_double_quoted_attr() {
        // `>` inside title=">" should not split the tag.
        let input = r#"<rect title=">" onload="alert(1)"/>"#;
        let result = strip_dangerous_attrs(input);
        assert!(
            !result.contains("onload"),
            "onload after quoted > must be stripped"
        );
        assert!(result.contains("title"));
    }

    #[test]
    fn test_strip_dangerous_attrs_gt_in_single_quoted_attr() {
        let input = "<rect title='>' onload=\"alert(1)\"/>";
        let result = strip_dangerous_attrs(input);
        assert!(
            !result.contains("onload"),
            "onload after single-quoted > must be stripped"
        );
    }

    // --- URI scheme with embedded whitespace/control chars (#655) ---

    #[test]
    fn test_dangerous_uri_scheme_with_embedded_tab() {
        assert!(has_dangerous_uri_scheme("java\tscript:alert(1)"));
    }

    #[test]
    fn test_dangerous_uri_scheme_with_embedded_newline() {
        assert!(has_dangerous_uri_scheme("java\nscript:alert(1)"));
    }

    #[test]
    fn test_dangerous_uri_scheme_with_control_chars() {
        assert!(has_dangerous_uri_scheme("java\x00script:alert(1)"));
    }

    #[test]
    fn test_safe_uri_not_flagged() {
        assert!(!has_dangerous_uri_scheme("https://example.com"));
    }

    #[test]
    fn test_dangerous_uri_scheme_with_many_embedded_whitespace() {
        // 1 tab between each letter: colon at raw position 20, within the 30-char window.
        // Both old and new code detect this; kept as a basic obfuscation smoke-test.
        let payload = "j\ta\tv\ta\ts\tc\tr\ti\tp\tt\t:\ta\tl\te\tr\tt\t(\t1\t)\t";
        assert!(
            has_dangerous_uri_scheme(payload),
            "1 tab between letters should not bypass javascript: detection"
        );
    }

    #[test]
    fn test_dangerous_uri_scheme_whitespace_bypass_regression() {
        // 3 tabs between each letter pushes the colon to raw position 40, past the
        // 30-char cap. The old `.take(30).filter(...)` ordering cut off the colon and
        // missed the match. Filter-first (`.filter(...).take(30)`) fixes this.
        // This test FAILS with the old ordering and PASSES with the fix.
        let payload = "j\t\t\ta\t\t\tv\t\t\ta\t\t\ts\t\t\tc\t\t\tr\t\t\ti\t\t\tp\t\t\tt\t\t\t:";
        assert!(
            has_dangerous_uri_scheme(payload),
            "3 tabs between letters (colon at raw position 40) must still be detected"
        );
    }

    // --- Multi-line tag splitting XSS prevention (#711) ---

    #[test]
    fn test_svg_section_blocks_multiline_script_tag_splitting() {
        // Splitting <script> across two lines must NOT bypass the sanitizer.
        let input = "{start_of_svg}\n<script\n>alert(1)</script>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("alert(1)"),
            "multi-line <script> tag splitting must not execute JS"
        );
        assert!(
            !html.to_lowercase().contains("<script"),
            "multi-line <script> tag must be stripped"
        );
    }

    #[test]
    fn test_svg_section_blocks_multiline_iframe_tag_splitting() {
        let input =
            "{start_of_svg}\n<iframe\nsrc=\"javascript:alert(1)\">\n</iframe>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.to_lowercase().contains("<iframe"),
            "multi-line <iframe> tag splitting must be stripped"
        );
        assert!(
            !html.contains("javascript:"),
            "javascript: URI in split iframe must be stripped"
        );
    }

    #[test]
    fn test_svg_section_blocks_multiline_foreignobject_splitting() {
        let input = "{start_of_svg}\n<foreignObject\n><script>alert(1)</script></foreignObject>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.to_lowercase().contains("<foreignobject"),
            "multi-line <foreignObject> splitting must be stripped"
        );
    }

    // --- file: and blob: URI scheme blocking (#1538) ---

    #[test]
    fn test_dangerous_uri_file_scheme_blocked() {
        // file: URI in href must be blocked — parity with is_safe_image_src
        assert!(
            has_dangerous_uri_scheme("file:///etc/passwd"),
            "file: URI scheme must be detected as dangerous"
        );
        assert!(
            has_dangerous_uri_scheme("FILE:///etc/passwd"),
            "FILE: (uppercase) must be detected as dangerous"
        );
    }

    #[test]
    fn test_dangerous_uri_blob_scheme_blocked() {
        assert!(
            has_dangerous_uri_scheme("blob:https://example.com/uuid"),
            "blob: URI scheme must be detected as dangerous"
        );
        assert!(
            has_dangerous_uri_scheme("BLOB:https://example.com/uuid"),
            "BLOB: (uppercase) must be detected as dangerous"
        );
    }

    #[test]
    fn test_svg_section_strips_file_uri_in_use_href() {
        // <use href="file:///etc/passwd"/> must have the href stripped
        let input = "{start_of_svg}\n<svg><use href=\"file:///etc/passwd\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("file:///"),
            "file: URI in <use href> must be stripped; got: {html}"
        );
    }

    #[test]
    fn test_svg_section_strips_file_uri_in_xlink_href() {
        let input =
            "{start_of_svg}\n<svg><use xlink:href=\"file:///etc/passwd\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("file:///"),
            "file: URI in xlink:href must be stripped; got: {html}"
        );
    }

    // --- feImage tag blocking (#1545) ---

    #[test]
    fn test_svg_section_strips_feimage_element() {
        // <feImage href="file:///etc/passwd"/> — SVG filter primitive loading external content
        let input =
            "{start_of_svg}\n<svg><feImage href=\"file:///etc/passwd\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.to_lowercase().contains("<feimage"),
            "feImage element must be stripped entirely; got: {html}"
        );
        assert!(
            !html.contains("file:///"),
            "file: URI inside feImage must not appear in output; got: {html}"
        );
    }

    #[test]
    fn test_svg_section_strips_feimage_with_http_href() {
        // feImage is dangerous regardless of URI scheme because it loads external SVG content
        let input = "{start_of_svg}\n<svg><feImage href=\"https://evil.example.com/spy.svg\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.to_lowercase().contains("<feimage"),
            "feImage element must be stripped even with http href; got: {html}"
        );
    }

    // --- Extended URI attribute list (#1545) ---

    #[test]
    fn test_svg_section_strips_action_javascript_uri() {
        // action attribute carrying javascript: URI must be stripped
        let input =
            "{start_of_svg}\n<svg><a action=\"javascript:alert(1)\">click</a></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("javascript:"),
            "javascript: URI in action attribute must be stripped; got: {html}"
        );
    }

    #[test]
    fn test_svg_section_strips_formaction_javascript_uri() {
        let input = "{start_of_svg}\n<svg><a formaction=\"javascript:alert(1)\">click</a></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("javascript:"),
            "javascript: URI in formaction attribute must be stripped; got: {html}"
        );
    }

    #[test]
    fn test_svg_section_strips_ping_javascript_uri() {
        // ping attribute sends POST requests on link click
        let input =
            "{start_of_svg}\n<svg><a ping=\"javascript:alert(1)\">click</a></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("javascript:"),
            "javascript: URI in ping attribute must be stripped; got: {html}"
        );
    }

    #[test]
    fn test_svg_section_strips_poster_file_uri() {
        // poster attribute on video — blocked via file: URI scheme
        let input =
            "{start_of_svg}\n<svg><video poster=\"file:///etc/passwd\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("file:///"),
            "file: URI in poster attribute must be stripped; got: {html}"
        );
    }

    #[test]
    fn test_svg_section_strips_background_file_uri() {
        // background attribute (legacy HTML body attribute)
        let input =
            "{start_of_svg}\n<svg><body background=\"file:///etc/passwd\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.contains("file:///"),
            "file: URI in background attribute must be stripped; got: {html}"
        );
    }

    // --- mhtml: URI scheme blocking (parity with is_safe_image_src) ---

    #[test]
    fn test_dangerous_uri_mhtml_scheme_blocked() {
        // mhtml: is an IE-era MIME HTML scheme; blocked by is_safe_image_src via allowlist.
        assert!(
            has_dangerous_uri_scheme("mhtml:file://C:/page.mhtml"),
            "mhtml: URI scheme must be detected as dangerous"
        );
        assert!(
            has_dangerous_uri_scheme("MHTML:file://C:/page.mhtml"),
            "MHTML: (uppercase) must be detected as dangerous"
        );
    }

    // --- SVG <image> element stripping ---

    #[test]
    fn test_svg_section_strips_image_element() {
        // SVG <image> can load external raster/vector content and is not needed
        // in music notation SVG.
        let input =
            "{start_of_svg}\n<svg><image href=\"https://evil.com/spy.png\"/></svg>\n{end_of_svg}";
        let html = render(input);
        assert!(
            !html.to_lowercase().contains("<image"),
            "SVG <image> element must be stripped entirely; got: {html}"
        );
    }

    // --- Font size clamping (renderer parity with PDF) ---

    #[test]
    fn test_extreme_textsize_is_clamped_to_max() {
        // Font size must be clamped to MAX_FONT_SIZE (200), not 99999.
        // Matches the equivalent test in the PDF renderer.
        let input = "{title: T}\n{textsize: 99999}\n[C]Hello";
        let html = render(input);
        assert!(
            !html.contains("99999"),
            "extreme textsize should be clamped, not passed through"
        );
        assert!(
            html.contains("200"),
            "extreme textsize should be clamped to MAX_FONT_SIZE (200)"
        );
    }

    #[test]
    fn test_negative_textsize_is_clamped_to_min() {
        // Negative size must be clamped to MIN_FONT_SIZE (0.5).
        // Matches the equivalent test in the PDF renderer.
        let input = "{title: T}\n{textsize: -10}\n[C]Hello";
        let html = render(input);
        assert!(
            html.contains("0.5"),
            "negative textsize should be clamped to MIN_FONT_SIZE (0.5)"
        );
    }

    #[test]
    fn test_extreme_chordsize_is_clamped_to_max() {
        let input = "{title: T}\n{chordsize: 50000}\n[C]Hello";
        let html = render(input);
        assert!(
            !html.contains("50000"),
            "extreme chordsize should be clamped"
        );
        assert!(
            html.contains("200"),
            "extreme chordsize should be clamped to MAX_FONT_SIZE (200)"
        );
    }
}
