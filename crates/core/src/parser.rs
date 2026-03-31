//! Parser that transforms a token stream into a ChordPro AST.
//!
//! The parser accepts the flat token sequence produced by [`crate::Lexer`] and
//! builds a [`Song`] AST. Each source line is classified as a directive, a
//! lyrics line (with optional inline chord annotations), an empty line, or a
//! comment (from `{comment}`, `{comment_italic}`, or `{comment_box}`
//! directives).
//!
//! # Directive Classification
//!
//! Directives are classified into typed variants via [`DirectiveKind`]. The
//! parser resolves short aliases (e.g., `t` → `title`, `soc` →
//! `start_of_chorus`) and normalizes names to their canonical lowercase form.
//! Metadata directives automatically populate the [`Song::metadata`] fields.
//!
//! # Convenience Function
//!
//! The [`parse`] function combines lexing and parsing into a single step:
//!
//! ```
//! use chordpro_core::parser::parse;
//!
//! let song = parse("{title: Hello}\n[Am]World").unwrap();
//! assert_eq!(song.metadata.title.as_deref(), Some("Hello"));
//! assert_eq!(song.lines.len(), 2);
//! ```
//!
//! # Error Handling
//!
//! The parser returns [`ParseError`] when the token stream contains structural
//! problems such as unclosed directives, unclosed chords, or empty directives.

use crate::Lexer;
use crate::ast::{
    Chord, CommentStyle, Directive, DirectiveKind, ImageAttributes, Line, LyricsLine,
    LyricsSegment, Song,
};
use crate::inline_markup;
use crate::token::{Span, Token, TokenKind};

// ---------------------------------------------------------------------------
// ParseError
// ---------------------------------------------------------------------------

/// An error encountered during parsing.
///
/// Each error carries a human-readable message and the [`Span`] in the source
/// text where the problem was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// A description of what went wrong.
    pub message: String,
    /// The location in the source text where the error was detected.
    pub span: Span,
}

impl ParseError {
    /// Creates a new `ParseError` with the given message and span.
    fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    /// Returns the 1-based line number where the error was detected.
    #[must_use]
    pub fn line(&self) -> usize {
        self.span.start.line
    }

    /// Returns the 1-based column number where the error was detected.
    #[must_use]
    pub fn column(&self) -> usize {
        self.span.start.column
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "parse error at line {}, column {}: {}",
            self.span.start.line, self.span.start.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}

// ---------------------------------------------------------------------------
// ParseResult
// ---------------------------------------------------------------------------

/// The result of a lenient parse, containing a partial AST and any errors.
///
/// When using [`Parser::parse_lenient`] or [`parse_lenient`], the parser
/// recovers from errors by skipping problematic lines and continuing.
/// The `song` field contains all successfully parsed lines, and `errors`
/// contains all problems encountered.
///
/// # Examples
///
/// ```
/// use chordpro_core::parser::parse_lenient;
///
/// let result = parse_lenient("{title: Test}\n[Am\nHello world");
/// assert_eq!(result.song.metadata.title.as_deref(), Some("Test"));
/// assert_eq!(result.errors.len(), 1); // unclosed chord on line 2
/// assert_eq!(result.song.lines.len(), 2); // title directive + lyrics (error line skipped)
/// ```
#[derive(Debug, Clone)]
pub struct ParseResult {
    /// The partial AST with all successfully parsed lines.
    pub song: Song,
    /// All errors encountered during parsing.
    pub errors: Vec<ParseError>,
}

impl ParseResult {
    /// Returns `true` if no errors were encountered.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns `true` if any errors were encountered.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// A parser that transforms a token stream into a [`Song`] AST.
///
/// The parser is created from a `Vec<Token>` (typically produced by
/// [`Lexer::tokenize`]) and consumes tokens one at a time, building up the
/// AST line by line.
pub struct Parser {
    /// The token stream to consume.
    tokens: Vec<Token>,
    /// Current index into `tokens`.
    pos: usize,
    /// When inside a verbatim section (tab, grid, ABC, Lilypond, SVG, textblock),
    /// this holds the end-directive name that will close the section.
    /// Lines inside verbatim sections are treated as plain text (no chord parsing).
    verbatim_end: Option<String>,
}

impl Parser {
    /// Creates a new parser for the given token stream.
    #[must_use]
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            verbatim_end: None,
        }
    }

    /// Parses the token stream and returns a [`Song`] AST.
    ///
    /// Metadata directives (`{title}`, `{artist}`, etc.) automatically
    /// populate [`Song::metadata`]. Comment directives are converted to
    /// [`Line::Comment`] with the appropriate [`CommentStyle`].
    ///
    /// Returns a [`ParseError`] on the first structural problem encountered
    /// (e.g., unclosed directives or chords). Use [`parse_lenient`] to
    /// collect all errors and obtain a partial AST.
    pub fn parse(mut self) -> Result<Song, ParseError> {
        let mut song = Song::new();

        while !self.is_at_end() {
            let line = self.parse_line()?;

            // If this is a metadata directive, populate the Song's metadata.
            if let Line::Directive(ref directive) = line {
                Self::populate_metadata(&mut song.metadata, directive);
            }

            song.lines.push(line);
        }

        Ok(song)
    }

    /// Parses the token stream leniently, collecting all errors.
    ///
    /// Unlike [`parse`], this method does not stop at the first error.
    /// When a line cannot be parsed, the error is recorded and the parser
    /// skips to the next line to continue. The returned [`ParseResult`]
    /// contains the partial AST (all successfully parsed lines) and a
    /// list of all errors encountered.
    pub fn parse_lenient(mut self) -> ParseResult {
        let mut song = Song::new();
        let mut errors = Vec::new();

        while !self.is_at_end() {
            match self.parse_line() {
                Ok(line) => {
                    if let Line::Directive(ref directive) = line {
                        Self::populate_metadata(&mut song.metadata, directive);
                    }
                    song.lines.push(line);
                }
                Err(e) => {
                    errors.push(e);
                    // Skip to the next line to recover.
                    self.skip_to_next_line();
                }
            }
        }

        ParseResult { song, errors }
    }

    /// Advances past all tokens until the next Newline or Eof,
    /// then consumes the Newline if present. Used for error recovery.
    fn skip_to_next_line(&mut self) {
        while !self.is_at_end() {
            if self.peek_kind() == &TokenKind::Newline {
                self.advance();
                return;
            }
            self.advance();
        }
    }

    // -- Metadata population ------------------------------------------------

    /// Populates metadata fields from a directive, if it is a known metadata
    /// directive with a value.
    fn populate_metadata(metadata: &mut crate::ast::Metadata, directive: &Directive) {
        let value = match directive.value.as_deref() {
            Some(v) => v.to_string(),
            None => return, // Metadata directives without values are no-ops.
        };

        match directive.kind {
            DirectiveKind::Title => {
                metadata.title = Some(value);
            }
            DirectiveKind::Subtitle => {
                metadata.subtitles.push(value);
            }
            DirectiveKind::Artist => {
                metadata.artists.push(value);
            }
            DirectiveKind::Composer => {
                metadata.composers.push(value);
            }
            DirectiveKind::Lyricist => {
                metadata.lyricists.push(value);
            }
            DirectiveKind::Album => {
                metadata.album = Some(value);
            }
            DirectiveKind::Year => {
                metadata.year = Some(value);
            }
            DirectiveKind::Key => {
                metadata.key = Some(value);
            }
            DirectiveKind::Tempo => {
                metadata.tempo = Some(value);
            }
            DirectiveKind::Time => {
                metadata.time = Some(value);
            }
            DirectiveKind::Capo => {
                metadata.capo = Some(value);
            }
            DirectiveKind::SortTitle => {
                metadata.sort_title = Some(value);
            }
            DirectiveKind::SortArtist => {
                metadata.sort_artist = Some(value);
            }
            DirectiveKind::Arranger => {
                metadata.arrangers.push(value);
            }
            DirectiveKind::Copyright => {
                metadata.copyright = Some(value);
            }
            DirectiveKind::Duration => {
                metadata.duration = Some(value);
            }
            DirectiveKind::Tag => {
                metadata.tags.push(value);
            }
            DirectiveKind::Meta(ref key) => match key.to_ascii_lowercase().as_str() {
                "title" | "t" => metadata.title = Some(value),
                "subtitle" | "st" => metadata.subtitles.push(value),
                "artist" => metadata.artists.push(value),
                "composer" => metadata.composers.push(value),
                "lyricist" => metadata.lyricists.push(value),
                "album" => metadata.album = Some(value),
                "year" => metadata.year = Some(value),
                "key" => metadata.key = Some(value),
                "tempo" => metadata.tempo = Some(value),
                "time" => metadata.time = Some(value),
                "capo" => metadata.capo = Some(value),
                "sorttitle" => metadata.sort_title = Some(value),
                "sortartist" => metadata.sort_artist = Some(value),
                "arranger" => metadata.arrangers.push(value),
                "copyright" => metadata.copyright = Some(value),
                "duration" => metadata.duration = Some(value),
                "tag" => metadata.tags.push(value),
                _ => metadata.custom.push((key.clone(), value)),
            },
            DirectiveKind::Unknown(ref name) => {
                metadata.custom.push((name.clone(), value));
            }
            _ => {}
        }
    }

    // -- Token navigation ---------------------------------------------------

    /// Returns `true` when all meaningful tokens have been consumed.
    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.peek_kind() == &TokenKind::Eof
    }

    /// Returns a reference to the current token's kind without advancing.
    fn peek_kind(&self) -> &TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    /// Returns a reference to the current token without advancing.
    fn peek(&self) -> &Token {
        // SAFETY: the caller ensures we are not past the end. The last token
        // is always Eof, so indexing is safe as long as `pos < tokens.len()`.
        &self.tokens[self.pos]
    }

    /// Advances past the current token and returns it.
    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos];
        self.pos += 1;
        tok
    }

    // -- Line parsing -------------------------------------------------------

    /// Parses a single line (up to and including the next Newline or Eof).
    fn parse_line(&mut self) -> Result<Line, ParseError> {
        let in_verbatim = self.verbatim_end.is_some();

        match self.peek_kind() {
            // An empty line: just a Newline token.
            TokenKind::Newline => {
                self.advance();
                Ok(Line::Empty)
            }
            // A directive line: starts with `{`.
            TokenKind::DirectiveOpen => {
                // Inside a verbatim section: only the matching end directive
                // is parsed; everything else is verbatim text.
                if in_verbatim && !self.is_verbatim_end_ahead() {
                    return self.parse_verbatim_line();
                }
                let line = self.parse_directive_line()?;
                // Track verbatim section state.
                if let Line::Directive(ref d) = line {
                    if let Some(end_name) = Self::verbatim_end_for(&d.kind) {
                        self.verbatim_end = Some(end_name);
                    } else if d.kind.is_section_end() && in_verbatim {
                        self.verbatim_end = None;
                    }
                }
                Ok(line)
            }
            // Inside a verbatim section: treat as plain text (no chord parsing).
            _ if in_verbatim => self.parse_verbatim_line(),
            // Anything else: a lyrics line.
            _ => self.parse_lyrics_line(),
        }
    }

    /// Returns the end-directive name for section types that use verbatim
    /// content (tab, grid, ABC, Lilypond, SVG, textblock). Returns `None` for
    /// sections that parse chords normally (chorus, verse, bridge, custom).
    fn verbatim_end_for(kind: &DirectiveKind) -> Option<String> {
        match kind {
            DirectiveKind::StartOfTab => Some("end_of_tab".to_string()),
            DirectiveKind::StartOfGrid => Some("end_of_grid".to_string()),
            DirectiveKind::StartOfAbc => Some("end_of_abc".to_string()),
            DirectiveKind::StartOfLy => Some("end_of_ly".to_string()),
            DirectiveKind::StartOfSvg => Some("end_of_svg".to_string()),
            DirectiveKind::StartOfTextblock => Some("end_of_textblock".to_string()),
            _ => None,
        }
    }

    /// Peeks ahead to check if the current `{` starts the end directive
    /// that closes the current verbatim section. This allows the parser
    /// to exit verbatim mode.
    ///
    /// Only checks the next token after `DirectiveOpen` for the directive
    /// name text; the full directive structure (including `DirectiveClose`)
    /// is validated later by `parse_directive_line`.
    fn is_verbatim_end_ahead(&self) -> bool {
        if let Some(ref end_name) = self.verbatim_end {
            if self.pos + 1 < self.tokens.len() {
                if let TokenKind::Text(ref text) = self.tokens[self.pos + 1].kind {
                    let trimmed = text.trim().to_ascii_lowercase();
                    // Check full name
                    if trimmed == *end_name {
                        return true;
                    }
                    // Check short aliases
                    return match end_name.as_str() {
                        "end_of_tab" => trimmed == "eot",
                        "end_of_grid" => trimmed == "eog",
                        _ => false,
                    };
                }
            }
        }
        false
    }

    /// Parses a verbatim text line (used inside tab, grid, and delegate environment sections).
    ///
    /// All tokens until the next Newline/Eof are collected as plain text,
    /// with no chord bracket interpretation. The result is a lyrics line
    /// with a single text-only segment.
    fn parse_verbatim_line(&mut self) -> Result<Line, ParseError> {
        let mut text = String::new();

        loop {
            match self.peek_kind() {
                TokenKind::Newline | TokenKind::Eof => break,
                TokenKind::ChordOpen => {
                    text.push('[');
                    self.advance();
                }
                TokenKind::ChordClose => {
                    text.push(']');
                    self.advance();
                }
                TokenKind::DirectiveOpen => {
                    text.push('{');
                    self.advance();
                }
                TokenKind::DirectiveClose => {
                    text.push('}');
                    self.advance();
                }
                TokenKind::Colon => {
                    text.push(':');
                    self.advance();
                }
                TokenKind::Text(t) => {
                    text.push_str(t);
                    self.advance();
                }
            }
        }

        // Consume the newline.
        if self.peek_kind() == &TokenKind::Newline {
            self.advance();
        }

        if text.is_empty() {
            Ok(Line::Empty)
        } else {
            Ok(Line::Lyrics(LyricsLine {
                segments: vec![LyricsSegment::text_only(text)],
            }))
        }
    }

    // -- Directive parsing --------------------------------------------------

    /// Parses a directive line: `{name}` or `{name: value}`.
    ///
    /// After parsing the directive itself, consumes the trailing Newline (or
    /// verifies Eof). Comment directives (`comment`, `comment_italic`,
    /// `comment_box`) are converted to [`Line::Comment`].
    fn parse_directive_line(&mut self) -> Result<Line, ParseError> {
        let open_span = self.peek().span;
        self.advance(); // consume DirectiveOpen

        // Collect the directive name.
        let name = self.parse_directive_name(&open_span)?;

        // Check for a colon (indicates a value follows).
        let value = if self.peek_kind() == &TokenKind::Colon {
            self.advance(); // consume Colon
            Some(self.parse_directive_value())
        } else {
            None
        };

        // Expect the closing brace.
        if self.peek_kind() != &TokenKind::DirectiveClose {
            let span = self.peek().span;
            return Err(ParseError::new("unclosed directive: expected `}`", span));
        }
        self.advance();

        // Consume trailing newline if present.
        if self.peek_kind() == &TokenKind::Newline {
            self.advance();
        }

        // Trim whitespace from name and value.
        let name = name.trim().to_string();
        let value = value.map(|v| v.trim().to_string());

        // Classify the directive, detecting any selector suffix.
        let (kind, selector) = DirectiveKind::resolve_with_selector(&name);

        // Comment directives without a selector → Line::Comment with appropriate style.
        // Comment directives WITH a selector are kept as Line::Directive so
        // the selector information is preserved for downstream filtering.
        if kind.is_comment() && selector.is_none() {
            let style = match kind {
                DirectiveKind::Comment => CommentStyle::Normal,
                DirectiveKind::CommentItalic => CommentStyle::Italic,
                DirectiveKind::CommentBox => CommentStyle::Boxed,
                _ => unreachable!(),
            };
            let text = value.unwrap_or_default();
            return Ok(Line::Comment(style, text));
        }

        // Meta directive: split value into key + remaining value.
        if matches!(kind, DirectiveKind::Meta(_)) {
            if let Some(ref val) = value {
                let trimmed = val.trim();
                if let Some(pos) = trimmed.find(|c: char| c.is_whitespace()) {
                    let meta_key = trimmed[..pos].to_string();
                    let meta_value = trimmed[pos..].trim().to_string();
                    let kind = DirectiveKind::Meta(meta_key.clone());
                    let directive = Directive {
                        name: "meta".to_string(),
                        value: if meta_value.is_empty() {
                            None
                        } else {
                            Some(meta_value)
                        },
                        kind,
                        selector,
                    };
                    return Ok(Line::Directive(directive));
                } else if !trimmed.is_empty() {
                    // Only a key, no value
                    let meta_key = trimmed.to_string();
                    let kind = DirectiveKind::Meta(meta_key);
                    let directive = Directive {
                        name: "meta".to_string(),
                        value: None,
                        kind,
                        selector,
                    };
                    return Ok(Line::Directive(directive));
                }
            }
            // {meta} without value — treat as unknown
            let directive = Directive {
                name: "meta".to_string(),
                value: None,
                kind: DirectiveKind::Unknown("meta".to_string()),
                selector,
            };
            return Ok(Line::Directive(directive));
        }

        // Image directive: parse key=value attributes from the value string.
        if kind.is_image() {
            let attrs = match &value {
                Some(v) => parse_image_attributes(v),
                None => ImageAttributes::default(),
            };
            let kind = DirectiveKind::Image(attrs);
            let canonical = kind.canonical_name().to_string();
            let directive = Directive {
                name: canonical,
                value,
                kind,
                selector,
            };
            return Ok(Line::Directive(directive));
        }

        // Build the directive with canonical name, kind, and optional selector.
        let canonical = kind.full_canonical_name();
        let directive = Directive {
            name: canonical,
            value,
            kind,
            selector,
        };

        Ok(Line::Directive(directive))
    }

    /// Parses the directive name (text between `{` and either `:` or `}`).
    fn parse_directive_name(&mut self, open_span: &Span) -> Result<String, ParseError> {
        let mut name = String::new();

        loop {
            match self.peek_kind() {
                TokenKind::Text(text) => {
                    name.push_str(text);
                    self.advance();
                }
                TokenKind::Colon | TokenKind::DirectiveClose => break,
                TokenKind::Eof | TokenKind::Newline => {
                    return Err(ParseError::new(
                        "unclosed directive: expected `}`",
                        *open_span,
                    ));
                }
                _ => {
                    // Unexpected token inside directive name (e.g., ChordOpen).
                    let tok = self.peek();
                    return Err(ParseError::new(
                        format!("unexpected {:?} in directive name", tok.kind),
                        tok.span,
                    ));
                }
            }
        }

        if name.trim().is_empty() {
            return Err(ParseError::new("empty directive name", *open_span));
        }

        Ok(name)
    }

    /// Parses the directive value (everything between `:` and `}`).
    ///
    /// The value may contain text tokens and other tokens (like ChordOpen/Close)
    /// that appear literally in the directive value. We collect all text content.
    fn parse_directive_value(&mut self) -> String {
        let mut value = String::new();

        loop {
            match self.peek_kind() {
                TokenKind::Text(text) => {
                    value.push_str(text);
                    self.advance();
                }
                TokenKind::DirectiveClose | TokenKind::Eof | TokenKind::Newline => break,
                TokenKind::Colon => {
                    // Additional colons in value are literal text.
                    value.push(':');
                    self.advance();
                }
                TokenKind::ChordOpen => {
                    value.push('[');
                    self.advance();
                }
                TokenKind::ChordClose => {
                    value.push(']');
                    self.advance();
                }
                TokenKind::DirectiveOpen => {
                    value.push('{');
                    self.advance();
                }
            }
        }

        value
    }

    // -- Lyrics line parsing ------------------------------------------------

    /// Parses a lyrics line containing text and optional chord annotations.
    ///
    /// The line is split into [`LyricsSegment`]s, each consisting of an
    /// optional chord followed by lyric text.
    fn parse_lyrics_line(&mut self) -> Result<Line, ParseError> {
        let mut segments: Vec<LyricsSegment> = Vec::new();
        let mut current_chord: Option<Chord> = None;
        let mut current_text = String::new();

        loop {
            match self.peek_kind() {
                TokenKind::Newline | TokenKind::Eof => {
                    break;
                }
                TokenKind::ChordOpen => {
                    // Flush the current segment before starting a new chord.
                    if current_chord.is_some() || !current_text.is_empty() {
                        segments.push(LyricsSegment::new(
                            current_chord.take(),
                            core::mem::take(&mut current_text),
                        ));
                    }

                    current_chord = Some(self.parse_chord()?);
                }
                TokenKind::Text(text) => {
                    current_text.push_str(text);
                    self.advance();
                }
                TokenKind::DirectiveOpen => {
                    // A directive starting mid-line is unexpected in well-formed
                    // ChordPro, but we handle it gracefully by treating it as
                    // the start of a directive line. First, flush the current
                    // lyrics if any, then break and let the directive be parsed
                    // on a subsequent call.
                    //
                    // However, per the task spec, directives always start at the
                    // beginning of a line. If we see one mid-line, it is likely
                    // a stray `{`. Treat the rest as text.
                    current_text.push('{');
                    self.advance();
                }
                TokenKind::DirectiveClose => {
                    // A stray `}` outside a directive — include as literal text.
                    current_text.push('}');
                    self.advance();
                }
                TokenKind::ChordClose => {
                    // A stray `]` outside a chord — include as literal text.
                    current_text.push(']');
                    self.advance();
                }
                TokenKind::Colon => {
                    // Outside a directive, colons are text. The lexer only emits
                    // Colon inside directives, so this shouldn't normally occur
                    // here, but handle defensively.
                    current_text.push(':');
                    self.advance();
                }
            }
        }

        // Flush the last segment.
        if current_chord.is_some() || !current_text.is_empty() {
            segments.push(LyricsSegment::new(current_chord, current_text));
        }

        // Consume the trailing newline if present.
        if self.peek_kind() == &TokenKind::Newline {
            self.advance();
        }

        if segments.is_empty() {
            Ok(Line::Empty)
        } else {
            // Parse inline markup for each segment's text.
            let segments = segments
                .into_iter()
                .map(Self::apply_inline_markup)
                .collect();
            Ok(Line::Lyrics(LyricsLine { segments }))
        }
    }

    /// Applies inline markup parsing to a lyrics segment.
    ///
    /// If the segment's text contains inline markup tags, the `spans` field is
    /// populated with the parsed span tree and the `text` field is updated to
    /// contain only the plain text (markup tags stripped). If no markup is found,
    /// the segment is returned unchanged.
    fn apply_inline_markup(mut segment: LyricsSegment) -> LyricsSegment {
        if inline_markup::has_inline_markup(&segment.text) {
            let spans = inline_markup::parse_inline_markup(&segment.text);
            if !spans.is_empty() {
                // Update text to be the plain-text version (tags stripped)
                segment.text = inline_markup::spans_to_plain_text(&spans);
                segment.spans = spans;
            }
        }
        segment
    }

    /// Parses a chord annotation: `[` text `]`.
    ///
    /// The opening bracket has already been peeked; this method consumes it,
    /// the chord text, and the closing bracket.
    fn parse_chord(&mut self) -> Result<Chord, ParseError> {
        let open_span = self.peek().span;
        self.advance(); // consume ChordOpen

        let mut name = String::new();

        loop {
            match self.peek_kind() {
                TokenKind::Text(text) => {
                    name.push_str(text);
                    self.advance();
                }
                TokenKind::ChordClose => {
                    self.advance(); // consume ChordClose
                    break;
                }
                TokenKind::Newline | TokenKind::Eof => {
                    return Err(ParseError::new("unclosed chord: expected `]`", open_span));
                }
                _ => {
                    // Unexpected token inside a chord (e.g., DirectiveOpen).
                    let tok = self.peek();
                    return Err(ParseError::new(
                        format!("unexpected {:?} inside chord", tok.kind),
                        tok.span,
                    ));
                }
            }
        }

        Ok(Chord::new(name))
    }
}

// ---------------------------------------------------------------------------
// Convenience function
// ---------------------------------------------------------------------------

/// Parses a ChordPro source string into a [`Song`] AST.
///
/// This is a convenience function that runs the lexer and parser in sequence.
/// Metadata directives populate [`Song::metadata`] automatically.
///
/// # Errors
///
/// Returns a [`ParseError`] if the input contains structural problems.
///
/// # Examples
///
/// ```
/// use chordpro_core::parser::parse;
///
/// let song = parse("{title: Hello World}\n[Am]La la la").unwrap();
/// assert_eq!(song.metadata.title.as_deref(), Some("Hello World"));
/// assert_eq!(song.lines.len(), 2);
/// ```
pub fn parse(input: &str) -> Result<Song, ParseError> {
    parse_with_options(input, &ParseOptions::default())
}

/// Options that control parser behavior.
#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// Maximum input size in bytes. Inputs exceeding this limit are rejected
    /// with a [`ParseError`] before lexing begins. Set to `0` to disable.
    ///
    /// Default: 10 MB (10 × 1024 × 1024 bytes).
    pub max_input_size: usize,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            max_input_size: 10 * 1024 * 1024, // 10 MB
        }
    }
}

/// Parses a ChordPro source string into a [`Song`] AST with custom options.
///
/// See [`parse`] for details. This variant allows configuring parser behavior
/// via [`ParseOptions`].
///
/// # Errors
///
/// Returns a [`ParseError`] if the input exceeds the configured size limit
/// or contains structural problems.
pub fn parse_with_options(input: &str, options: &ParseOptions) -> Result<Song, ParseError> {
    if options.max_input_size > 0 && input.len() > options.max_input_size {
        return Err(ParseError::new(
            format!(
                "input size ({} bytes) exceeds maximum ({} bytes)",
                input.len(),
                options.max_input_size
            ),
            Span::new(
                crate::token::Position::new(1, 1),
                crate::token::Position::new(1, 1),
            ),
        ));
    }
    let tokens = Lexer::new(input).tokenize();
    Parser::new(tokens).parse()
}

/// Parses a ChordPro source string leniently, collecting all errors.
///
/// Unlike [`parse`], this function does not fail on the first error.
/// It returns a [`ParseResult`] containing the partial AST and all
/// errors encountered. The size limit from [`ParseOptions::default`]
/// is enforced.
///
/// # Examples
///
/// ```
/// use chordpro_core::parser::parse_lenient;
///
/// let result = parse_lenient("{title: Test}\n{bad\n[G]Hello");
/// assert!(result.has_errors());
/// assert_eq!(result.song.metadata.title.as_deref(), Some("Test"));
/// // The valid lyrics line was still parsed.
/// assert!(result.song.lines.len() >= 2);
/// ```
pub fn parse_lenient(input: &str) -> ParseResult {
    parse_lenient_with_options(input, &ParseOptions::default())
}

/// Parses a ChordPro source string leniently with custom options.
///
/// See [`parse_lenient`] for details.
pub fn parse_lenient_with_options(input: &str, options: &ParseOptions) -> ParseResult {
    if options.max_input_size > 0 && input.len() > options.max_input_size {
        return ParseResult {
            song: Song::new(),
            errors: vec![ParseError::new(
                format!(
                    "input size ({} bytes) exceeds maximum ({} bytes)",
                    input.len(),
                    options.max_input_size
                ),
                Span::new(
                    crate::token::Position::new(1, 1),
                    crate::token::Position::new(1, 1),
                ),
            )],
        };
    }
    let tokens = Lexer::new(input).tokenize();
    Parser::new(tokens).parse_lenient()
}

// ---------------------------------------------------------------------------
// Multi-song result
// ---------------------------------------------------------------------------

/// The result of a lenient multi-song parse.
///
/// When using [`parse_multi_lenient`], the parser splits the input at `{new_song}`
/// boundaries and parses each segment independently. Each entry in `results`
/// contains the lenient parse result for one song segment.
#[derive(Debug, Clone)]
pub struct MultiParseResult {
    /// The parsed songs, one per segment between `{new_song}` boundaries.
    /// Each entry is the lenient parse result for that song segment.
    pub results: Vec<ParseResult>,
}

impl MultiParseResult {
    /// Returns all successfully parsed songs.
    #[must_use]
    pub fn songs(&self) -> Vec<&Song> {
        self.results.iter().map(|r| &r.song).collect()
    }

    /// Returns `true` if no errors were encountered in any song.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.results.iter().all(|r| r.is_ok())
    }

    /// Returns `true` if any errors were encountered in any song.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.results.iter().any(|r| r.has_errors())
    }

    /// Returns all errors from all songs.
    #[must_use]
    pub fn all_errors(&self) -> Vec<&ParseError> {
        self.results.iter().flat_map(|r| r.errors.iter()).collect()
    }
}

// ---------------------------------------------------------------------------
// Multi-song convenience functions
// ---------------------------------------------------------------------------

/// Checks whether a trimmed line is a `{new_song}` or `{ns}` directive.
fn is_new_song_line(trimmed: &str) -> bool {
    // Match patterns like {new_song}, { new_song }, {ns}, { ns }, case-insensitive
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return false;
    }
    let inner = trimmed[1..trimmed.len() - 1].trim().to_ascii_lowercase();
    inner == "new_song" || inner == "ns"
}

/// Splits input text at `{new_song}` / `{ns}` directive boundaries.
///
/// Returns a vector of string slices, where each element is the text of one
/// song. If the input contains no `{new_song}` directives, returns a
/// single-element vector containing the entire input.
fn split_at_new_song(input: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    let mut line_byte_start = 0;
    let bytes = input.as_bytes();

    for line in input.lines() {
        let line_byte_end = line_byte_start + line.len();
        let trimmed = line.trim();

        // Calculate the number of bytes used by the line terminator (\r\n or \n).
        let newline_len = if line_byte_end < bytes.len() && bytes[line_byte_end] == b'\r' {
            if line_byte_end + 1 < bytes.len() && bytes[line_byte_end + 1] == b'\n' {
                2
            } else {
                0
            }
        } else if line_byte_end < bytes.len() && bytes[line_byte_end] == b'\n' {
            1
        } else {
            0
        };

        if is_new_song_line(trimmed) {
            // Add everything before this line as a segment.
            segments.push(&input[start..line_byte_start]);
            // Skip past this line and its line terminator.
            start = line_byte_end + newline_len;
        }

        // Advance past this line and its line terminator.
        line_byte_start = line_byte_end + newline_len;
    }

    // Add the remaining text as the last segment.
    segments.push(&input[start..]);
    segments
}

/// Parses a multi-song ChordPro source string, splitting at `{new_song}` / `{ns}`
/// boundaries and parsing each segment as an independent [`Song`].
///
/// If the input contains no `{new_song}` directives, the result is a single-element
/// vector containing the entire input parsed as one song.
///
/// # Errors
///
/// Returns a [`ParseError`] if any song segment contains structural problems.
/// On error, parsing stops at the first problematic segment.
///
/// # Examples
///
/// use chordpro_core::parser::parse_multi;
///
/// let input = "{title: Song One}\nLyrics one\n{new_song}\n{title: Song Two}\nLyrics two";
/// let songs = parse_multi(input).unwrap();
/// assert_eq!(songs.len(), 2);
/// assert_eq!(songs[0].metadata.title.as_deref(), Some("Song One"));
/// assert_eq!(songs[1].metadata.title.as_deref(), Some("Song Two"));
/// ```
pub fn parse_multi(input: &str) -> Result<Vec<Song>, ParseError> {
    parse_multi_with_options(input, &ParseOptions::default())
}

/// Parses a multi-song ChordPro source string with custom options.
///
/// See [`parse_multi`] for details. This variant allows configuring parser
/// behavior via [`ParseOptions`]. The size limit applies to the entire input,
/// not individual song segments.
///
/// # Errors
///
/// Returns a [`ParseError`] if the input exceeds the configured size limit
/// or any song segment contains structural problems.
pub fn parse_multi_with_options(
    input: &str,
    options: &ParseOptions,
) -> Result<Vec<Song>, ParseError> {
    if options.max_input_size > 0 && input.len() > options.max_input_size {
        return Err(ParseError::new(
            format!(
                "input size ({} bytes) exceeds maximum ({} bytes)",
                input.len(),
                options.max_input_size
            ),
            Span::new(
                crate::token::Position::new(1, 1),
                crate::token::Position::new(1, 1),
            ),
        ));
    }

    let segments = split_at_new_song(input);
    let mut songs = Vec::with_capacity(segments.len());

    for segment in segments {
        let tokens = Lexer::new(segment).tokenize();
        let song = Parser::new(tokens).parse()?;
        songs.push(song);
    }

    Ok(songs)
}

/// Parses a multi-song ChordPro source string leniently, collecting all errors.
///
/// Unlike [`parse_multi`], this function does not fail on the first error.
/// Each song segment is parsed independently with [`parse_lenient`], and all
/// errors are collected. The size limit from [`ParseOptions::default`] is
/// enforced on the entire input.
///
/// # Examples
///
/// ```
/// use chordpro_core::parser::parse_multi_lenient;
///
/// let input = "{title: Song One}\n[Am\n{new_song}\n{title: Song Two}\n[G]Hello";
/// let result = parse_multi_lenient(input);
/// assert_eq!(result.results.len(), 2);
/// assert!(result.results[0].has_errors()); // unclosed chord
/// assert!(result.results[1].is_ok());
/// ```
pub fn parse_multi_lenient(input: &str) -> MultiParseResult {
    parse_multi_lenient_with_options(input, &ParseOptions::default())
}

/// Parses a multi-song ChordPro source string leniently with custom options.
///
/// See [`parse_multi_lenient`] for details.
pub fn parse_multi_lenient_with_options(input: &str, options: &ParseOptions) -> MultiParseResult {
    if options.max_input_size > 0 && input.len() > options.max_input_size {
        return MultiParseResult {
            results: vec![ParseResult {
                song: Song::new(),
                errors: vec![ParseError::new(
                    format!(
                        "input size ({} bytes) exceeds maximum ({} bytes)",
                        input.len(),
                        options.max_input_size
                    ),
                    Span::new(
                        crate::token::Position::new(1, 1),
                        crate::token::Position::new(1, 1),
                    ),
                )],
            }],
        };
    }

    let segments = split_at_new_song(input);
    let results: Vec<ParseResult> = segments
        .into_iter()
        .map(|segment| {
            let tokens = Lexer::new(segment).tokenize();
            Parser::new(tokens).parse_lenient()
        })
        .collect();

    MultiParseResult { results }
}

// ---------------------------------------------------------------------------
// Image attribute parsing
// ---------------------------------------------------------------------------

/// Parses the value string of an `{image}` directive into [`ImageAttributes`].
///
/// The value string is expected to contain `key=value` pairs separated by
/// whitespace. Quoted values (e.g., `title="Album Cover"`) are supported.
/// Unknown keys are silently ignored.
///
/// # Examples
///
/// ```
/// # use chordpro_core::parser::parse_image_attributes;
/// # use chordpro_core::ast::ImageAttributes;
/// let attrs = parse_image_attributes("src=photo.jpg width=200");
/// assert_eq!(attrs.src, "photo.jpg");
/// assert_eq!(attrs.width.as_deref(), Some("200"));
/// ```
#[must_use]
pub fn parse_image_attributes(input: &str) -> ImageAttributes {
    let mut attrs = ImageAttributes::default();
    let pairs = split_key_value_pairs(input);

    for (key, value) in pairs {
        match key.as_str() {
            "src" => attrs.src = value,
            "width" => attrs.width = Some(value),
            "height" => attrs.height = Some(value),
            "scale" => attrs.scale = Some(value),
            "title" => attrs.title = Some(value),
            "anchor" => attrs.anchor = Some(value),
            _ => {
                // Unknown attributes are silently ignored per spec.
            }
        }
    }

    attrs
}

/// Splits an input string into `(key, value)` pairs from `key=value` tokens.
///
/// Handles both unquoted values (`key=value`) and quoted values
/// (`key="value with spaces"`). Tokens without an `=` sign are ignored.
fn split_key_value_pairs(input: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip whitespace.
        while i < len && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        // Read key (up to '=' or whitespace).
        let key_start = i;
        while i < len && chars[i] != '=' && !chars[i].is_whitespace() {
            i += 1;
        }
        let key: String = chars[key_start..i].iter().collect();

        if i >= len || chars[i] != '=' {
            // No '=' found — skip this token.
            // Advance past any non-whitespace to avoid infinite loop.
            while i < len && !chars[i].is_whitespace() {
                i += 1;
            }
            continue;
        }

        // Skip '='.
        i += 1;

        // Read value (possibly quoted).
        let value = if i < len && chars[i] == '"' {
            // Quoted value: read until closing '"'.
            i += 1; // skip opening quote
            let val_start = i;
            while i < len && chars[i] != '"' {
                i += 1;
            }
            let val: String = chars[val_start..i].iter().collect();
            if i < len {
                i += 1; // skip closing quote
            }
            val
        } else {
            // Unquoted value: read until whitespace.
            let val_start = i;
            while i < len && !chars[i].is_whitespace() {
                i += 1;
            }
            chars[val_start..i].iter().collect()
        };

        if !key.is_empty() {
            pairs.push((key, value));
        }
    }

    pairs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        Chord, CommentStyle, Directive, DirectiveKind, Line, LyricsLine, LyricsSegment,
    };

    // -- Helper -------------------------------------------------------------

    /// Parses the input and returns the lines, panicking on error.
    fn lines(input: &str) -> Vec<Line> {
        parse(input).expect("parse failed").lines
    }

    // -- Input size limits (#60) -----------------------------------------------

    #[test]
    fn input_within_limit_succeeds() {
        let opts = ParseOptions {
            max_input_size: 100,
        };
        let result = parse_with_options("{title: Test}", &opts);
        assert!(result.is_ok());
    }

    #[test]
    fn input_exceeding_limit_fails() {
        let opts = ParseOptions { max_input_size: 10 };
        let result = parse_with_options("{title: This is too long}", &opts);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("exceeds maximum"));
    }

    #[test]
    fn zero_limit_disables_check() {
        let opts = ParseOptions { max_input_size: 0 };
        let result = parse_with_options("{title: Any size is fine}", &opts);
        assert!(result.is_ok());
    }

    #[test]
    fn default_limit_is_10mb() {
        let opts = ParseOptions::default();
        assert_eq!(opts.max_input_size, 10 * 1024 * 1024);
    }

    // -- Empty input --------------------------------------------------------

    #[test]
    fn empty_input() {
        let song = parse("").unwrap();
        assert!(song.lines.is_empty());
    }

    // -- Empty lines --------------------------------------------------------

    #[test]
    fn single_empty_line() {
        let result = lines("\n");
        assert_eq!(result, vec![Line::Empty]);
    }

    #[test]
    fn multiple_empty_lines() {
        let result = lines("\n\n\n");
        assert_eq!(result, vec![Line::Empty, Line::Empty, Line::Empty]);
    }

    // -- Plain text (lyrics without chords) ---------------------------------

    #[test]
    fn plain_text_line() {
        let result = lines("Hello world");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![LyricsSegment::text_only("Hello world")],
            })]
        );
    }

    #[test]
    fn multiple_plain_text_lines() {
        let result = lines("Line one\nLine two");
        assert_eq!(
            result,
            vec![
                Line::Lyrics(LyricsLine {
                    segments: vec![LyricsSegment::text_only("Line one")],
                }),
                Line::Lyrics(LyricsLine {
                    segments: vec![LyricsSegment::text_only("Line two")],
                }),
            ]
        );
    }

    // -- Chord annotations --------------------------------------------------

    #[test]
    fn single_chord_with_text() {
        let result = lines("[Am]Hello");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![LyricsSegment::new(Some(Chord::new("Am")), "Hello")],
            })]
        );
    }

    #[test]
    fn multiple_chords_with_text() {
        let result = lines("[Am]Hello [G]world");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![
                    LyricsSegment::new(Some(Chord::new("Am")), "Hello "),
                    LyricsSegment::new(Some(Chord::new("G")), "world"),
                ],
            })]
        );
    }

    #[test]
    fn chord_only_no_text() {
        let result = lines("[Am]");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![LyricsSegment::chord_only(Chord::new("Am"))],
            })]
        );
    }

    #[test]
    fn consecutive_chords_no_text_between() {
        let result = lines("[Am][G]");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![
                    LyricsSegment::chord_only(Chord::new("Am")),
                    LyricsSegment::chord_only(Chord::new("G")),
                ],
            })]
        );
    }

    #[test]
    fn text_before_first_chord() {
        let result = lines("Hello [Am]world");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![
                    LyricsSegment::text_only("Hello "),
                    LyricsSegment::new(Some(Chord::new("Am")), "world"),
                ],
            })]
        );
    }

    #[test]
    fn chord_at_end_of_line() {
        let result = lines("Hello [Am]");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![
                    LyricsSegment::text_only("Hello "),
                    LyricsSegment::chord_only(Chord::new("Am")),
                ],
            })]
        );
    }

    #[test]
    fn empty_chord_name() {
        // An empty chord `[]` is valid — chord name is an empty string.
        let result = lines("[]text");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![LyricsSegment::new(Some(Chord::new("")), "text")],
            })]
        );
    }

    // -- Directives ---------------------------------------------------------

    #[test]
    fn directive_with_value() {
        let result = lines("{title: My Song}");
        assert_eq!(
            result,
            vec![Line::Directive(Directive::with_value("title", "My Song"))],
        );
    }

    #[test]
    fn directive_without_value() {
        let result = lines("{start_of_chorus}");
        assert_eq!(
            result,
            vec![Line::Directive(Directive::name_only("start_of_chorus"))],
        );
    }

    #[test]
    fn directive_value_trimmed() {
        let result = lines("{title:  Hello World  }");
        assert_eq!(
            result,
            vec![Line::Directive(Directive::with_value(
                "title",
                "Hello World"
            ))],
        );
    }

    #[test]
    fn directive_name_trimmed() {
        let result = lines("{  title  : value}");
        assert_eq!(
            result,
            vec![Line::Directive(Directive::with_value("title", "value"))],
        );
    }

    #[test]
    fn directive_with_colon_in_value() {
        // The lexer emits multiple Colon tokens; the parser joins extra colons.
        let result = lines("{comment: time 10:30}");
        // This is the `comment` directive, so it becomes Line::Comment.
        assert_eq!(
            result,
            vec![Line::Comment(
                CommentStyle::Normal,
                "time 10:30".to_string()
            )]
        );
    }

    #[test]
    fn directive_followed_by_lyrics() {
        let result = lines("{title: Test}\n[Am]Hello");
        assert_eq!(
            result,
            vec![
                Line::Directive(Directive::with_value("title", "Test")),
                Line::Lyrics(LyricsLine {
                    segments: vec![LyricsSegment::new(Some(Chord::new("Am")), "Hello")],
                }),
            ]
        );
    }

    // -- Comment directive --------------------------------------------------

    #[test]
    fn comment_directive_full_name() {
        let result = lines("{comment: This is a comment}");
        assert_eq!(
            result,
            vec![Line::Comment(
                CommentStyle::Normal,
                "This is a comment".to_string()
            )],
        );
    }

    #[test]
    fn comment_directive_short_name() {
        let result = lines("{c: Short comment}");
        assert_eq!(
            result,
            vec![Line::Comment(
                CommentStyle::Normal,
                "Short comment".to_string()
            )],
        );
    }

    #[test]
    fn comment_directive_no_value() {
        let result = lines("{comment}");
        assert_eq!(
            result,
            vec![Line::Comment(CommentStyle::Normal, String::new())]
        );
    }

    #[test]
    fn comment_italic_directive() {
        let result = lines("{comment_italic: Softly}");
        assert_eq!(
            result,
            vec![Line::Comment(CommentStyle::Italic, "Softly".to_string())],
        );
    }

    #[test]
    fn comment_italic_short_name() {
        let result = lines("{ci: Softly}");
        assert_eq!(
            result,
            vec![Line::Comment(CommentStyle::Italic, "Softly".to_string())],
        );
    }

    #[test]
    fn comment_box_directive() {
        let result = lines("{comment_box: Important}");
        assert_eq!(
            result,
            vec![Line::Comment(CommentStyle::Boxed, "Important".to_string())],
        );
    }

    #[test]
    fn comment_box_short_name() {
        let result = lines("{cb: Important}");
        assert_eq!(
            result,
            vec![Line::Comment(CommentStyle::Boxed, "Important".to_string())],
        );
    }

    // -- Directive classification -------------------------------------------

    #[test]
    fn directive_short_alias_title() {
        let result = lines("{t: My Song}");
        let expected = Directive::with_value("title", "My Song");
        assert_eq!(result, vec![Line::Directive(expected)]);
    }

    #[test]
    fn directive_short_alias_subtitle() {
        let result = lines("{st: Alternate}");
        let expected = Directive::with_value("subtitle", "Alternate");
        assert_eq!(result, vec![Line::Directive(expected)]);
    }

    #[test]
    fn directive_short_alias_soc() {
        let result = lines("{soc}");
        let expected = Directive::name_only("start_of_chorus");
        assert_eq!(result, vec![Line::Directive(expected)]);
    }

    #[test]
    fn directive_short_alias_eoc() {
        let result = lines("{eoc}");
        let expected = Directive::name_only("end_of_chorus");
        assert_eq!(result, vec![Line::Directive(expected)]);
    }

    #[test]
    fn directive_case_insensitive() {
        let result = lines("{TITLE: Upper}");
        let expected = Directive::with_value("title", "Upper");
        assert_eq!(result, vec![Line::Directive(expected)]);
    }

    #[test]
    fn directive_mixed_case() {
        let result = lines("{Start_Of_Chorus}");
        let expected = Directive::name_only("start_of_chorus");
        assert_eq!(result, vec![Line::Directive(expected)]);
    }

    #[test]
    fn directive_unknown_preserved() {
        let result = lines("{my_custom: value}");
        assert_eq!(
            result,
            vec![Line::Directive(Directive {
                name: "my_custom".to_string(),
                value: Some("value".to_string()),
                kind: DirectiveKind::Unknown("my_custom".to_string()),
                selector: None,
            })],
        );
    }

    #[test]
    fn directive_kind_on_parsed_directive() {
        let song = parse("{title: Test}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::Title);
            assert_eq!(d.name, "title");
        } else {
            panic!("expected directive");
        }
    }

    // -- Environment directives (all variants) ------------------------------

    #[test]
    fn environment_directives_long_form() {
        let cases = vec![
            (
                "{start_of_chorus}",
                "start_of_chorus",
                DirectiveKind::StartOfChorus,
            ),
            (
                "{end_of_chorus}",
                "end_of_chorus",
                DirectiveKind::EndOfChorus,
            ),
            (
                "{start_of_verse}",
                "start_of_verse",
                DirectiveKind::StartOfVerse,
            ),
            ("{end_of_verse}", "end_of_verse", DirectiveKind::EndOfVerse),
            (
                "{start_of_bridge}",
                "start_of_bridge",
                DirectiveKind::StartOfBridge,
            ),
            (
                "{end_of_bridge}",
                "end_of_bridge",
                DirectiveKind::EndOfBridge,
            ),
            ("{start_of_tab}", "start_of_tab", DirectiveKind::StartOfTab),
            ("{end_of_tab}", "end_of_tab", DirectiveKind::EndOfTab),
        ];

        for (input, expected_name, expected_kind) in cases {
            let result = lines(input);
            if let Line::Directive(ref d) = result[0] {
                assert_eq!(d.name, expected_name, "failed for input: {input}");
                assert_eq!(d.kind, expected_kind, "failed for input: {input}");
            } else {
                panic!("expected directive for input: {input}");
            }
        }
    }

    #[test]
    fn environment_directives_short_form() {
        let cases = vec![
            ("{soc}", "start_of_chorus", DirectiveKind::StartOfChorus),
            ("{eoc}", "end_of_chorus", DirectiveKind::EndOfChorus),
            ("{sov}", "start_of_verse", DirectiveKind::StartOfVerse),
            ("{eov}", "end_of_verse", DirectiveKind::EndOfVerse),
            ("{sob}", "start_of_bridge", DirectiveKind::StartOfBridge),
            ("{eob}", "end_of_bridge", DirectiveKind::EndOfBridge),
            ("{sot}", "start_of_tab", DirectiveKind::StartOfTab),
            ("{eot}", "end_of_tab", DirectiveKind::EndOfTab),
        ];

        for (input, expected_name, expected_kind) in cases {
            let result = lines(input);
            if let Line::Directive(ref d) = result[0] {
                assert_eq!(d.name, expected_name, "failed for input: {input}");
                assert_eq!(d.kind, expected_kind, "failed for input: {input}");
            } else {
                panic!("expected directive for input: {input}");
            }
        }
    }

    // -- Metadata population ------------------------------------------------

    #[test]
    fn metadata_title_populated() {
        let song = parse("{title: Amazing Grace}").unwrap();
        assert_eq!(song.metadata.title.as_deref(), Some("Amazing Grace"));
    }

    #[test]
    fn metadata_title_via_short_alias() {
        let song = parse("{t: Amazing Grace}").unwrap();
        assert_eq!(song.metadata.title.as_deref(), Some("Amazing Grace"));
    }

    #[test]
    fn metadata_subtitle_populated() {
        let song = parse("{subtitle: How sweet}\n{st: The sound}").unwrap();
        assert_eq!(song.metadata.subtitles, vec!["How sweet", "The sound"]);
    }

    #[test]
    fn metadata_artist_populated() {
        let song = parse("{artist: John Newton}").unwrap();
        assert_eq!(song.metadata.artists, vec!["John Newton"]);
    }

    #[test]
    fn metadata_multiple_artists() {
        let song = parse("{artist: John}\n{artist: Jane}").unwrap();
        assert_eq!(song.metadata.artists, vec!["John", "Jane"]);
    }

    #[test]
    fn metadata_composer_populated() {
        let song = parse("{composer: Bach}").unwrap();
        assert_eq!(song.metadata.composers, vec!["Bach"]);
    }

    #[test]
    fn metadata_lyricist_populated() {
        let song = parse("{lyricist: Someone}").unwrap();
        assert_eq!(song.metadata.lyricists, vec!["Someone"]);
    }

    #[test]
    fn metadata_album_populated() {
        let song = parse("{album: Greatest Hits}").unwrap();
        assert_eq!(song.metadata.album.as_deref(), Some("Greatest Hits"));
    }

    #[test]
    fn metadata_year_populated() {
        let song = parse("{year: 1779}").unwrap();
        assert_eq!(song.metadata.year.as_deref(), Some("1779"));
    }

    #[test]
    fn metadata_key_populated() {
        let song = parse("{key: G}").unwrap();
        assert_eq!(song.metadata.key.as_deref(), Some("G"));
    }

    #[test]
    fn metadata_tempo_populated() {
        let song = parse("{tempo: 120}").unwrap();
        assert_eq!(song.metadata.tempo.as_deref(), Some("120"));
    }

    #[test]
    fn metadata_time_populated() {
        let song = parse("{time: 3/4}").unwrap();
        assert_eq!(song.metadata.time.as_deref(), Some("3/4"));
    }

    #[test]
    fn metadata_capo_populated() {
        let song = parse("{capo: 2}").unwrap();
        assert_eq!(song.metadata.capo.as_deref(), Some("2"));
    }

    #[test]
    fn metadata_case_insensitive() {
        let song = parse("{TITLE: Upper Case}").unwrap();
        assert_eq!(song.metadata.title.as_deref(), Some("Upper Case"));
    }

    #[test]
    fn metadata_not_populated_without_value() {
        let song = parse("{title}").unwrap();
        assert_eq!(song.metadata.title, None);
    }

    #[test]
    fn metadata_all_fields_populated() {
        let input = "\
{title: My Song}
{subtitle: A Sub}
{artist: An Artist}
{composer: A Composer}
{lyricist: A Lyricist}
{album: An Album}
{year: 2024}
{key: Am}
{tempo: 100}
{time: 4/4}
{capo: 3}";

        let song = parse(input).unwrap();
        assert_eq!(song.metadata.title.as_deref(), Some("My Song"));
        assert_eq!(song.metadata.subtitles, vec!["A Sub"]);
        assert_eq!(song.metadata.artists, vec!["An Artist"]);
        assert_eq!(song.metadata.composers, vec!["A Composer"]);
        assert_eq!(song.metadata.lyricists, vec!["A Lyricist"]);
        assert_eq!(song.metadata.album.as_deref(), Some("An Album"));
        assert_eq!(song.metadata.year.as_deref(), Some("2024"));
        assert_eq!(song.metadata.key.as_deref(), Some("Am"));
        assert_eq!(song.metadata.tempo.as_deref(), Some("100"));
        assert_eq!(song.metadata.time.as_deref(), Some("4/4"));
        assert_eq!(song.metadata.capo.as_deref(), Some("3"));
    }

    #[test]
    fn metadata_custom_populated_for_unknown_directive() {
        let song = parse("{x_my_custom: some value}").unwrap();
        assert_eq!(
            song.metadata.custom,
            vec![("x_my_custom".to_string(), "some value".to_string())]
        );
    }

    #[test]
    fn metadata_custom_multiple_unknown_directives() {
        let song = parse("{x_one: first}\n{x_two: second}").unwrap();
        assert_eq!(
            song.metadata.custom,
            vec![
                ("x_one".to_string(), "first".to_string()),
                ("x_two".to_string(), "second".to_string()),
            ]
        );
    }

    #[test]
    fn metadata_custom_not_populated_without_value() {
        let song = parse("{x_no_value}").unwrap();
        assert!(song.metadata.custom.is_empty());
    }

    #[test]
    fn metadata_custom_coexists_with_standard_metadata() {
        let input = "{title: My Song}\n{x_custom: custom value}";
        let song = parse(input).unwrap();
        assert_eq!(song.metadata.title.as_deref(), Some("My Song"));
        assert_eq!(
            song.metadata.custom,
            vec![("x_custom".to_string(), "custom value".to_string())]
        );
    }

    // -- Error cases --------------------------------------------------------

    #[test]
    fn unclosed_directive() {
        let err = parse("{title: oops").unwrap_err();
        assert!(
            err.message.contains("unclosed directive"),
            "error message was: {}",
            err.message
        );
    }

    #[test]
    fn unclosed_chord() {
        let err = parse("[Am").unwrap_err();
        assert!(
            err.message.contains("unclosed chord"),
            "error message was: {}",
            err.message
        );
    }

    #[test]
    fn empty_directive_name() {
        let err = parse("{}").unwrap_err();
        assert!(
            err.message.contains("empty directive name"),
            "error message was: {}",
            err.message
        );
    }

    #[test]
    fn empty_directive_with_colon() {
        let err = parse("{: value}").unwrap_err();
        assert!(
            err.message.contains("empty directive name"),
            "error message was: {}",
            err.message
        );
    }

    #[test]
    fn unclosed_chord_at_newline() {
        let err = parse("[Am\ntext").unwrap_err();
        assert!(
            err.message.contains("unclosed chord"),
            "error message was: {}",
            err.message
        );
    }

    #[test]
    fn parse_error_display() {
        let err = parse("{title: no close").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("parse error at line"));
        assert!(msg.contains("unclosed directive"));
    }

    // -- Mixed content / integration ----------------------------------------

    #[test]
    fn full_song() {
        let input = "\
{title: Amazing Grace}
{artist: John Newton}

[G]Amazing [G7]grace, how [C]sweet the [G]sound
[G]That saved a [Em]wretch like [D]me";

        let song = parse(input).unwrap();
        assert_eq!(song.lines.len(), 5);

        // Metadata populated
        assert_eq!(song.metadata.title.as_deref(), Some("Amazing Grace"));
        assert_eq!(song.metadata.artists, vec!["John Newton"]);

        // First line: title directive
        assert_eq!(
            song.lines[0],
            Line::Directive(Directive::with_value("title", "Amazing Grace")),
        );

        // Second line: artist directive
        assert_eq!(
            song.lines[1],
            Line::Directive(Directive::with_value("artist", "John Newton")),
        );

        // Third line: empty
        assert_eq!(song.lines[2], Line::Empty);

        // Fourth line: lyrics with chords
        if let Line::Lyrics(ref lyrics) = song.lines[3] {
            assert_eq!(lyrics.text(), "Amazing grace, how sweet the sound");
            assert!(lyrics.has_chords());
            assert_eq!(lyrics.segments.len(), 4);
            assert_eq!(lyrics.segments[0].chord.as_ref().unwrap().name, "G");
            assert_eq!(lyrics.segments[0].text, "Amazing ");
            assert_eq!(lyrics.segments[1].chord.as_ref().unwrap().name, "G7");
            assert_eq!(lyrics.segments[1].text, "grace, how ");
            assert_eq!(lyrics.segments[2].chord.as_ref().unwrap().name, "C");
            assert_eq!(lyrics.segments[2].text, "sweet the ");
            assert_eq!(lyrics.segments[3].chord.as_ref().unwrap().name, "G");
            assert_eq!(lyrics.segments[3].text, "sound");
        } else {
            panic!("expected Line::Lyrics for line 4");
        }

        // Fifth line: lyrics with chords
        if let Line::Lyrics(ref lyrics) = song.lines[4] {
            assert_eq!(lyrics.text(), "That saved a wretch like me");
            assert_eq!(lyrics.segments.len(), 3);
        } else {
            panic!("expected Line::Lyrics for line 5");
        }
    }

    #[test]
    fn song_with_sections() {
        let input = "\
{start_of_chorus}
[C]La la [G]la
{end_of_chorus}";

        let song = parse(input).unwrap();
        assert_eq!(song.lines.len(), 3);
        assert!(matches!(song.lines[0], Line::Directive(_)));
        assert!(matches!(song.lines[1], Line::Lyrics(_)));
        assert!(matches!(song.lines[2], Line::Directive(_)));
    }

    #[test]
    fn song_with_comments_and_empty_lines() {
        let input = "\
{title: Test}
{comment: Intro}

[Am]Hello
";

        let song = parse(input).unwrap();
        assert_eq!(song.lines.len(), 4);
        assert_eq!(
            song.lines[0],
            Line::Directive(Directive::with_value("title", "Test"))
        );
        assert_eq!(
            song.lines[1],
            Line::Comment(CommentStyle::Normal, "Intro".to_string())
        );
        assert_eq!(song.lines[2], Line::Empty);
        assert!(matches!(song.lines[3], Line::Lyrics(_)));
    }

    #[test]
    fn crlf_line_endings() {
        let input = "{title: Test}\r\n[Am]Hello\r\n";
        let song = parse(input).unwrap();
        assert_eq!(song.lines.len(), 2);
        assert_eq!(
            song.lines[0],
            Line::Directive(Directive::with_value("title", "Test")),
        );
        assert!(matches!(song.lines[1], Line::Lyrics(_)));
    }

    #[test]
    fn stray_close_brace_in_lyrics() {
        // A stray `}` outside a directive is treated as literal text.
        let result = lines("hello } world");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![LyricsSegment::text_only("hello } world")],
            })]
        );
    }

    #[test]
    fn stray_close_bracket_in_lyrics() {
        // A stray `]` outside a chord is treated as literal text.
        let result = lines("hello ] world");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![LyricsSegment::text_only("hello ] world")],
            })]
        );
    }

    #[test]
    fn unicode_in_chords_and_lyrics() {
        let result = lines("[Am]こんにちは [G]世界");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![
                    LyricsSegment::new(Some(Chord::new("Am")), "こんにちは "),
                    LyricsSegment::new(Some(Chord::new("G")), "世界"),
                ],
            })]
        );
    }

    #[test]
    fn multiple_colons_in_directive_value() {
        // Extra colons after the first are treated as part of the value.
        // When the directive is "meta", it is parsed as a Meta directive.
        // Since "key:value:extra" has no whitespace, the whole string
        // becomes the meta key with no value.
        let result = lines("{meta: key:value:extra}");
        assert_eq!(
            result,
            vec![Line::Directive(Directive {
                name: "meta".to_string(),
                value: None,
                kind: DirectiveKind::Meta("key:value:extra".to_string()),
                selector: None,
            })],
        );

        // For non-meta directives, extra colons remain in the value.
        let result = lines("{custom_dir: key:value:extra}");
        assert_eq!(
            result,
            vec![Line::Directive(Directive {
                name: "custom_dir".to_string(),
                value: Some("key:value:extra".to_string()),
                kind: DirectiveKind::Unknown("custom_dir".to_string()),
                selector: None,
            })],
        );
    }

    #[test]
    fn directive_only_whitespace_name() {
        let err = parse("{   }").unwrap_err();
        assert!(
            err.message.contains("empty directive name"),
            "error message was: {}",
            err.message
        );
    }

    #[test]
    fn directive_with_brackets_in_value() {
        // Brackets inside a directive value are included literally.
        let result = lines("{comment: play [Am] here}");
        assert_eq!(
            result,
            vec![Line::Comment(
                CommentStyle::Normal,
                "play [Am] here".to_string()
            )],
        );
    }

    #[test]
    fn chord_line_with_spaces() {
        let result = lines("[Am]  [G]  [C]");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![
                    LyricsSegment::new(Some(Chord::new("Am")), "  "),
                    LyricsSegment::new(Some(Chord::new("G")), "  "),
                    LyricsSegment::chord_only(Chord::new("C")),
                ],
            })]
        );
    }

    #[test]
    fn trailing_newline_produces_empty_line() {
        let result = lines("text\n");
        assert_eq!(
            result,
            vec![Line::Lyrics(LyricsLine {
                segments: vec![LyricsSegment::text_only("text")],
            })]
        );
    }

    #[test]
    fn parser_struct_directly() {
        // Test using Parser::new directly with tokens.
        let tokens = Lexer::new("[C]Hello").tokenize();
        let song = Parser::new(tokens).parse().unwrap();
        assert_eq!(song.lines.len(), 1);
    }

    // -- Full song with all directive types ---------------------------------

    #[test]
    fn full_song_with_all_directive_types() {
        let input = "\
{t: Amazing Grace}
{st: A Hymn}
{artist: John Newton}
{key: G}
{tempo: 80}
{time: 3/4}
{capo: 2}
{comment: Verse 1}
{ci: Play softly}
{cb: Key change ahead}
{soc}
[G]Amazing [G7]grace
{eoc}";

        let song = parse(input).unwrap();

        // Metadata checks
        assert_eq!(song.metadata.title.as_deref(), Some("Amazing Grace"));
        assert_eq!(song.metadata.subtitles, vec!["A Hymn"]);
        assert_eq!(song.metadata.artists, vec!["John Newton"]);
        assert_eq!(song.metadata.key.as_deref(), Some("G"));
        assert_eq!(song.metadata.tempo.as_deref(), Some("80"));
        assert_eq!(song.metadata.time.as_deref(), Some("3/4"));
        assert_eq!(song.metadata.capo.as_deref(), Some("2"));

        // Line type checks
        assert_eq!(song.lines.len(), 13);
        assert!(matches!(song.lines[0], Line::Directive(_))); // title
        assert!(matches!(song.lines[1], Line::Directive(_))); // subtitle
        assert!(matches!(song.lines[2], Line::Directive(_))); // artist
        assert!(matches!(song.lines[3], Line::Directive(_))); // key
        assert!(matches!(song.lines[4], Line::Directive(_))); // tempo
        assert!(matches!(song.lines[5], Line::Directive(_))); // time
        assert!(matches!(song.lines[6], Line::Directive(_))); // capo
        assert_eq!(
            song.lines[7],
            Line::Comment(CommentStyle::Normal, "Verse 1".to_string())
        );
        assert_eq!(
            song.lines[8],
            Line::Comment(CommentStyle::Italic, "Play softly".to_string())
        );
        assert_eq!(
            song.lines[9],
            Line::Comment(CommentStyle::Boxed, "Key change ahead".to_string())
        );
        // soc
        if let Line::Directive(ref d) = song.lines[10] {
            assert_eq!(d.kind, DirectiveKind::StartOfChorus);
            assert_eq!(d.name, "start_of_chorus");
        } else {
            panic!("expected directive");
        }
        assert!(matches!(song.lines[11], Line::Lyrics(_))); // lyrics
        // eoc
        if let Line::Directive(ref d) = song.lines[12] {
            assert_eq!(d.kind, DirectiveKind::EndOfChorus);
            assert_eq!(d.name, "end_of_chorus");
        } else {
            panic!("expected directive");
        }
    }

    // -- Error diagnostics (issue #25) --------------------------------------

    #[test]
    fn parse_error_implements_std_error() {
        let err = parse("[Am").unwrap_err();
        // Verify that ParseError can be used as a std::error::Error trait object.
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn parse_error_source_is_none() {
        let err = parse("[Am").unwrap_err();
        let err_ref: &dyn std::error::Error = &err;
        assert!(err_ref.source().is_none());
    }

    #[test]
    fn parse_error_line_column_accessors() {
        let err = parse("[Am").unwrap_err();
        assert_eq!(err.line(), 1);
        assert_eq!(err.column(), 1);
    }

    #[test]
    fn unclosed_chord_error_location() {
        let err = parse("[Am").unwrap_err();
        assert!(err.message.contains("unclosed chord"));
        assert_eq!(err.span.start.line, 1);
        assert_eq!(err.span.start.column, 1);
    }

    #[test]
    fn unclosed_chord_on_second_line() {
        let err = parse("Hello\n[Am").unwrap_err();
        assert!(err.message.contains("unclosed chord"));
        assert_eq!(err.span.start.line, 2);
        assert_eq!(err.span.start.column, 1);
    }

    #[test]
    fn unclosed_chord_mid_line() {
        let err = parse("text [Am").unwrap_err();
        assert!(err.message.contains("unclosed chord"));
        assert_eq!(err.span.start.line, 1);
        assert_eq!(err.span.start.column, 6);
    }

    #[test]
    fn unclosed_directive_error_location() {
        let err = parse("{title: oops").unwrap_err();
        assert!(err.message.contains("unclosed directive"));
        // Span points to EOF where the closing brace was expected.
        assert_eq!(err.span.start.line, 1);
        assert_eq!(err.span.start.column, 13);
    }

    #[test]
    fn unclosed_directive_on_third_line() {
        let err = parse("line one\nline two\n{title: oops").unwrap_err();
        assert!(err.message.contains("unclosed directive"));
        // Span points to EOF where the closing brace was expected.
        assert_eq!(err.span.start.line, 3);
        assert_eq!(err.span.start.column, 13);
    }

    #[test]
    fn empty_directive_error_location() {
        let err = parse("{}").unwrap_err();
        assert!(err.message.contains("empty directive name"));
        assert_eq!(err.span.start.line, 1);
        assert_eq!(err.span.start.column, 1);
    }

    #[test]
    fn empty_directive_with_colon_error_location() {
        let err = parse("{: value}").unwrap_err();
        assert!(err.message.contains("empty directive name"));
        assert_eq!(err.span.start.line, 1);
        assert_eq!(err.span.start.column, 1);
    }

    #[test]
    fn error_display_format_with_line_column() {
        let err = parse("first line\n{title: no close").unwrap_err();
        let msg = format!("{err}");
        // The error reports the position where the closing brace was expected.
        assert!(
            msg.starts_with("parse error at line 2, column 17:"),
            "unexpected display format: {msg}"
        );
    }

    #[test]
    fn unclosed_chord_at_end_of_line_error_location() {
        // [Am at end followed by newline — error points to the opening bracket
        let err = parse("[Am\nmore text").unwrap_err();
        assert!(err.message.contains("unclosed chord"));
        assert_eq!(err.span.start.line, 1);
        assert_eq!(err.span.start.column, 1);
    }

    #[test]
    fn unclosed_directive_at_eof_error_location() {
        let err = parse("{title").unwrap_err();
        assert!(err.message.contains("unclosed directive"));
        assert_eq!(err.span.start.line, 1);
        assert_eq!(err.span.start.column, 1);
    }

    #[test]
    fn whitespace_only_directive_name_error_location() {
        let err = parse("{   : value}").unwrap_err();
        assert!(err.message.contains("empty directive name"));
        assert_eq!(err.span.start.line, 1);
        assert_eq!(err.span.start.column, 1);
    }

    #[test]
    fn error_after_valid_content() {
        // Valid content followed by an error on a later line
        let input = "{title: Test}\n[Am]Hello\n[G";
        let err = parse(input).unwrap_err();
        assert!(err.message.contains("unclosed chord"));
        assert_eq!(err.span.start.line, 3);
        assert_eq!(err.span.start.column, 1);
    }

    #[test]
    fn multiple_errors_first_is_reported() {
        // Parser stops at first error — verify it's the correct one.
        let err = parse("{title\n{another").unwrap_err();
        assert!(err.message.contains("unclosed directive"));
        assert_eq!(err.span.start.line, 1);
    }

    // --- Tab verbatim (#59) ---

    #[test]
    fn tab_content_is_verbatim() {
        // Brackets inside tab should NOT be parsed as chords.
        let song = parse("{start_of_tab}\ne|---[0]---|\n{end_of_tab}").unwrap();
        // Line 0: start_of_tab directive
        // Line 1: verbatim text line
        // Line 2: end_of_tab directive
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert_eq!(l.segments.len(), 1);
            assert!(l.segments[0].chord.is_none());
            assert_eq!(l.segments[0].text, "e|---[0]---|");
        } else {
            panic!("expected lyrics line for tab content");
        }
    }

    #[test]
    fn tab_content_preserves_braces() {
        let song = parse("{sot}\n{some text}\n{eot}").unwrap();
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert_eq!(l.segments[0].text, "{some text}");
        } else {
            panic!("expected lyrics line for tab content");
        }
    }

    #[test]
    fn chords_parsed_after_tab_ends() {
        // After end_of_tab, chord parsing should resume.
        let song = parse("{sot}\ne|---|\n{eot}\n[Am]Hello").unwrap();
        // Line 3 should be a lyrics line with a chord.
        if let Line::Lyrics(ref l) = song.lines[3] {
            assert!(l.segments[0].chord.is_some());
            assert_eq!(l.segments[0].chord.as_ref().unwrap().name, "Am");
        } else {
            panic!("expected lyrics line with chord after tab section");
        }
    }

    // --- Grid verbatim (#107) ---

    #[test]
    fn grid_content_is_verbatim() {
        // Brackets inside grid should NOT be parsed as chords.
        let song = parse("{start_of_grid}\n| [Am] . | [C] . |\n{end_of_grid}").unwrap();
        // Line 0: start_of_grid directive
        // Line 1: verbatim text line
        // Line 2: end_of_grid directive
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert_eq!(l.segments.len(), 1);
            assert!(l.segments[0].chord.is_none());
            assert_eq!(l.segments[0].text, "| [Am] . | [C] . |");
        } else {
            panic!("expected lyrics line for grid content");
        }
    }

    #[test]
    fn grid_content_preserves_braces() {
        let song = parse("{sog}\n{some text}\n{eog}").unwrap();
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert_eq!(l.segments[0].text, "{some text}");
        } else {
            panic!("expected lyrics line for grid content");
        }
    }

    #[test]
    fn chords_parsed_after_grid_ends() {
        // After end_of_grid, chord parsing should resume.
        let song = parse("{sog}\n| Am . |\n{eog}\n[Am]Hello").unwrap();
        // Line 3 should be a lyrics line with a chord.
        if let Line::Lyrics(ref l) = song.lines[3] {
            assert!(l.segments[0].chord.is_some());
            assert_eq!(l.segments[0].chord.as_ref().unwrap().name, "Am");
        } else {
            panic!("expected lyrics line with chord after grid section");
        }
    }

    #[test]
    fn grid_short_aliases_sog_eog() {
        let song = parse("{sog}\n| Am |\n{eog}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::StartOfGrid);
            assert_eq!(d.name, "start_of_grid");
        } else {
            panic!("expected start_of_grid directive");
        }
        if let Line::Directive(ref d) = song.lines[2] {
            assert_eq!(d.kind, DirectiveKind::EndOfGrid);
            assert_eq!(d.name, "end_of_grid");
        } else {
            panic!("expected end_of_grid directive");
        }
    }

    #[test]
    fn grid_with_label() {
        let song = parse("{start_of_grid: Intro}\n| Am . | C . |\n{end_of_grid}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::StartOfGrid);
            assert_eq!(d.value.as_deref(), Some("Intro"));
        } else {
            panic!("expected start_of_grid directive with label");
        }
    }

    // --- Define directive (#37) ---

    #[test]
    fn define_directive_parsed() {
        let song = parse("{define: Asus4 base-fret 1 frets x 0 2 2 3 0}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::Define);
            assert_eq!(d.name, "define");
            assert_eq!(
                d.value.as_deref(),
                Some("Asus4 base-fret 1 frets x 0 2 2 3 0")
            );
        } else {
            panic!("expected define directive");
        }
    }

    #[test]
    fn chord_directive_parsed() {
        let song = parse("{chord: Asus4}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::ChordDirective);
            assert_eq!(d.value.as_deref(), Some("Asus4"));
        } else {
            panic!("expected chord directive");
        }
    }

    #[test]
    fn page_control_directives_long_form() {
        let song = parse("{new_page}\n{new_physical_page}\n{column_break}\n{columns: 2}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::NewPage);
            assert_eq!(d.name, "new_page");
            assert!(d.value.is_none());
        } else {
            panic!("expected new_page directive");
        }
        if let Line::Directive(ref d) = song.lines[1] {
            assert_eq!(d.kind, DirectiveKind::NewPhysicalPage);
            assert_eq!(d.name, "new_physical_page");
            assert!(d.value.is_none());
        } else {
            panic!("expected new_physical_page directive");
        }
        if let Line::Directive(ref d) = song.lines[2] {
            assert_eq!(d.kind, DirectiveKind::ColumnBreak);
            assert_eq!(d.name, "column_break");
            assert!(d.value.is_none());
        } else {
            panic!("expected column_break directive");
        }
        if let Line::Directive(ref d) = song.lines[3] {
            assert_eq!(d.kind, DirectiveKind::Columns);
            assert_eq!(d.name, "columns");
            assert_eq!(d.value.as_deref(), Some("2"));
        } else {
            panic!("expected columns directive");
        }
    }

    #[test]
    fn page_control_directives_short_form() {
        let song = parse("{np}\n{npp}\n{colb}\n{col: 3}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::NewPage);
            assert_eq!(d.name, "new_page");
        } else {
            panic!("expected new_page directive");
        }
        if let Line::Directive(ref d) = song.lines[1] {
            assert_eq!(d.kind, DirectiveKind::NewPhysicalPage);
            assert_eq!(d.name, "new_physical_page");
        } else {
            panic!("expected new_physical_page directive");
        }
        if let Line::Directive(ref d) = song.lines[2] {
            assert_eq!(d.kind, DirectiveKind::ColumnBreak);
            assert_eq!(d.name, "column_break");
        } else {
            panic!("expected column_break directive");
        }
        if let Line::Directive(ref d) = song.lines[3] {
            assert_eq!(d.kind, DirectiveKind::Columns);
            assert_eq!(d.name, "columns");
            assert_eq!(d.value.as_deref(), Some("3"));
        } else {
            panic!("expected columns directive");
        }
    }

    #[test]
    fn page_control_not_metadata() {
        let song = parse("{new_page}\n{columns: 2}").unwrap();
        // Page control directives should not populate metadata
        assert!(song.metadata.title.is_none());
        assert!(song.metadata.custom.is_empty());
    }

    // --- Lenient parsing / multi-error (#61) ---

    #[test]
    fn parse_lenient_no_errors() {
        let result = parse_lenient("{title: Test}\n[Am]Hello");
        assert!(result.is_ok());
        assert!(!result.has_errors());
        assert_eq!(result.song.metadata.title.as_deref(), Some("Test"));
        assert_eq!(result.song.lines.len(), 2);
    }

    #[test]
    fn parse_lenient_collects_multiple_errors() {
        // Two errors: unclosed directive on line 1, unclosed chord on line 3
        let result = parse_lenient("{title\nHello world\n[Am");
        assert!(result.has_errors());
        assert_eq!(result.errors.len(), 2);
        // The valid lyrics line in the middle should still be present.
        assert!(result.song.lines.iter().any(|l| {
            if let Line::Lyrics(ll) = l {
                ll.text() == "Hello world"
            } else {
                false
            }
        }));
    }

    #[test]
    fn parse_lenient_partial_ast_with_metadata() {
        // Title parses successfully, then an error, then more content.
        let result = parse_lenient("{title: My Song}\n{bad\n[G]La la");
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.song.metadata.title.as_deref(), Some("My Song"));
        // Title directive + skipped error line + lyrics = at least 2 lines
        assert!(result.song.lines.len() >= 2);
    }

    #[test]
    fn parse_lenient_all_lines_bad() {
        let result = parse_lenient("{unclosed\n[bad");
        assert_eq!(result.errors.len(), 2);
        assert!(result.song.lines.is_empty());
    }

    #[test]
    fn parse_lenient_error_locations() {
        let result = parse_lenient("{ok: fine}\n{bad\n[Am]Good\n{also bad");
        assert_eq!(result.errors.len(), 2);
        assert_eq!(result.errors[0].line(), 2);
        assert_eq!(result.errors[1].line(), 4);
    }

    #[test]
    fn parse_lenient_empty_input() {
        let result = parse_lenient("");
        assert!(result.is_ok());
        assert!(result.song.lines.is_empty());
    }

    #[test]
    fn parse_lenient_size_limit() {
        let opts = ParseOptions { max_input_size: 10 };
        let result = parse_lenient_with_options("this input is too long", &opts);
        assert!(result.has_errors());
        assert_eq!(result.errors.len(), 1);
        assert!(result.errors[0].message.contains("exceeds maximum"));
    }

    #[test]
    fn transpose_directive_parsed() {
        let song = parse("{transpose: 2}").expect("parse failed");
        assert_eq!(song.lines.len(), 1);
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::Transpose);
            assert_eq!(d.name, "transpose");
            assert_eq!(d.value.as_deref(), Some("2"));
        } else {
            panic!("expected transpose directive");
        }
    }

    #[test]
    fn transpose_directive_negative_value() {
        let song = parse("{transpose: -3}").expect("parse failed");
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::Transpose);
            assert_eq!(d.value.as_deref(), Some("-3"));
        } else {
            panic!("expected transpose directive");
        }
    }

    #[test]
    fn transpose_directive_no_value() {
        let song = parse("{transpose}").expect("parse failed");
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::Transpose);
            assert!(d.value.is_none());
        } else {
            panic!("expected transpose directive");
        }
    }

    #[test]
    fn transpose_directive_is_not_metadata() {
        let kind = DirectiveKind::Transpose;
        assert!(!kind.is_metadata());
    }

    #[test]
    fn transpose_directive_case_insensitive() {
        let song = parse("{Transpose: 5}").expect("parse failed");
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.kind, DirectiveKind::Transpose);
            assert_eq!(d.name, "transpose");
            assert_eq!(d.value.as_deref(), Some("5"));
        } else {
            panic!("expected transpose directive");
        }
    }

    // -- Custom section directives (#108) -----------------------------------

    #[test]
    fn custom_section_start_parsed() {
        let result = lines("{start_of_intro}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "start_of_intro");
            assert_eq!(d.kind, DirectiveKind::StartOfSection("intro".to_string()));
            assert!(d.is_section_start());
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn custom_section_end_parsed() {
        let result = lines("{end_of_intro}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "end_of_intro");
            assert_eq!(d.kind, DirectiveKind::EndOfSection("intro".to_string()));
            assert!(d.is_section_end());
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn custom_section_with_label() {
        let result = lines("{start_of_intro: Guitar Intro}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "start_of_intro");
            assert_eq!(d.value.as_deref(), Some("Guitar Intro"));
            assert_eq!(d.kind, DirectiveKind::StartOfSection("intro".to_string()));
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn custom_section_lyrics_parsed_normally() {
        let song = parse("{start_of_intro}\n[Am]Hello [G]world\n{end_of_intro}").unwrap();
        // Lines: start_of_intro, lyrics, end_of_intro
        assert_eq!(song.lines.len(), 3);
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert!(l.has_chords());
            assert_eq!(l.segments.len(), 2);
            assert_eq!(l.segments[0].chord.as_ref().unwrap().name, "Am");
        } else {
            panic!("expected lyrics line inside custom section");
        }
    }

    #[test]
    fn custom_section_various_names() {
        for name in &["outro", "solo", "interlude", "coda", "pre_chorus"] {
            let input = format!("{{start_of_{name}}}");
            let result = lines(&input);
            if let Line::Directive(ref d) = result[0] {
                assert_eq!(d.name, format!("start_of_{name}"));
                assert!(d.is_section_start(), "should be section start for {name}");
            } else {
                panic!("expected directive for {name}");
            }
        }
    }

    // -- Inline markup in lyrics -------------------------------------------

    #[test]
    fn lyrics_with_bold_markup() {
        use crate::inline_markup::TextSpan;

        let result = lines("[Am]Hello <b>world</b>");
        match &result[0] {
            Line::Lyrics(lyrics) => {
                assert_eq!(lyrics.segments.len(), 1);
                let seg = &lyrics.segments[0];
                assert_eq!(seg.text, "Hello world");
                assert_eq!(
                    seg.spans,
                    vec![
                        TextSpan::Plain("Hello ".to_string()),
                        TextSpan::Bold(vec![TextSpan::Plain("world".to_string())]),
                    ]
                );
            }
            _ => panic!("expected lyrics line"),
        }
    }

    #[test]
    fn custom_section_case_insensitive() {
        let result = lines("{Start_Of_Intro}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "start_of_intro");
            assert_eq!(d.kind, DirectiveKind::StartOfSection("intro".to_string()));
        } else {
            panic!("expected directive");
        }
    }

    // --- Image directive (#124) ---

    #[test]
    fn image_directive_basic() {
        let song = parse("{image: src=photo.jpg}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.name, "image");
            if let DirectiveKind::Image(ref attrs) = d.kind {
                assert_eq!(attrs.src, "photo.jpg");
                assert!(attrs.width.is_none());
                assert!(attrs.height.is_none());
                assert!(attrs.scale.is_none());
                assert!(attrs.title.is_none());
                assert!(attrs.anchor.is_none());
            } else {
                panic!("expected Image directive kind");
            }
        } else {
            panic!("expected directive");
        }
    }

    // --- Inline markup (#112) ---

    #[test]
    fn lyrics_without_markup_has_empty_spans() {
        let result = lines("[Am]Hello world");
        match &result[0] {
            Line::Lyrics(lyrics) => {
                assert_eq!(lyrics.segments[0].text, "Hello world");
                assert!(lyrics.segments[0].spans.is_empty());
            }
            _ => panic!("expected lyrics line"),
        }
    }

    #[test]
    fn image_directive_all_attributes() {
        let song =
            parse(r#"{image: src=logo.png width=200 height=100 scale=0.5 title="Album Cover" anchor=top}"#)
                .unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            if let DirectiveKind::Image(ref attrs) = d.kind {
                assert_eq!(attrs.src, "logo.png");
                assert_eq!(attrs.width.as_deref(), Some("200"));
                assert_eq!(attrs.height.as_deref(), Some("100"));
                assert_eq!(attrs.scale.as_deref(), Some("0.5"));
                assert_eq!(attrs.title.as_deref(), Some("Album Cover"));
                assert_eq!(attrs.anchor.as_deref(), Some("top"));
            } else {
                panic!("expected Image directive kind");
            }
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn lyrics_with_nested_markup() {
        use crate::inline_markup::TextSpan;

        let result = lines("<b><i>both</i></b>");
        match &result[0] {
            Line::Lyrics(lyrics) => {
                assert_eq!(lyrics.segments[0].text, "both");
                assert_eq!(
                    lyrics.segments[0].spans,
                    vec![TextSpan::Bold(vec![TextSpan::Italic(vec![
                        TextSpan::Plain("both".to_string())
                    ])])]
                );
            }
            _ => panic!("expected lyrics line"),
        }
    }

    #[test]
    fn image_directive_quoted_value_with_spaces() {
        let song = parse(r#"{image: src=cover.jpg title="My Great Album"}"#).unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            if let DirectiveKind::Image(ref attrs) = d.kind {
                assert_eq!(attrs.src, "cover.jpg");
                assert_eq!(attrs.title.as_deref(), Some("My Great Album"));
            } else {
                panic!("expected Image directive kind");
            }
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn lyrics_markup_text_field_has_stripped_content() {
        let result = lines("<b>bold</b> and <i>italic</i> text");
        match &result[0] {
            Line::Lyrics(lyrics) => {
                // text field should have markup stripped
                assert_eq!(lyrics.segments[0].text, "bold and italic text");
                // spans should be populated
                assert!(!lyrics.segments[0].spans.is_empty());
            }
            _ => panic!("expected lyrics line"),
        }
    }

    #[test]
    fn image_directive_no_value() {
        let song = parse("{image}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.name, "image");
            if let DirectiveKind::Image(ref attrs) = d.kind {
                assert_eq!(attrs.src, "");
            } else {
                panic!("expected Image directive kind");
            }
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn image_directive_unknown_attributes_ignored() {
        let song = parse("{image: src=pic.jpg unknown=foo bar}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            if let DirectiveKind::Image(ref attrs) = d.kind {
                assert_eq!(attrs.src, "pic.jpg");
                // unknown attribute is silently ignored
            } else {
                panic!("expected Image directive kind");
            }
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn image_directive_case_insensitive() {
        let song = parse("{IMAGE: src=photo.jpg}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            assert_eq!(d.name, "image");
            assert!(d.kind.is_image());
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn image_directive_width_only() {
        let song = parse("{image: src=img.png width=50%}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            if let DirectiveKind::Image(ref attrs) = d.kind {
                assert_eq!(attrs.src, "img.png");
                assert_eq!(attrs.width.as_deref(), Some("50%"));
                assert!(attrs.height.is_none());
            } else {
                panic!("expected Image directive kind");
            }
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn image_directive_preserves_raw_value() {
        let song = parse("{image: src=photo.jpg width=200}").unwrap();
        if let Line::Directive(ref d) = song.lines[0] {
            // The raw value string is preserved.
            assert_eq!(d.value.as_deref(), Some("src=photo.jpg width=200"));
        } else {
            panic!("expected directive");
        }
    }

    // --- parse_image_attributes unit tests ---

    #[test]
    fn parse_image_attributes_empty_input() {
        let attrs = super::parse_image_attributes("");
        assert_eq!(attrs.src, "");
        assert!(attrs.width.is_none());
    }

    #[test]
    fn parse_image_attributes_src_only() {
        let attrs = super::parse_image_attributes("src=test.png");
        assert_eq!(attrs.src, "test.png");
    }

    #[test]
    fn parse_image_attributes_multiple() {
        let attrs = super::parse_image_attributes("src=a.jpg width=100 height=200");
        assert_eq!(attrs.src, "a.jpg");
        assert_eq!(attrs.width.as_deref(), Some("100"));
        assert_eq!(attrs.height.as_deref(), Some("200"));
    }

    #[test]
    fn parse_image_attributes_quoted_value() {
        let attrs = super::parse_image_attributes(r#"src=a.jpg title="Hello World""#);
        assert_eq!(attrs.src, "a.jpg");
        assert_eq!(attrs.title.as_deref(), Some("Hello World"));
    }

    #[test]
    fn parse_image_attributes_extra_whitespace() {
        let attrs = super::parse_image_attributes("  src=a.jpg   width=100  ");
        assert_eq!(attrs.src, "a.jpg");
        assert_eq!(attrs.width.as_deref(), Some("100"));
    }

    #[test]
    fn split_key_value_pairs_basic() {
        let pairs = super::split_key_value_pairs("key=value");
        assert_eq!(pairs, vec![("key".to_string(), "value".to_string())]);
    }

    #[test]
    fn split_key_value_pairs_quoted() {
        let pairs = super::split_key_value_pairs(r#"key="hello world""#);
        assert_eq!(pairs, vec![("key".to_string(), "hello world".to_string())]);
    }

    #[test]
    fn split_key_value_pairs_mixed() {
        let pairs = super::split_key_value_pairs(r#"a=1 b="two three" c=4"#);
        assert_eq!(pairs.len(), 3);
        assert_eq!(pairs[0], ("a".to_string(), "1".to_string()));
        assert_eq!(pairs[1], ("b".to_string(), "two three".to_string()));
        assert_eq!(pairs[2], ("c".to_string(), "4".to_string()));
    }

    #[test]
    fn split_key_value_pairs_no_equals() {
        let pairs = super::split_key_value_pairs("bare_token");
        assert!(pairs.is_empty());
    }

    #[test]
    fn split_key_value_pairs_empty() {
        let pairs = super::split_key_value_pairs("");
        assert!(pairs.is_empty());
    }
}

#[cfg(test)]
mod delegate_tests {
    use super::*;
    use crate::ast::{DirectiveKind, Line};

    fn lines(input: &str) -> Vec<Line> {
        parse(input).expect("parse failed").lines
    }

    #[test]
    fn abc_content_is_verbatim() {
        let song = parse("{start_of_abc}\nX:1\nK:G\n{end_of_abc}").unwrap();
        assert_eq!(song.lines.len(), 4);
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert_eq!(l.segments.len(), 1);
            assert!(l.segments[0].chord.is_none());
            assert_eq!(l.segments[0].text, "X:1");
        } else {
            panic!("expected lyrics line for ABC content");
        }
    }

    #[test]
    fn abc_preserves_brackets() {
        let song = parse("{start_of_abc}\n|:GABc|[1d2d2:|[2d4d4:|\n{end_of_abc}").unwrap();
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert_eq!(l.segments[0].text, "|:GABc|[1d2d2:|[2d4d4:|");
        } else {
            panic!("expected verbatim lyrics line");
        }
    }

    #[test]
    fn ly_content_is_verbatim() {
        let song = parse("{start_of_ly}\n\\relative c' { c4 d e f }\n{end_of_ly}").unwrap();
        assert_eq!(song.lines.len(), 3);
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert!(l.segments[0].chord.is_none());
        } else {
            panic!("expected lyrics line for Lilypond content");
        }
    }

    #[test]
    fn svg_content_is_verbatim() {
        let song = parse("{start_of_svg}\n<svg><rect/></svg>\n{end_of_svg}").unwrap();
        assert_eq!(song.lines.len(), 3);
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert!(l.segments[0].chord.is_none());
        } else {
            panic!("expected lyrics line for SVG content");
        }
    }

    #[test]
    fn textblock_content_is_verbatim() {
        let song = parse("{start_of_textblock}\n[Am]Not a chord\n{end_of_textblock}").unwrap();
        assert_eq!(song.lines.len(), 3);
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert_eq!(l.segments.len(), 1);
            assert!(l.segments[0].chord.is_none());
            assert_eq!(l.segments[0].text, "[Am]Not a chord");
        } else {
            panic!("expected lyrics line for textblock content");
        }
    }

    #[test]
    fn textblock_preserves_braces() {
        let song = parse("{start_of_textblock}\n{some directive}\n{end_of_textblock}").unwrap();
        if let Line::Lyrics(ref l) = song.lines[1] {
            assert_eq!(l.segments[0].text, "{some directive}");
        } else {
            panic!("expected verbatim lyrics line");
        }
    }

    #[test]
    fn chords_parsed_after_abc_ends() {
        let song = parse("{start_of_abc}\nX:1\n{end_of_abc}\n[Am]Hello").unwrap();
        if let Line::Lyrics(ref l) = song.lines[3] {
            assert!(l.segments[0].chord.is_some());
            assert_eq!(l.segments[0].chord.as_ref().unwrap().name, "Am");
        } else {
            panic!("expected lyrics line with chord after ABC section");
        }
    }

    #[test]
    fn chords_parsed_after_ly_ends() {
        let song = parse("{start_of_ly}\nnotes\n{end_of_ly}\n[G]Hello").unwrap();
        if let Line::Lyrics(ref l) = song.lines[3] {
            assert!(l.segments[0].chord.is_some());
            assert_eq!(l.segments[0].chord.as_ref().unwrap().name, "G");
        } else {
            panic!("expected lyrics line with chord after Lilypond section");
        }
    }

    #[test]
    fn chords_parsed_after_svg_ends() {
        let song = parse("{start_of_svg}\n<svg/>\n{end_of_svg}\n[C]Hello").unwrap();
        if let Line::Lyrics(ref l) = song.lines[3] {
            assert!(l.segments[0].chord.is_some());
            assert_eq!(l.segments[0].chord.as_ref().unwrap().name, "C");
        } else {
            panic!("expected lyrics line with chord after SVG section");
        }
    }

    #[test]
    fn chords_parsed_after_textblock_ends() {
        let song = parse("{start_of_textblock}\ntext\n{end_of_textblock}\n[D]Hello").unwrap();
        if let Line::Lyrics(ref l) = song.lines[3] {
            assert!(l.segments[0].chord.is_some());
            assert_eq!(l.segments[0].chord.as_ref().unwrap().name, "D");
        } else {
            panic!("expected lyrics line with chord after textblock section");
        }
    }

    #[test]
    fn abc_directive_with_label() {
        let result = lines("{start_of_abc: My Melody}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.kind, DirectiveKind::StartOfAbc);
            assert_eq!(d.value.as_deref(), Some("My Melody"));
        } else {
            panic!("expected directive");
        }
    }

    // -- Selector suffix parsing --------------------------------------------

    #[test]
    fn selector_suffix_on_metadata_directive() {
        let result = lines("{title-piano: My Song}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "title");
            assert_eq!(d.value.as_deref(), Some("My Song"));
            assert_eq!(d.kind, DirectiveKind::Title);
            assert_eq!(d.selector.as_deref(), Some("piano"));
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn textblock_directive_with_label() {
        let result = lines("{start_of_textblock: Notes}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.kind, DirectiveKind::StartOfTextblock);
            assert_eq!(d.value.as_deref(), Some("Notes"));
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn selector_suffix_on_key_directive() {
        let result = lines("{key-bass: E}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "key");
            assert_eq!(d.value.as_deref(), Some("E"));
            assert_eq!(d.kind, DirectiveKind::Key);
            assert_eq!(d.selector.as_deref(), Some("bass"));
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn delegate_sections_not_custom() {
        assert_eq!(
            DirectiveKind::from_name("start_of_abc"),
            DirectiveKind::StartOfAbc
        );
        assert_eq!(
            DirectiveKind::from_name("start_of_ly"),
            DirectiveKind::StartOfLy
        );
        assert_eq!(
            DirectiveKind::from_name("start_of_svg"),
            DirectiveKind::StartOfSvg
        );
        assert_eq!(
            DirectiveKind::from_name("start_of_textblock"),
            DirectiveKind::StartOfTextblock
        );
    }

    #[test]
    fn lyrics_markup_preserves_backward_compat() {
        // The LyricsLine::text() method should return plain text
        let result = lines("[Am]Hello <b>bold</b> [G]world");
        match &result[0] {
            Line::Lyrics(lyrics) => {
                assert_eq!(lyrics.text(), "Hello bold world");
            }
            _ => panic!("expected lyrics line"),
        }
    }

    // -- NewSong directive --------------------------------------------------

    #[test]
    fn new_song_directive_kind() {
        assert_eq!(DirectiveKind::from_name("new_song"), DirectiveKind::NewSong);
        assert_eq!(DirectiveKind::from_name("ns"), DirectiveKind::NewSong);
        assert_eq!(DirectiveKind::from_name("NEW_SONG"), DirectiveKind::NewSong);
        assert_eq!(DirectiveKind::from_name("Ns"), DirectiveKind::NewSong);
    }

    #[test]
    fn new_song_canonical_name() {
        assert_eq!(DirectiveKind::NewSong.canonical_name(), "new_song");
    }

    #[test]
    fn new_song_parsed_as_directive() {
        let result = lines("{new_song}");
        assert_eq!(result.len(), 1);
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "new_song");
            assert_eq!(d.kind, DirectiveKind::NewSong);
            assert!(d.value.is_none());
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn selector_suffix_on_comment_directive() {
        // Comment directives with selectors are kept as Line::Directive
        // (not converted to Line::Comment) to preserve the selector.
        let result = lines("{comment-piano: Play softly}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.kind, DirectiveKind::Comment);
            assert_eq!(d.value.as_deref(), Some("Play softly"));
            assert_eq!(d.selector.as_deref(), Some("piano"));
        } else {
            panic!(
                "expected directive for comment with selector, got {:?}",
                result[0]
            );
        }
    }

    #[test]
    fn ns_alias_parsed_as_directive() {
        let result = lines("{ns}");
        assert_eq!(result.len(), 1);
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "new_song");
            assert_eq!(d.kind, DirectiveKind::NewSong);
        } else {
            panic!("expected directive");
        }
    }

    // -- Multi-song parsing -------------------------------------------------

    #[test]
    fn parse_multi_single_song() {
        let input = "{title: Only Song}\n[G]Hello";
        let songs = parse_multi(input).unwrap();
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].metadata.title.as_deref(), Some("Only Song"));
    }

    #[test]
    fn parse_multi_two_songs() {
        let input = "{title: Song One}\nLyrics one\n{new_song}\n{title: Song Two}\nLyrics two";
        let songs = parse_multi(input).unwrap();
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].metadata.title.as_deref(), Some("Song One"));
        assert_eq!(songs[1].metadata.title.as_deref(), Some("Song Two"));
    }

    #[test]
    fn parse_multi_ns_alias() {
        let input = "{title: First}\n{ns}\n{title: Second}";
        let songs = parse_multi(input).unwrap();
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].metadata.title.as_deref(), Some("First"));
        assert_eq!(songs[1].metadata.title.as_deref(), Some("Second"));
    }

    #[test]
    fn parse_multi_three_songs() {
        let input = "{title: A}\n{new_song}\n{title: B}\n{new_song}\n{title: C}";
        let songs = parse_multi(input).unwrap();
        assert_eq!(songs.len(), 3);
        assert_eq!(songs[0].metadata.title.as_deref(), Some("A"));
        assert_eq!(songs[1].metadata.title.as_deref(), Some("B"));
        assert_eq!(songs[2].metadata.title.as_deref(), Some("C"));
    }

    #[test]
    fn parse_multi_empty_first_song() {
        // {new_song} at the very beginning means the first segment is empty
        let input = "{new_song}\n{title: Second}";
        let songs = parse_multi(input).unwrap();
        assert_eq!(songs.len(), 2);
        assert!(songs[0].metadata.title.is_none());
        assert_eq!(songs[1].metadata.title.as_deref(), Some("Second"));
    }

    #[test]
    fn parse_multi_case_insensitive() {
        let input = "{title: A}\n{NEW_SONG}\n{title: B}";
        let songs = parse_multi(input).unwrap();
        assert_eq!(songs.len(), 2);
    }

    #[test]
    fn parse_multi_with_whitespace() {
        let input = "{title: A}\n{ new_song }\n{title: B}";
        let songs = parse_multi(input).unwrap();
        assert_eq!(songs.len(), 2);
    }

    #[test]
    fn parse_multi_crlf_line_endings() {
        let input = "{title: A}\r\n[G]Hello\r\n{new_song}\r\n{title: B}\r\n[Am]World\r\n";
        let songs = parse_multi(input).unwrap();
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].metadata.title, Some("A".to_string()));
        assert_eq!(songs[1].metadata.title, Some("B".to_string()));
    }

    #[test]
    fn parse_multi_lenient_collects_errors() {
        let input = "{title: Good}\n[Am\n{new_song}\n{title: Also Good}\n[G]Hello";
        let result = parse_multi_lenient(input);
        assert_eq!(result.results.len(), 2);
        assert!(result.results[0].has_errors()); // unclosed chord
        assert!(result.results[1].is_ok());
        assert_eq!(
            result.results[1].song.metadata.title.as_deref(),
            Some("Also Good")
        );
    }

    #[test]
    fn comment_without_selector_still_becomes_line_comment() {
        let result = lines("{comment: Normal comment}");
        assert!(
            matches!(result[0], Line::Comment(CommentStyle::Normal, _)),
            "comment without selector should still be Line::Comment"
        );
    }

    #[test]
    fn parse_multi_songs_helper() {
        let input = "{title: A}\n{new_song}\n{title: B}";
        let result = parse_multi_lenient(input);
        let songs = result.songs();
        assert_eq!(songs.len(), 2);
        assert_eq!(songs[0].metadata.title.as_deref(), Some("A"));
        assert_eq!(songs[1].metadata.title.as_deref(), Some("B"));
    }

    #[test]
    fn parse_multi_preserves_song_content() {
        let input = "{title: Song One}
{artist: Artist One}
{start_of_chorus}
[G]La la [C]la
{end_of_chorus}
{new_song}
{title: Song Two}
{key: Am}
[Am]Hello [G]world";
        let songs = parse_multi(input).unwrap();
        assert_eq!(songs.len(), 2);

        // First song
        assert_eq!(songs[0].metadata.title.as_deref(), Some("Song One"));
        assert_eq!(songs[0].metadata.artists, vec!["Artist One".to_string()]);

        // Second song
        assert_eq!(songs[1].metadata.title.as_deref(), Some("Song Two"));
        assert_eq!(songs[1].metadata.key.as_deref(), Some("Am"));
    }

    #[test]
    fn is_new_song_line_detection() {
        assert!(is_new_song_line("{new_song}"));
        assert!(is_new_song_line("{ns}"));
        assert!(is_new_song_line("{NEW_SONG}"));
        assert!(is_new_song_line("{NS}"));
        assert!(is_new_song_line("{ new_song }"));
        assert!(is_new_song_line("{ ns }"));

        assert!(!is_new_song_line("{new_song: value}"));
        assert!(!is_new_song_line("{title}"));
        assert!(!is_new_song_line("new_song"));
        assert!(!is_new_song_line(""));
        assert!(!is_new_song_line("{new_songs}"));
    }

    #[test]
    fn single_parse_ignores_new_song() {
        // The single-song parse() should treat {new_song} as a regular directive
        // and not fail.
        let song = parse("{title: Test}\n{new_song}\n[G]Hello").unwrap();
        assert_eq!(song.metadata.title.as_deref(), Some("Test"));
        // The {new_song} should appear as a Directive line
        let has_new_song = song
            .lines
            .iter()
            .any(|l| matches!(l, Line::Directive(d) if d.kind == DirectiveKind::NewSong));
        assert!(has_new_song);
    }

    #[test]
    fn selector_suffix_on_environment_directive() {
        let result = lines("{start_of_chorus-piano}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "start_of_chorus");
            assert_eq!(d.kind, DirectiveKind::StartOfChorus);
            assert_eq!(d.selector.as_deref(), Some("piano"));
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn selector_suffix_on_end_environment() {
        let result = lines("{end_of_verse-guitar}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "end_of_verse");
            assert_eq!(d.kind, DirectiveKind::EndOfVerse);
            assert_eq!(d.selector.as_deref(), Some("guitar"));
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn no_selector_on_plain_directive() {
        let result = lines("{title: My Song}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.selector, None);
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn selector_suffix_case_insensitive() {
        let result = lines("{Title-PIANO: My Song}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.kind, DirectiveKind::Title);
            assert_eq!(d.selector.as_deref(), Some("piano"));
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn selector_with_short_alias() {
        let result = lines("{t-guitar: My Song}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.name, "title");
            assert_eq!(d.kind, DirectiveKind::Title);
            assert_eq!(d.selector.as_deref(), Some("guitar"));
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn unknown_directive_with_hyphen_no_selector() {
        // "my-custom" -> "my" is Unknown, so the whole name is Unknown
        let result = lines("{my-custom: value}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.kind, DirectiveKind::Unknown("my-custom".to_string()));
            assert_eq!(d.selector, None);
        } else {
            panic!("expected directive");
        }
    }

    #[test]
    fn custom_section_with_selector() {
        let result = lines("{start_of_intro-piano}");
        if let Line::Directive(ref d) = result[0] {
            assert_eq!(d.kind, DirectiveKind::StartOfSection("intro".to_string()));
            assert_eq!(d.selector.as_deref(), Some("piano"));
        } else {
            panic!("expected directive");
        }
    }
}
