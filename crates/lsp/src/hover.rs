//! LSP hover support for ChordPro files.
//!
//! `textDocument/hover` returns a Markdown popup for two kinds of targets:
//!
//! - **Chord name** (`[Am]`): ASCII diagram from the built-in voicing DB,
//!   rendered for guitar (6-string) by default.
//! - **Directive name** (`{start_of_chorus}`): syntax and a brief description.
//!
//! Context detection is cursor-position-aware: it scans the line to find the
//! innermost open delimiter and determines whether the cursor sits on a chord
//! name or a directive name.

use chordsketch_core::chord_diagram::render_ascii;
use chordsketch_core::voicings::guitar_voicing;

// ---------------------------------------------------------------------------
// Hover context detection
// ---------------------------------------------------------------------------

/// The hover target at a given cursor position.
#[derive(Debug, PartialEq)]
pub enum HoverContext {
    /// Cursor is inside `[...]` — hover over a chord name.
    ChordName {
        /// Full chord name text inside the brackets.
        name: String,
    },
    /// Cursor is inside `{…}` at the directive-name segment (before `:` or `}`).
    DirectiveName {
        /// Directive name, lowercased.
        name: String,
    },
    /// Not inside a recognized hover context.
    None,
}

/// Detects the hover context for `line` at 0-based character column `col`.
///
/// Scans the line to find the innermost open bracket or brace that contains
/// `col`, then extracts the relevant token (chord name or directive name).
/// Returns [`HoverContext::None`] when the cursor is outside any recognized
/// region.
#[must_use]
pub fn detect_hover_context(line: &str, col: usize) -> HoverContext {
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len());

    // Scan left from `col` to find the nearest unmatched open delimiter.
    // We also need to know where the token ends, so we'll scan right too.

    // First pass: scan left to find the context.
    let mut bracket_start: Option<usize> = None; // index of the `[`
    let mut brace_start: Option<usize> = None; // index of the `{`

    // Scan left from col (inclusive) to find which delimiter wraps us.
    let scan_to = col;
    let mut depth_bracket = 0i32;
    let mut depth_brace = 0i32;

    for i in (0..scan_to).rev() {
        match chars[i] {
            ']' => depth_bracket += 1,
            '[' => {
                if depth_bracket == 0 {
                    bracket_start = Some(i);
                    break;
                }
                depth_bracket -= 1;
            }
            '}' => depth_brace += 1,
            '{' => {
                if depth_brace == 0 {
                    brace_start = Some(i);
                    break;
                }
                depth_brace -= 1;
            }
            _ => {}
        }
    }

    if let Some(bstart) = brace_start {
        // Find the colon within the brace (between bstart+1 and col).
        let colon_pos = chars[bstart + 1..col]
            .iter()
            .position(|&c| c == ':')
            .map(|p| bstart + 1 + p);

        if colon_pos.is_some() {
            // Cursor is past the colon — not in the directive name segment.
            return HoverContext::None;
        }

        // Extract directive name: text from bstart+1 to the first `:` or `}` or col.
        let name_end = chars[bstart + 1..]
            .iter()
            .position(|&c| c == ':' || c == '}')
            .map(|p| bstart + 1 + p)
            .unwrap_or(chars.len());

        // Only hover if the cursor is actually within the directive name span.
        if col > name_end {
            return HoverContext::None;
        }

        let name: String = chars[bstart + 1..name_end]
            .iter()
            .collect::<String>()
            .trim()
            .to_ascii_lowercase();

        if name.is_empty() {
            return HoverContext::None;
        }
        return HoverContext::DirectiveName { name };
    }

    if let Some(bstart) = bracket_start {
        // Find the matching `]` (if any) to the right of bstart.
        let close = chars[bstart + 1..]
            .iter()
            .position(|&c| c == ']')
            .map(|p| bstart + 1 + p);

        // Cursor must be between `[` (exclusive) and `]` (exclusive).
        // col >= end means the cursor sits on or after the closing `]`.
        let end = close.unwrap_or(chars.len());
        if col >= end {
            return HoverContext::None;
        }

        let name: String = chars[bstart + 1..end].iter().collect();
        let name = name.trim().to_string();
        if name.is_empty() {
            return HoverContext::None;
        }
        return HoverContext::ChordName { name };
    }

    HoverContext::None
}

// ---------------------------------------------------------------------------
// Hover content builders
// ---------------------------------------------------------------------------

/// Returns the character-offset span `[start, end)` of the hovered token in `line`.
///
/// This mirrors the logic of [`detect_hover_context`] but returns the token's
/// position so that callers can construct an LSP [`Range`] for the hover
/// response (causing editors to highlight the hovered symbol).
///
/// Returns `None` when there is no hover target at `col`.
///
/// [`Range`]: tower_lsp::lsp_types::Range
#[must_use]
pub fn hover_token_span(line: &str, col: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len());

    let scan_to = col;
    let mut bracket_start: Option<usize> = None;
    let mut brace_start: Option<usize> = None;
    let mut depth_bracket = 0i32;
    let mut depth_brace = 0i32;

    for i in (0..scan_to).rev() {
        match chars[i] {
            ']' => depth_bracket += 1,
            '[' => {
                if depth_bracket == 0 {
                    bracket_start = Some(i);
                    break;
                }
                depth_bracket -= 1;
            }
            '}' => depth_brace += 1,
            '{' => {
                if depth_brace == 0 {
                    brace_start = Some(i);
                    break;
                }
                depth_brace -= 1;
            }
            _ => {}
        }
    }

    if let Some(bstart) = brace_start {
        // If there's a colon between bstart+1 and col, cursor is in value → no span.
        let colon_pos = chars[bstart + 1..col]
            .iter()
            .position(|&c| c == ':')
            .map(|p| bstart + 1 + p);
        if colon_pos.is_some() {
            return None;
        }
        let name_end = chars[bstart + 1..]
            .iter()
            .position(|&c| c == ':' || c == '}')
            .map(|p| bstart + 1 + p)
            .unwrap_or(chars.len());
        if col > name_end {
            return None;
        }
        let name: String = chars[bstart + 1..name_end]
            .iter()
            .collect::<String>()
            .trim()
            .to_ascii_lowercase();
        if name.is_empty() {
            return None;
        }
        // Span covers the directive name characters (excluding braces and whitespace).
        let name_start = bstart
            + 1
            + chars[bstart + 1..name_end]
                .iter()
                .take_while(|&&c| c == ' ')
                .count();
        return Some((name_start, name_end));
    }

    if let Some(bstart) = bracket_start {
        let close = chars[bstart + 1..]
            .iter()
            .position(|&c| c == ']')
            .map(|p| bstart + 1 + p);
        let end = close.unwrap_or(chars.len());
        if col >= end {
            return None;
        }
        let name: String = chars[bstart + 1..end].iter().collect();
        if name.trim().is_empty() {
            return None;
        }
        // Span covers the chord name characters (excluding the `[` and `]`).
        Some((bstart + 1, end))
    } else {
        None
    }
}

/// Build Markdown hover content for a chord name.
///
/// Looks up the chord in the built-in guitar voicing database and renders an
/// ASCII diagram inside a fenced code block. Returns `None` if no voicing is
/// available for the chord.
#[must_use]
pub fn chord_hover_markdown(chord_name: &str) -> Option<String> {
    let diagram = guitar_voicing(chord_name)?;
    let ascii = render_ascii(&diagram);
    Some(format!("**{chord_name}** (guitar)\n\n```\n{ascii}\n```"))
}

/// Static directive documentation: `(syntax, description)`.
///
/// Used to build hover content for directive names.
const DIRECTIVE_DOCS: &[(&str, &str, &str)] = &[
    // Metadata
    (
        "title",
        "{title: Text}",
        "Sets the song title. Shown in the header on the first page.",
    ),
    (
        "subtitle",
        "{subtitle: Text}",
        "Sets the subtitle (e.g. artist name). Shown below the title.",
    ),
    (
        "artist",
        "{artist: Name}",
        "Sets the artist / performer name.",
    ),
    ("composer", "{composer: Name}", "Sets the composer name."),
    ("lyricist", "{lyricist: Name}", "Sets the lyricist name."),
    ("album", "{album: Name}", "Sets the album name."),
    ("year", "{year: YYYY}", "Sets the publication year."),
    ("key", "{key: Key}", "Sets the song key (e.g. `Am`, `C`)."),
    (
        "tempo",
        "{tempo: BPM}",
        "Sets the tempo in beats per minute.",
    ),
    (
        "time",
        "{time: N/D}",
        "Sets the time signature (e.g. `4/4`, `3/4`).",
    ),
    (
        "capo",
        "{capo: N}",
        "Indicates the capo position (fret number).",
    ),
    (
        "sorttitle",
        "{sorttitle: Text}",
        "Sort key for the title (e.g. omitting \"The\").",
    ),
    (
        "sortartist",
        "{sortartist: Text}",
        "Sort key for the artist name.",
    ),
    ("arranger", "{arranger: Name}", "Sets the arranger name."),
    (
        "copyright",
        "{copyright: Text}",
        "Sets the copyright notice.",
    ),
    (
        "duration",
        "{duration: M:SS}",
        "Sets the song duration (e.g. `3:45`).",
    ),
    (
        "tag",
        "{tag: Label}",
        "Attaches an arbitrary tag label to the song.",
    ),
    // Transpose
    (
        "transpose",
        "{transpose: N}",
        "Transposes all chords by `N` semitones (can be negative).",
    ),
    // Song boundary
    (
        "new_song",
        "{new_song}",
        "Starts a new song in a multi-song file. Alias: `{ns}`.",
    ),
    // Comment / annotation
    (
        "comment",
        "{comment: Text}",
        "Displays a comment box in the rendered output. Alias: `{c}`.",
    ),
    (
        "comment_italic",
        "{comment_italic: Text}",
        "Displays an italicised comment. Alias: `{ci}`.",
    ),
    (
        "comment_box",
        "{comment_box: Text}",
        "Displays a boxed comment. Alias: `{cb}`.",
    ),
    // Environments
    (
        "start_of_chorus",
        "{start_of_chorus}",
        "Opens a chorus section. Alias: `{soc}`.",
    ),
    (
        "end_of_chorus",
        "{end_of_chorus}",
        "Closes a chorus section. Alias: `{eoc}`.",
    ),
    (
        "start_of_verse",
        "{start_of_verse}",
        "Opens a verse section. Alias: `{sov}`.",
    ),
    (
        "end_of_verse",
        "{end_of_verse}",
        "Closes a verse section. Alias: `{eov}`.",
    ),
    (
        "start_of_bridge",
        "{start_of_bridge}",
        "Opens a bridge section. Alias: `{sob}`.",
    ),
    (
        "end_of_bridge",
        "{end_of_bridge}",
        "Closes a bridge section. Alias: `{eob}`.",
    ),
    (
        "start_of_tab",
        "{start_of_tab}",
        "Opens a guitar tab section. Alias: `{sot}`.",
    ),
    (
        "end_of_tab",
        "{end_of_tab}",
        "Closes a guitar tab section. Alias: `{eot}`.",
    ),
    (
        "start_of_grid",
        "{start_of_grid}",
        "Opens a chord grid section. Alias: `{sog}`.",
    ),
    (
        "end_of_grid",
        "{end_of_grid}",
        "Closes a chord grid section. Alias: `{eog}`.",
    ),
    (
        "start_of_abc",
        "{start_of_abc}",
        "Opens an ABC notation delegate block.",
    ),
    (
        "end_of_abc",
        "{end_of_abc}",
        "Closes an ABC notation delegate block.",
    ),
    (
        "start_of_ly",
        "{start_of_ly}",
        "Opens a Lilypond notation delegate block.",
    ),
    (
        "end_of_ly",
        "{end_of_ly}",
        "Closes a Lilypond notation delegate block.",
    ),
    (
        "start_of_svg",
        "{start_of_svg}",
        "Opens a raw SVG delegate block.",
    ),
    (
        "end_of_svg",
        "{end_of_svg}",
        "Closes a raw SVG delegate block.",
    ),
    (
        "start_of_musicxml",
        "{start_of_musicxml}",
        "Opens a MusicXML delegate block (rendered via MuseScore).",
    ),
    (
        "end_of_musicxml",
        "{end_of_musicxml}",
        "Closes a MusicXML delegate block.",
    ),
    (
        "start_of_textblock",
        "{start_of_textblock}",
        "Opens a verbatim text block.",
    ),
    (
        "end_of_textblock",
        "{end_of_textblock}",
        "Closes a verbatim text block.",
    ),
    // Recall
    (
        "chorus",
        "{chorus}",
        "Recalls (repeats) the most recent chorus.",
    ),
    // Page control
    (
        "new_page",
        "{new_page}",
        "Forces a page break. Alias: `{np}`.",
    ),
    (
        "new_physical_page",
        "{new_physical_page}",
        "Forces a physical page break. Alias: `{npp}`.",
    ),
    (
        "column_break",
        "{column_break}",
        "Forces a column break. Alias: `{colb}`.",
    ),
    (
        "columns",
        "{columns: N}",
        "Sets the number of columns. Alias: `{col}`.",
    ),
    // Chord definitions
    (
        "define",
        "{define: Name frets …}",
        "Defines a custom chord diagram. Syntax: `{define: Am base-fret 1 frets x 0 2 2 1 0}`.",
    ),
    (
        "chord",
        "{chord: Name}",
        "Renders a chord diagram inline in the text.",
    ),
    (
        "diagrams",
        "{diagrams: on|off|guitar|ukulele}",
        "Controls chord diagram rendering.",
    ),
    // Generic metadata
    (
        "meta",
        "{meta: Key Value}",
        "Sets an arbitrary metadata key/value pair.",
    ),
    // Image
    (
        "image",
        "{image: path}",
        "Embeds an image. Accepts `src=`, `width=`, `height=`, `scale=`, `align=` attributes.",
    ),
    // Selectors
    (
        "ifdef",
        "{ifdef: KEY}",
        "Includes the following block only if KEY is defined.",
    ),
    (
        "ifndef",
        "{ifndef: KEY}",
        "Includes the following block only if KEY is not defined.",
    ),
    (
        "endif",
        "{endif}",
        "Closes an `{ifdef}` or `{ifndef}` block.",
    ),
    // Inline font / size / colour directives (Aliases: tf ts tc cf cs cc)
    (
        "textfont",
        "{textfont: Font}",
        "Sets the font for lyrics text. Alias: `{tf}`. Takes effect from this point forward.",
    ),
    (
        "textsize",
        "{textsize: N}",
        "Sets the point size for lyrics text. Alias: `{ts}`.",
    ),
    (
        "textcolour",
        "{textcolour: Colour}",
        "Sets the colour for lyrics text (CSS colour name or `#RRGGBB`). Alias: `{tc}`.",
    ),
    (
        "chordfont",
        "{chordfont: Font}",
        "Sets the font for chord names. Alias: `{cf}`.",
    ),
    (
        "chordsize",
        "{chordsize: N}",
        "Sets the point size for chord names. Alias: `{cs}`.",
    ),
    (
        "chordcolour",
        "{chordcolour: Colour}",
        "Sets the colour for chord names. Alias: `{cc}`.",
    ),
    (
        "tabfont",
        "{tabfont: Font}",
        "Sets the font for tab sections.",
    ),
    (
        "tabsize",
        "{tabsize: N}",
        "Sets the point size for tab sections.",
    ),
    (
        "tabcolour",
        "{tabcolour: Colour}",
        "Sets the colour for tab sections.",
    ),
    // Title-level font / size / colour directives
    (
        "titlefont",
        "{titlefont: Font}",
        "Sets the font for the song title.",
    ),
    (
        "titlesize",
        "{titlesize: N}",
        "Sets the point size for the song title.",
    ),
    (
        "titlecolour",
        "{titlecolour: Colour}",
        "Sets the colour for the song title.",
    ),
    (
        "chorusfont",
        "{chorusfont: Font}",
        "Sets the font for chorus section text.",
    ),
    (
        "chorussize",
        "{chorussize: N}",
        "Sets the point size for chorus section text.",
    ),
    (
        "choruscolour",
        "{choruscolour: Colour}",
        "Sets the colour for chorus section text.",
    ),
    (
        "footerfont",
        "{footerfont: Font}",
        "Sets the font for page footers.",
    ),
    (
        "footersize",
        "{footersize: N}",
        "Sets the point size for page footers.",
    ),
    (
        "footercolour",
        "{footercolour: Colour}",
        "Sets the colour for page footers.",
    ),
    (
        "headerfont",
        "{headerfont: Font}",
        "Sets the font for page headers.",
    ),
    (
        "headersize",
        "{headersize: N}",
        "Sets the point size for page headers.",
    ),
    (
        "headercolour",
        "{headercolour: Colour}",
        "Sets the colour for page headers.",
    ),
    (
        "labelfont",
        "{labelfont: Font}",
        "Sets the font for section labels.",
    ),
    (
        "labelsize",
        "{labelsize: N}",
        "Sets the point size for section labels.",
    ),
    (
        "labelcolour",
        "{labelcolour: Colour}",
        "Sets the colour for section labels.",
    ),
    (
        "gridfont",
        "{gridfont: Font}",
        "Sets the font for chord grid text.",
    ),
    (
        "gridsize",
        "{gridsize: N}",
        "Sets the point size for chord grid text.",
    ),
    (
        "gridcolour",
        "{gridcolour: Colour}",
        "Sets the colour for chord grid text.",
    ),
    (
        "tocfont",
        "{tocfont: Font}",
        "Sets the font for the table of contents.",
    ),
    (
        "tocsize",
        "{tocsize: N}",
        "Sets the point size for the table of contents.",
    ),
    (
        "toccolour",
        "{toccolour: Colour}",
        "Sets the colour for the table of contents.",
    ),
];

/// Build Markdown hover content for a directive name.
///
/// Returns `None` if the directive name is not recognized.
#[must_use]
pub fn directive_hover_markdown(directive_name: &str) -> Option<String> {
    let lower = directive_name.to_ascii_lowercase();
    // Resolve aliases to canonical names.
    let canonical = resolve_alias(&lower);
    DIRECTIVE_DOCS
        .iter()
        .find(|(name, _, _)| *name == canonical)
        .map(|(_, syntax, description)| format!("**`{syntax}`**\n\n{description}"))
}

/// Resolve short directive aliases to their canonical names.
fn resolve_alias(name: &str) -> &str {
    match name {
        "t" => "title",
        "st" => "subtitle",
        "ns" => "new_song",
        "c" => "comment",
        "ci" => "comment_italic",
        "cb" => "comment_box",
        "soc" => "start_of_chorus",
        "eoc" => "end_of_chorus",
        "sov" => "start_of_verse",
        "eov" => "end_of_verse",
        "sob" => "start_of_bridge",
        "eob" => "end_of_bridge",
        "sot" => "start_of_tab",
        "eot" => "end_of_tab",
        "sog" => "start_of_grid",
        "eog" => "end_of_grid",
        "np" => "new_page",
        "npp" => "new_physical_page",
        "colb" => "column_break",
        "col" => "columns",
        "tf" => "textfont",
        "ts" => "textsize",
        "tc" => "textcolour",
        "cf" => "chordfont",
        "cs" => "chordsize",
        "cc" => "chordcolour",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- detect_hover_context ------------------------------------------------

    #[test]
    fn hover_chord_cursor_inside_bracket() {
        // Line: [Am] — cursor at position 1 (the 'A')
        let ctx = detect_hover_context("[Am]", 1);
        assert_eq!(
            ctx,
            HoverContext::ChordName {
                name: "Am".to_string()
            }
        );
    }

    #[test]
    fn hover_chord_cursor_at_bracket_open() {
        // Cursor sits on the `[` itself — considered outside.
        let ctx = detect_hover_context("[Am]", 0);
        assert_eq!(ctx, HoverContext::None);
    }

    #[test]
    fn hover_chord_cursor_at_bracket_close() {
        // Cursor sits on the `]` — considered outside.
        let ctx = detect_hover_context("[Am]", 3);
        assert_eq!(ctx, HoverContext::None);
    }

    #[test]
    fn hover_chord_cursor_end_of_name() {
        // [Am] — cursor at position 2 (the 'm'), last char of chord name
        let ctx = detect_hover_context("[Am]", 2);
        assert_eq!(
            ctx,
            HoverContext::ChordName {
                name: "Am".to_string()
            }
        );
    }

    #[test]
    fn hover_directive_name() {
        // {title: My Song} — cursor at position 3 (inside "title")
        let ctx = detect_hover_context("{title: My Song}", 3);
        assert_eq!(
            ctx,
            HoverContext::DirectiveName {
                name: "title".to_string()
            }
        );
    }

    #[test]
    fn hover_directive_value_returns_none() {
        // Cursor is past the colon — no hover for the value part.
        let ctx = detect_hover_context("{title: My Song}", 10);
        assert_eq!(ctx, HoverContext::None);
    }

    #[test]
    fn hover_outside_any_delimiter_returns_none() {
        // Plain lyrics line — no hover target.
        let ctx = detect_hover_context("Hello world", 3);
        assert_eq!(ctx, HoverContext::None);
    }

    #[test]
    fn hover_chord_in_mixed_line() {
        // "[C]Hello [G7]world" — cursor at 10 (inside "G7")
        let ctx = detect_hover_context("[C]Hello [G7]world", 10);
        assert_eq!(
            ctx,
            HoverContext::ChordName {
                name: "G7".to_string()
            }
        );
    }

    // --- chord_hover_markdown ------------------------------------------------

    #[test]
    fn chord_hover_produces_markdown_for_known_chord() {
        let md = chord_hover_markdown("Am");
        assert!(md.is_some(), "Am should have a built-in voicing");
        let md = md.unwrap();
        assert!(md.contains("**Am**"));
        assert!(md.contains("```"));
    }

    #[test]
    fn chord_hover_returns_none_for_unknown_chord() {
        let md = chord_hover_markdown("Xq#dim13");
        assert!(md.is_none(), "unknown chord should return None");
    }

    // --- directive_hover_markdown --------------------------------------------

    #[test]
    fn directive_hover_produces_markdown_for_title() {
        let md = directive_hover_markdown("title");
        assert!(md.is_some(), "title should have hover docs");
        let md = md.unwrap();
        assert!(md.contains("{title:"));
    }

    #[test]
    fn directive_hover_resolves_alias_soc() {
        // `soc` is an alias for `start_of_chorus`
        let md = directive_hover_markdown("soc");
        assert!(
            md.is_some(),
            "alias soc should resolve to start_of_chorus docs"
        );
        let md = md.unwrap();
        assert!(md.contains("start_of_chorus"));
    }

    #[test]
    fn directive_hover_returns_none_for_unknown() {
        let md = directive_hover_markdown("notadirective");
        assert!(md.is_none());
    }

    #[test]
    fn directive_hover_case_insensitive() {
        let md = directive_hover_markdown("TITLE");
        assert!(md.is_some(), "should match regardless of case");
    }

    // --- font/colour directives (#1269) --------------------------------------

    #[test]
    fn directive_hover_textfont_canonical() {
        let md = directive_hover_markdown("textfont");
        assert!(md.is_some(), "textfont should have hover docs");
        let md = md.unwrap();
        assert!(md.contains("{textfont:"));
    }

    #[test]
    fn directive_hover_textfont_alias_tf() {
        // alias tf → textfont
        let md = directive_hover_markdown("tf");
        assert!(
            md.is_some(),
            "alias tf should resolve to textfont hover docs"
        );
        let md = md.unwrap();
        assert!(md.contains("textfont"));
    }

    #[test]
    fn directive_hover_chordfont_alias_cf() {
        let md = directive_hover_markdown("cf");
        assert!(md.is_some(), "alias cf should resolve to chordfont docs");
    }

    #[test]
    fn directive_hover_titlefont() {
        let md = directive_hover_markdown("titlefont");
        assert!(md.is_some(), "titlefont should have hover docs");
    }

    #[test]
    fn directive_hover_gridcolour() {
        let md = directive_hover_markdown("gridcolour");
        assert!(md.is_some(), "gridcolour should have hover docs");
    }

    // --- hover_token_span (#1270) --------------------------------------------

    #[test]
    fn token_span_chord_inside_bracket() {
        // [Am] — cursor at position 1 ('A')
        // span should be (1, 3) for "Am" (chars 1 and 2)
        let span = super::hover_token_span("[Am]", 1);
        assert_eq!(span, Some((1, 3)));
    }

    #[test]
    fn token_span_chord_at_bracket_open_returns_none() {
        let span = super::hover_token_span("[Am]", 0);
        assert_eq!(span, None);
    }

    #[test]
    fn token_span_chord_at_bracket_close_returns_none() {
        let span = super::hover_token_span("[Am]", 3);
        assert_eq!(span, None);
    }

    #[test]
    fn token_span_directive_name() {
        // {title: My Song} — cursor at 3
        // span should be (1, 6) for "title"
        let span = super::hover_token_span("{title: My Song}", 3);
        assert_eq!(span, Some((1, 6)));
    }

    #[test]
    fn token_span_directive_value_returns_none() {
        let span = super::hover_token_span("{title: My Song}", 10);
        assert_eq!(span, None);
    }

    #[test]
    fn token_span_no_context_returns_none() {
        let span = super::hover_token_span("Hello world", 3);
        assert_eq!(span, None);
    }
}
