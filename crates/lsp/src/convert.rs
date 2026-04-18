//! Conversion helpers: ChordPro parser types → LSP types.
//!
//! The parser uses 1-based line/column numbers where `column` is a
//! 1-based **character** count (not a byte or UTF-16 code-unit count).
//! LSP positions are 0-based and `character` is measured in the units
//! of the negotiated [`PositionEncoding`]. This module bridges the two.

use chordsketch_core::{ParseError, Span};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::encoding::{PositionEncoding, char_idx_to_lsp_char, nth_line};

/// Converts a [`ParseError`] to an LSP [`Diagnostic`].
///
/// `text` is the full document text — required so the parser's character
/// column can be converted to the negotiated encoding's units.
#[must_use]
pub fn parse_error_to_diagnostic(
    e: &ParseError,
    text: &str,
    encoding: PositionEncoding,
) -> Diagnostic {
    Diagnostic {
        range: span_to_range(&e.span, text, encoding),
        severity: Some(DiagnosticSeverity::ERROR),
        message: e.message.clone(),
        source: Some("chordsketch".into()),
        ..Default::default()
    }
}

/// Converts a parser [`Span`] (1-based, character-column, half-open) to an
/// LSP [`Range`] (0-based) in the negotiated position encoding.
///
/// The parser enforces an input-size cap (see [`chordsketch_core::ParseOptions`])
/// so line/column values fit comfortably within `u32`. `try_from` is used
/// defensively; any value that somehow overflows is clamped to `u32::MAX`.
fn span_to_range(span: &Span, text: &str, encoding: PositionEncoding) -> Range {
    let start_line_idx = span.start.line.saturating_sub(1);
    let start_char_idx = span.start.column.saturating_sub(1);
    let end_line_idx = span.end.line.saturating_sub(1);
    let end_char_idx = span.end.column.saturating_sub(1);

    Range {
        start: Position {
            line: u32::try_from(start_line_idx).unwrap_or(u32::MAX),
            character: lsp_character_at(text, start_line_idx, start_char_idx, encoding),
        },
        end: Position {
            line: u32::try_from(end_line_idx).unwrap_or(u32::MAX),
            character: lsp_character_at(text, end_line_idx, end_char_idx, encoding),
        },
    }
}

/// Character value for the LSP `Position` at (0-based) `line_idx` and
/// 0-based character index `char_idx` into that line, in `encoding` units.
///
/// Uses [`nth_line`] rather than [`str::lines`] so that bare-CR line
/// terminators are recognised consistently with
/// `document_end_position` in `server.rs`.
fn lsp_character_at(
    text: &str,
    line_idx: usize,
    char_idx: usize,
    encoding: PositionEncoding,
) -> u32 {
    let line = nth_line(text, line_idx);
    char_idx_to_lsp_char(line, char_idx, encoding)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chordsketch_core::token::{Position as CsPosition, Span as CsSpan};

    #[test]
    fn span_to_range_converts_1based_to_0based_utf8_ascii() {
        let text = "hello\n";
        let span = CsSpan::new(CsPosition::new(1, 1), CsPosition::new(1, 5));
        let range = span_to_range(&span, text, PositionEncoding::Utf8);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 4);
    }

    #[test]
    fn span_to_range_multiline_utf8_ascii() {
        // Span from line 3 col 2 to line 5 col 2 (1-based).
        let text = "line0\nline1\nline2 foo\nline3\nline4 extra\n";
        let span = CsSpan::new(CsPosition::new(3, 7), CsPosition::new(5, 2));
        let range = span_to_range(&span, text, PositionEncoding::Utf8);
        assert_eq!(range.start.line, 2);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 4);
        assert_eq!(range.end.character, 1);
    }

    #[test]
    fn span_to_range_utf8_non_ascii_converts_chars_to_bytes() {
        // "Ré" occupies columns 1..=2 (char count) but bytes 0..=2 (3 bytes).
        // A span of columns 1..3 → UTF-8 byte range 0..3.
        let text = "Ré world\n";
        let span = CsSpan::new(CsPosition::new(1, 1), CsPosition::new(1, 3));
        let range = span_to_range(&span, text, PositionEncoding::Utf8);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.character, 3);
    }

    #[test]
    fn span_to_range_utf16_non_ascii_counts_code_units() {
        // "Ré" occupies 2 UTF-16 code units (both BMP).
        let text = "Ré world\n";
        let span = CsSpan::new(CsPosition::new(1, 1), CsPosition::new(1, 3));
        let range = span_to_range(&span, text, PositionEncoding::Utf16);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.character, 2);
    }

    #[test]
    fn span_to_range_utf16_astral_counts_surrogate_pair_as_two_units() {
        // U+1F3B8 GUITAR is a surrogate pair in UTF-16 (2 units, 1 char).
        // A span of columns 1..3 (chars 0..2) → UTF-16 units 0..3
        // (char 0 = "a" = 1 unit, char 1 = guitar = 2 units, total 3).
        let text = "a\u{1F3B8}b\n";
        let span = CsSpan::new(CsPosition::new(1, 1), CsPosition::new(1, 3));
        let range = span_to_range(&span, text, PositionEncoding::Utf16);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.character, 3);
    }

    #[test]
    fn span_to_range_cr_only_line_endings() {
        // Bare `\r` line terminators: `str::lines` would fail to split these,
        // producing `character: 0` for lines after the first. `nth_line`
        // recognises bare CR, matching `document_end_position` in server.rs.
        let text = "line0\rhello\rworld";
        let span = CsSpan::new(CsPosition::new(2, 1), CsPosition::new(2, 6));
        let range = span_to_range(&span, text, PositionEncoding::Utf8);
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 1);
        assert_eq!(range.end.character, 5);
    }
}
