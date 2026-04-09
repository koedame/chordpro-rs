//! ChordPro parser, AST definitions, and transforms.

pub mod ast;
pub mod chord;
pub mod chord_diagram;
pub mod config;
pub mod escape;
#[cfg(not(target_arch = "wasm32"))]
pub mod external_tool;
pub mod formatter;
pub mod heuristic;
pub mod image_path;
pub mod inline_markup;
pub mod lexer;
pub mod parser;
pub mod render_result;
pub mod rrjson;
pub mod selector;
pub mod token;
pub mod transpose;
pub mod voicings;

// Re-export key types for convenience.
pub use chord::{Accidental, ChordDetail, ChordQuality, Note, parse_chord};
pub use chord_diagram::{canonical_chord_name, resolve_diagrams_instrument};
// Aliased as `format_chordpro` to avoid ambiguity with the `format!` macro at
// call sites that use glob imports.
pub use formatter::{FormatOptions, format as format_chordpro};
pub use heuristic::{
    InputFormat, PlainTextImporter, convert_plain_text, detect_format, song_to_chordpro,
};
pub use lexer::Lexer;
pub use parser::{
    MultiParseResult, ParseError, ParseOptions, ParseResult, Parser, parse, parse_image_attributes,
    parse_lenient, parse_lenient_with_options, parse_multi, parse_multi_lenient,
    parse_multi_lenient_with_options, parse_multi_with_options, parse_with_options,
};
pub use render_result::RenderResult;
pub use token::{Position, Span, Token, TokenKind};
pub use voicings::{guitar_voicing, lookup_diagram, ukulele_voicing};

/// Returns the library version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Capitalize the first character of a string.
///
/// Returns a new string with the first character uppercased and the
/// rest unchanged. Returns an empty string for empty input.
#[must_use]
pub fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION"));
    }
}
