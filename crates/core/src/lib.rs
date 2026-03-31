//! ChordPro parser, AST definitions, and transforms.

pub mod ast;
pub mod chord;
pub mod chord_diagram;
pub mod config;
pub mod inline_markup;
pub mod lexer;
pub mod parser;
pub mod rrjson;
pub mod selector;
pub mod token;
pub mod transpose;

// Re-export key types for convenience.
pub use chord::{Accidental, ChordDetail, ChordQuality, Note, parse_chord};
pub use lexer::Lexer;
pub use parser::{
    MultiParseResult, ParseError, ParseOptions, ParseResult, Parser, parse, parse_image_attributes,
    parse_lenient, parse_lenient_with_options, parse_multi, parse_multi_lenient,
    parse_multi_lenient_with_options, parse_multi_with_options, parse_with_options,
};
pub use token::{Position, Span, Token, TokenKind};

/// Returns the library version.
#[must_use]
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(version(), "0.1.0");
    }
}
