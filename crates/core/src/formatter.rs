//! ChordPro source formatter.
//!
//! The [`format`] function normalizes ChordPro text to a canonical style:
//! directive names are expanded to their canonical form, spacing inside
//! directives is normalized, chord spelling is canonicalized, and blank lines
//! between sections are made consistent.
//!
//! # Usage
//!
//! ```
//! use chordsketch_core::formatter::{FormatOptions, format};
//!
//! let input = "{soc}\n[am]Hello\n{eoc}\n";
//! let output = format(input, &FormatOptions::default());
//! assert_eq!(output, "{start_of_chorus}\n[Am]Hello\n{end_of_chorus}\n");
//! ```

use crate::ast::DirectiveKind;
use crate::chord::parse_chord;

/// Options that control which normalizations [`format`] applies.
#[derive(Debug, Clone)]
pub struct FormatOptions {
    /// Expand directive name aliases to their canonical long form.
    ///
    /// Example: `{soc}` → `{start_of_chorus}`, `{t: My Song}` → `{title: My Song}`.
    ///
    /// Default: `true`.
    pub normalize_directive_names: bool,

    /// Normalize chord spelling: capitalize the root note.
    ///
    /// Example: `[am]` → `[Am]`, `[c#m7]` → `[C#m7]`.
    ///
    /// Default: `true`.
    pub normalize_chord_spelling: bool,

    /// Ensure exactly one blank line between section blocks
    /// (`{end_of_*}` … next non-blank line).
    ///
    /// Default: `true`.
    pub section_blank_lines: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            normalize_directive_names: true,
            normalize_chord_spelling: true,
            section_blank_lines: true,
        }
    }
}

/// Format a ChordPro source string.
///
/// Applies the normalizations described in [`FormatOptions`] and returns the
/// reformatted source. The output is always syntactically valid ChordPro.
///
/// # Idempotence
///
/// `format(format(s, opts), opts) == format(s, opts)` for any valid `s` and
/// `opts`.
///
/// # Line endings
///
/// All line endings in the input (`\r\n`, `\r`, `\n`) are normalized to `\n`.
/// The output always ends with a `\n` unless the input contained no non-blank
/// content (in which case the output is an empty string).
#[must_use]
pub fn format(input: &str, options: &FormatOptions) -> String {
    // Normalize line endings to LF first.
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");

    let mut out: Vec<String> = Vec::new();
    // Blank lines accumulated since the last emitted non-blank line.
    let mut pending_blanks: usize = 0;
    // Whether the last emitted non-blank line was a section-ending directive.
    let mut after_section_end = false;

    for raw_line in normalized.lines() {
        if raw_line.trim().is_empty() {
            pending_blanks += 1;
            continue;
        }

        // Format the non-blank line.
        let formatted = format_line(raw_line, options);
        let is_end = is_section_end_directive(&formatted);

        // Emit pending blank lines (collapsed to at most one), or inject a
        // mandatory blank line after a section-end boundary.
        if options.section_blank_lines && after_section_end {
            // Always exactly one blank line after a section end, even if the
            // original had none.
            out.push(String::new());
        } else if pending_blanks > 0 {
            // Collapse multiple consecutive blank lines to one.
            out.push(String::new());
        }
        pending_blanks = 0;

        out.push(formatted);
        after_section_end = is_end;
    }

    // Trailing blank lines are discarded (they were only accumulated, never
    // emitted because no subsequent non-blank line triggered their flush).

    if out.is_empty() {
        return String::new();
    }

    let mut result = out.join("\n");
    result.push('\n');
    result
}

/// Format a single non-blank ChordPro line.
fn format_line(line: &str, options: &FormatOptions) -> String {
    // Remove trailing whitespace only; preserve leading indentation (unusual
    // in ChordPro, but some editors add it).
    let trimmed = line.trim_end();

    // Comment lines are preserved verbatim.
    if trimmed.trim_start().starts_with('#') {
        return trimmed.to_string();
    }

    // Try to parse and reformat as a directive line.
    if let Some(formatted) = try_format_directive(trimmed, options) {
        return formatted;
    }

    // Lyrics / chords line: optionally normalize chord spellings.
    if options.normalize_chord_spelling {
        normalize_chords_in_line(trimmed)
    } else {
        trimmed.to_string()
    }
}

/// Try to parse and reformat a line as a ChordPro directive.
///
/// Returns `Some(formatted)` when the entire (trimmed) line is a single
/// `{…}` directive block, `None` otherwise.
fn try_format_directive(line: &str, options: &FormatOptions) -> Option<String> {
    // Must start with `{` and end with `}`.
    let inner = line.strip_prefix('{')?.strip_suffix('}')?;

    // Skip config-override directives (`{+config.key: value}`).
    if inner.starts_with('+') {
        return Some(line.to_string());
    }

    // Split at the first `:` to separate name from optional value.
    let (name_raw, value_opt) = match inner.find(':') {
        Some(pos) => (&inner[..pos], Some(&inner[pos + 1..])),
        None => (inner, None),
    };

    let name_trimmed = name_raw.trim();

    // Use the AST resolver to handle selector suffixes (e.g., `textfont-piano`).
    let (kind, selector) = DirectiveKind::resolve_with_selector(name_trimmed);

    let canonical_name = if options.normalize_directive_names {
        kind.full_canonical_name()
    } else {
        name_trimmed.to_string()
    };

    // Reconstruct the directive.
    //
    // When normalization is ON, `canonical_name` is the base name only
    // (e.g. `"textfont"`), so we must append the selector separately.
    // When normalization is OFF, `canonical_name` is `name_trimmed` which
    // already contains the selector (e.g. `"textfont-piano"`), so appending
    // it again would produce a doubled suffix like `"textfont-piano-piano"`.
    let mut result = String::from("{");
    result.push_str(&canonical_name);
    if options.normalize_directive_names {
        if let Some(sel) = &selector {
            result.push('-');
            result.push_str(sel);
        }
    }
    if let Some(value) = value_opt {
        let v = value.trim();
        result.push_str(": ");
        result.push_str(v);
    }
    result.push('}');
    Some(result)
}

/// Returns `true` if `line` is a section-closing directive.
///
/// Covers all `{end_of_*}` variants: chorus, verse, bridge, tab, grid,
/// ABC, Lilypond, SVG, textblock, and user-defined custom sections.
fn is_section_end_directive(line: &str) -> bool {
    let inner = match line.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        Some(s) => s,
        None => return false,
    };
    // Ignore selector and value; use only the name part.
    let name = inner.split(':').next().unwrap_or(inner).trim();
    let (kind, _) = DirectiveKind::resolve_with_selector(name);
    matches!(
        kind,
        DirectiveKind::EndOfChorus
            | DirectiveKind::EndOfVerse
            | DirectiveKind::EndOfBridge
            | DirectiveKind::EndOfTab
            | DirectiveKind::EndOfGrid
            | DirectiveKind::EndOfAbc
            | DirectiveKind::EndOfLy
            | DirectiveKind::EndOfSvg
            | DirectiveKind::EndOfTextblock
            | DirectiveKind::EndOfSection(_)
    )
}

/// Normalize chord spellings within a lyrics line.
///
/// Each `[…]` bracket group is treated as a chord name. If the chord can be
/// parsed, it is re-serialized from the structured representation to produce
/// consistent capitalization. Unrecognized chord strings are kept verbatim.
fn normalize_chords_in_line(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '[' {
            result.push(c);
            continue;
        }

        // Collect characters until the matching `]`.
        let mut chord_raw = String::new();
        let mut closed = false;
        for ch in chars.by_ref() {
            if ch == ']' {
                closed = true;
                break;
            }
            chord_raw.push(ch);
        }

        result.push('[');
        result.push_str(&normalize_chord_name(&chord_raw));
        if closed {
            result.push(']');
        }
    }
    result
}

/// Normalize a single chord name string.
///
/// Capitalizes the root letter and re-serializes via [`ChordDetail`] display
/// if parsing succeeds. Falls back to the original string on failure.
fn normalize_chord_name(raw: &str) -> String {
    if raw.is_empty() {
        return raw.to_string();
    }
    // Capitalize the first character so the chord parser — which requires an
    // uppercase root letter — can handle lowercase input (e.g., `"am"` → `"Am"`).
    let capitalized = crate::capitalize(raw);
    match parse_chord(&capitalized) {
        Some(detail) => detail.to_string(),
        None => raw.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> FormatOptions {
        FormatOptions::default()
    }

    // --- Directive name normalization ----------------------------------------

    #[test]
    fn directive_alias_soc_expanded() {
        assert_eq!(format("{soc}\n", &opts()), "{start_of_chorus}\n");
    }

    #[test]
    fn directive_alias_eoc_expanded() {
        assert_eq!(format("{eoc}\n", &opts()), "{end_of_chorus}\n");
    }

    #[test]
    fn directive_alias_sov_expanded() {
        assert_eq!(format("{sov}\n", &opts()), "{start_of_verse}\n");
    }

    #[test]
    fn directive_alias_t_with_value() {
        assert_eq!(format("{t: My Song}\n", &opts()), "{title: My Song}\n");
    }

    #[test]
    fn directive_alias_np_expanded() {
        assert_eq!(format("{np}\n", &opts()), "{new_page}\n");
    }

    // --- Directive spacing normalization ------------------------------------

    #[test]
    fn directive_spacing_added_after_colon() {
        assert_eq!(format("{title:My Song}\n", &opts()), "{title: My Song}\n");
    }

    #[test]
    fn directive_spacing_idempotent() {
        assert_eq!(format("{title: My Song}\n", &opts()), "{title: My Song}\n");
    }

    #[test]
    fn directive_no_value_preserved() {
        assert_eq!(format("{new_page}\n", &opts()), "{new_page}\n");
    }

    #[test]
    fn directive_with_selector_preserved() {
        assert_eq!(
            format("{textfont-piano: Courier}\n", &opts()),
            "{textfont-piano: Courier}\n"
        );
    }

    #[test]
    fn directive_name_normalization_disabled() {
        let opts = FormatOptions {
            normalize_directive_names: false,
            ..FormatOptions::default()
        };
        assert_eq!(format("{soc}\n", &opts), "{soc}\n");
    }

    #[test]
    fn directive_with_selector_normalization_disabled() {
        // Regression test: when normalize_directive_names is false the selector
        // must NOT be appended a second time.  Previously this produced
        // `{textfont-piano-piano: Courier}`.
        let opts = FormatOptions {
            normalize_directive_names: false,
            ..FormatOptions::default()
        };
        assert_eq!(
            format("{textfont-piano: Courier}\n", &opts),
            "{textfont-piano: Courier}\n"
        );
    }

    // --- Chord spelling normalization ----------------------------------------

    #[test]
    fn chord_root_capitalized() {
        assert_eq!(format("[am]Hello\n", &opts()), "[Am]Hello\n");
    }

    #[test]
    fn chord_sharp_root_capitalized() {
        assert_eq!(
            format("[c#m7]Hello [g]World\n", &opts()),
            "[C#m7]Hello [G]World\n"
        );
    }

    #[test]
    fn chord_already_canonical_unchanged() {
        assert_eq!(format("[Am]Hello\n", &opts()), "[Am]Hello\n");
    }

    #[test]
    fn chord_spelling_disabled() {
        let opts = FormatOptions {
            normalize_chord_spelling: false,
            ..FormatOptions::default()
        };
        assert_eq!(format("[am]Hello\n", &opts), "[am]Hello\n");
    }

    // --- Section blank lines -------------------------------------------------

    #[test]
    fn section_blank_line_inserted_after_end() {
        let input = "{start_of_chorus}\n[C]Hello\n{end_of_chorus}\n{start_of_verse}\n[G]World\n{end_of_verse}\n";
        let result = format(input, &opts());
        assert!(
            result.contains("{end_of_chorus}\n\n{start_of_verse}"),
            "expected blank line between sections, got:\n{result}"
        );
    }

    #[test]
    fn section_blank_line_not_doubled() {
        // Input already has a blank line — should not produce two blank lines.
        let input = "{start_of_chorus}\n[C]Hello\n{end_of_chorus}\n\n{start_of_verse}\n[G]World\n{end_of_verse}\n";
        let result = format(input, &opts());
        assert!(
            !result.contains("{end_of_chorus}\n\n\n"),
            "unexpected double blank line, got:\n{result}"
        );
    }

    #[test]
    fn section_blank_lines_disabled() {
        let opts = FormatOptions {
            section_blank_lines: false,
            ..FormatOptions::default()
        };
        let input = "{start_of_chorus}\n[C]Hello\n{end_of_chorus}\n{start_of_verse}\n[G]World\n{end_of_verse}\n";
        let result = format(input, &opts);
        assert!(
            !result.contains("{end_of_chorus}\n\n"),
            "expected no blank line insertion, got:\n{result}"
        );
    }

    // --- Blank line collapsing -----------------------------------------------

    #[test]
    fn multiple_blank_lines_collapsed() {
        let result = format("[C]Hello\n\n\n[G]World\n", &opts());
        assert_eq!(result, "[C]Hello\n\n[G]World\n");
    }

    #[test]
    fn trailing_blank_lines_removed() {
        let result = format("[C]Hello\n\n\n", &opts());
        assert_eq!(result, "[C]Hello\n");
    }

    // --- Encoding and newline normalization -----------------------------------

    #[test]
    fn crlf_normalized() {
        let result = format("[C]Hello\r\n[G]World\r\n", &opts());
        assert_eq!(result, "[C]Hello\n[G]World\n");
    }

    #[test]
    fn cr_normalized() {
        let result = format("[C]Hello\r[G]World\r", &opts());
        assert_eq!(result, "[C]Hello\n[G]World\n");
    }

    #[test]
    fn file_ends_with_newline() {
        let result = format("[C]Hello", &opts());
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(format("", &opts()), "");
    }

    #[test]
    fn blank_only_input_returns_empty() {
        assert_eq!(format("\n\n\n", &opts()), "");
    }

    // --- Comment preservation ------------------------------------------------

    #[test]
    fn comment_line_preserved() {
        assert_eq!(
            format("# This is a comment\n", &opts()),
            "# This is a comment\n"
        );
    }

    // --- Idempotence ---------------------------------------------------------

    #[test]
    fn idempotent_full_song() {
        let input = "{t:My Song}\n{artist:Test}\n{soc}\n[am]Hello [g]World\n{eoc}\n";
        let first = format(input, &opts());
        let second = format(&first, &opts());
        assert_eq!(first, second, "format is not idempotent");
    }

    #[test]
    fn idempotent_already_clean() {
        let clean = "{title: My Song}\n{start_of_chorus}\n[Am]Hello [G]World\n{end_of_chorus}\n";
        let result = format(clean, &opts());
        assert_eq!(result, clean, "clean input should be unchanged");
    }
}
