//! LSP position-encoding negotiation and offset conversion.
//!
//! LSP 3.17 requires the server to pick an encoding from the client's
//! advertised `general.positionEncodings` list. If the client does not
//! advertise the capability, the server must use UTF-16 (the spec default).
//!
//! `chordsketch-lsp` prefers UTF-8 when the client advertises it — that
//! matches the parser's native byte offsets and avoids per-character
//! UTF-16 conversion — but falls back to UTF-16 for clients (including
//! `vscode-languageclient` 9.x) that advertise only UTF-16.

use tower_lsp::lsp_types::{InitializeParams, PositionEncodingKind};

/// The negotiated LSP position encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionEncoding {
    /// `Position.character` is a UTF-8 byte offset.
    Utf8,
    /// `Position.character` is a UTF-16 code-unit offset.
    Utf16,
}

impl PositionEncoding {
    /// Maps this encoding to the corresponding [`PositionEncodingKind`]
    /// for the `InitializeResult.capabilities.position_encoding` reply.
    #[must_use]
    pub fn to_kind(self) -> PositionEncodingKind {
        match self {
            Self::Utf8 => PositionEncodingKind::UTF8,
            Self::Utf16 => PositionEncodingKind::UTF16,
        }
    }
}

/// Pick an encoding from the client's `general.positionEncodings`.
///
/// - If the client advertises UTF-8, choose UTF-8 (no per-character
///   conversion needed; matches the parser's native byte offsets).
/// - Otherwise, return UTF-16. This covers both the case where the client
///   advertises UTF-16 only (e.g. `vscode-languageclient` 9.x, which
///   hardcodes `['utf-16']`) and the case where the client omits the
///   capability entirely — per LSP 3.17 the default is `['utf-16']` and
///   the server MUST reply with UTF-16 in that case.
#[must_use]
pub fn negotiate_encoding(params: &InitializeParams) -> PositionEncoding {
    let Some(list) = params
        .capabilities
        .general
        .as_ref()
        .and_then(|g| g.position_encodings.as_ref())
    else {
        return PositionEncoding::Utf16;
    };
    if list.contains(&PositionEncodingKind::UTF8) {
        PositionEncoding::Utf8
    } else {
        PositionEncoding::Utf16
    }
}

/// Convert a client-supplied `Position.character` value on `line` to a
/// 0-based character index into that line.
///
/// - Under UTF-8, `lsp_char` is a byte offset into `line`.
/// - Under UTF-16, `lsp_char` is a UTF-16 code-unit offset.
///
/// When `lsp_char` falls inside a multi-byte code unit sequence (which
/// should not happen for a spec-compliant client), the returned index
/// points to the character whose first unit is beyond `lsp_char`.
#[must_use]
pub fn lsp_char_to_char_idx(line: &str, lsp_char: u32, encoding: PositionEncoding) -> usize {
    match encoding {
        PositionEncoding::Utf8 => {
            let byte_col = lsp_char as usize;
            line.char_indices()
                .take_while(|(b, _)| *b < byte_col)
                .count()
        }
        PositionEncoding::Utf16 => {
            let mut consumed: u32 = 0;
            for (i, c) in line.chars().enumerate() {
                if consumed >= lsp_char {
                    return i;
                }
                consumed += c.len_utf16() as u32;
            }
            line.chars().count()
        }
    }
}

/// Convert a 0-based character index into `line` to the `Position.character`
/// value appropriate for the negotiated encoding.
///
/// Returns the length of `line` in the negotiated units when `char_idx`
/// is at or beyond the last character (the correct sentinel for an LSP
/// range end).
#[must_use]
pub fn char_idx_to_lsp_char(line: &str, char_idx: usize, encoding: PositionEncoding) -> u32 {
    match encoding {
        PositionEncoding::Utf8 => line
            .char_indices()
            .nth(char_idx)
            .map(|(b, _)| b as u32)
            .unwrap_or_else(|| line.len() as u32),
        PositionEncoding::Utf16 => line
            .chars()
            .take(char_idx)
            .map(|c| c.len_utf16() as u32)
            .sum(),
    }
}

/// Length of `line` expressed in the negotiated encoding's units.
#[must_use]
pub fn line_length(line: &str, encoding: PositionEncoding) -> u32 {
    match encoding {
        PositionEncoding::Utf8 => line.len() as u32,
        PositionEncoding::Utf16 => line.chars().map(|c| c.len_utf16() as u32).sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{ClientCapabilities, GeneralClientCapabilities};

    fn make_params(encodings: Option<Vec<PositionEncodingKind>>) -> InitializeParams {
        InitializeParams {
            capabilities: ClientCapabilities {
                general: Some(GeneralClientCapabilities {
                    position_encodings: encodings,
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn negotiate_returns_utf8_when_client_advertises_utf8() {
        let params = make_params(Some(vec![
            PositionEncodingKind::UTF8,
            PositionEncodingKind::UTF16,
        ]));
        assert_eq!(negotiate_encoding(&params), PositionEncoding::Utf8);
    }

    #[test]
    fn negotiate_returns_utf16_when_client_advertises_only_utf16() {
        // vscode-languageclient 9.x behaviour.
        let params = make_params(Some(vec![PositionEncodingKind::UTF16]));
        assert_eq!(negotiate_encoding(&params), PositionEncoding::Utf16);
    }

    #[test]
    fn negotiate_returns_utf16_when_position_encodings_absent() {
        // LSP 3.17: when the capability is absent the default is ['utf-16'].
        let params = make_params(None);
        assert_eq!(negotiate_encoding(&params), PositionEncoding::Utf16);
    }

    #[test]
    fn negotiate_returns_utf16_when_general_absent() {
        let params = InitializeParams {
            capabilities: ClientCapabilities {
                general: None,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(negotiate_encoding(&params), PositionEncoding::Utf16);
    }

    #[test]
    fn negotiate_returns_utf16_when_list_empty() {
        let params = make_params(Some(vec![]));
        assert_eq!(negotiate_encoding(&params), PositionEncoding::Utf16);
    }

    #[test]
    fn negotiate_returns_utf16_when_list_has_only_utf32() {
        // Unknown/unsupported encodings are not UTF-8, so fall back to UTF-16.
        let params = make_params(Some(vec![PositionEncodingKind::UTF32]));
        assert_eq!(negotiate_encoding(&params), PositionEncoding::Utf16);
    }

    #[test]
    fn utf8_line_length_counts_bytes() {
        // "Ré" is 3 UTF-8 bytes (R=1, é=2).
        assert_eq!(line_length("Ré", PositionEncoding::Utf8), 3);
    }

    #[test]
    fn utf16_line_length_counts_code_units_bmp() {
        // All BMP characters → 1 UTF-16 code unit each.
        assert_eq!(line_length("Ré", PositionEncoding::Utf16), 2);
    }

    #[test]
    fn utf16_line_length_counts_code_units_astral() {
        // U+1F3B8 GUITAR = 2 UTF-16 code units (surrogate pair).
        assert_eq!(line_length("\u{1F3B8}", PositionEncoding::Utf16), 2);
        // And 4 UTF-8 bytes.
        assert_eq!(line_length("\u{1F3B8}", PositionEncoding::Utf8), 4);
    }

    #[test]
    fn char_idx_to_lsp_char_utf8_matches_byte_offset() {
        // "Ré world" — R=1 byte, é=2 bytes, rest ASCII.
        let line = "Ré world";
        assert_eq!(char_idx_to_lsp_char(line, 0, PositionEncoding::Utf8), 0);
        assert_eq!(char_idx_to_lsp_char(line, 1, PositionEncoding::Utf8), 1);
        assert_eq!(char_idx_to_lsp_char(line, 2, PositionEncoding::Utf8), 3);
        // char_idx beyond the last char clamps to line.len() (bytes).
        assert_eq!(
            char_idx_to_lsp_char(line, 999, PositionEncoding::Utf8),
            line.len() as u32
        );
    }

    #[test]
    fn char_idx_to_lsp_char_utf16_counts_code_units() {
        // BMP characters: 1 UTF-16 unit each.
        let line = "Ré world";
        assert_eq!(char_idx_to_lsp_char(line, 0, PositionEncoding::Utf16), 0);
        assert_eq!(char_idx_to_lsp_char(line, 1, PositionEncoding::Utf16), 1);
        assert_eq!(char_idx_to_lsp_char(line, 2, PositionEncoding::Utf16), 2);

        // Astral: surrogate pair counts as 2 units.
        let line = "a\u{1F3B8}b";
        assert_eq!(char_idx_to_lsp_char(line, 0, PositionEncoding::Utf16), 0);
        assert_eq!(char_idx_to_lsp_char(line, 1, PositionEncoding::Utf16), 1);
        assert_eq!(char_idx_to_lsp_char(line, 2, PositionEncoding::Utf16), 3);
        assert_eq!(char_idx_to_lsp_char(line, 3, PositionEncoding::Utf16), 4);
    }

    #[test]
    fn lsp_char_to_char_idx_utf8_maps_byte_to_char() {
        // "Ré world" — byte 0 → char 0, byte 3 → char 2 ("R" at byte 0, "é" at bytes 1-2, " " at byte 3).
        let line = "Ré world";
        assert_eq!(lsp_char_to_char_idx(line, 0, PositionEncoding::Utf8), 0);
        assert_eq!(lsp_char_to_char_idx(line, 1, PositionEncoding::Utf8), 1);
        assert_eq!(lsp_char_to_char_idx(line, 3, PositionEncoding::Utf8), 2);
    }

    #[test]
    fn lsp_char_to_char_idx_utf16_maps_code_unit_to_char() {
        // Surrogate pair: UTF-16 offsets 0, 1 (inside surrogate), 2, 3 map to char indices 0, 1, 1, 2.
        let line = "a\u{1F3B8}b";
        assert_eq!(lsp_char_to_char_idx(line, 0, PositionEncoding::Utf16), 0);
        assert_eq!(lsp_char_to_char_idx(line, 1, PositionEncoding::Utf16), 1);
        assert_eq!(lsp_char_to_char_idx(line, 3, PositionEncoding::Utf16), 2);
    }
}
