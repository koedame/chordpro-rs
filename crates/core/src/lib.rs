//! ChordPro parser, AST definitions, and transforms.

pub mod ast;
pub mod chord;
pub mod lexer;
pub mod parser;
pub mod token;
pub mod transpose;

// Re-export key types for convenience.
pub use chord::{Accidental, ChordDetail, ChordQuality, Note, parse_chord};
pub use lexer::Lexer;
pub use parser::{ParseError, ParseOptions, Parser, parse, parse_with_options};
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
