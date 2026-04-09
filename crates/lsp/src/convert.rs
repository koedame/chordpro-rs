//! Conversion helpers: ChordPro parser types → LSP types.
//!
//! The parser uses 1-based line/column numbers; the LSP protocol uses 0-based.
//! Every conversion in this module applies the necessary offset.

use chordsketch_core::{ParseError, Span};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

/// Converts a [`ParseError`] to an LSP [`Diagnostic`].
#[must_use]
pub fn parse_error_to_diagnostic(e: &ParseError) -> Diagnostic {
    Diagnostic {
        range: span_to_range(&e.span),
        severity: Some(DiagnosticSeverity::ERROR),
        message: e.message.clone(),
        source: Some("chordsketch".into()),
        ..Default::default()
    }
}

/// Converts a parser [`Span`] (1-based, half-open) to an LSP [`Range`] (0-based).
///
/// The parser enforces an input-size cap (see [`chordsketch_core::ParseOptions`])
/// so line/column values fit comfortably within `u32`. `try_from` is used
/// defensively; any value that somehow overflows is clamped to `u32::MAX`.
fn span_to_range(span: &Span) -> Range {
    Range {
        start: Position {
            line: u32::try_from(span.start.line.saturating_sub(1)).unwrap_or(u32::MAX),
            character: u32::try_from(span.start.column.saturating_sub(1)).unwrap_or(u32::MAX),
        },
        end: Position {
            line: u32::try_from(span.end.line.saturating_sub(1)).unwrap_or(u32::MAX),
            character: u32::try_from(span.end.column.saturating_sub(1)).unwrap_or(u32::MAX),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chordsketch_core::token::{Position as CsPosition, Span as CsSpan};

    #[test]
    fn span_to_range_converts_1based_to_0based() {
        let span = CsSpan::new(CsPosition::new(1, 1), CsPosition::new(1, 5));
        let range = span_to_range(&span);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 4);
    }

    #[test]
    fn span_to_range_multiline() {
        let span = CsSpan::new(CsPosition::new(3, 7), CsPosition::new(5, 2));
        let range = span_to_range(&span);
        assert_eq!(range.start.line, 2);
        assert_eq!(range.start.character, 6);
        assert_eq!(range.end.line, 4);
        assert_eq!(range.end.character, 1);
    }
}
