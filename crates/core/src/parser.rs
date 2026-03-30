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
    Chord, CommentStyle, Directive, DirectiveKind, Line, LyricsLine, LyricsSegment, Song,
};
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
    /// Whether we are currently inside a `{start_of_tab}`..`{end_of_tab}` block.
    /// Lines inside tab sections are treated as verbatim text (no chord parsing).
    in_tab: bool,
    /// Whether we are currently inside a `{start_of_grid}`..`{end_of_grid}` block.
    /// Lines inside grid sections are treated as verbatim text (no chord parsing).
    in_grid: bool,
}

impl Parser {
    /// Creates a new parser for the given token stream.
    #[must_use]
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            in_tab: false,
            in_grid: false,
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
        match self.peek_kind() {
            // An empty line: just a Newline token.
            TokenKind::Newline => {
                self.advance();
                Ok(Line::Empty)
            }
            // A directive line: starts with `{`.
            TokenKind::DirectiveOpen => {
                // Inside tab/grid: only the matching end directive is parsed;
                // everything else is verbatim text.
                if self.in_tab && !self.is_end_of_tab_ahead() {
                    return self.parse_verbatim_line();
                }
                if self.in_grid && !self.is_end_of_grid_ahead() {
                    return self.parse_verbatim_line();
                }
                let line = self.parse_directive_line()?;
                // Track tab/grid section state.
                if let Line::Directive(ref d) = line {
                    match d.kind {
                        DirectiveKind::StartOfTab => self.in_tab = true,
                        DirectiveKind::EndOfTab => self.in_tab = false,
                        DirectiveKind::StartOfGrid => self.in_grid = true,
                        DirectiveKind::EndOfGrid => self.in_grid = false,
                        _ => {}
                    }
                }
                Ok(line)
            }
            // Inside a tab or grid section: treat as verbatim text (no chord parsing).
            _ if self.in_tab || self.in_grid => self.parse_verbatim_line(),
            // Anything else: a lyrics line.
            _ => self.parse_lyrics_line(),
        }
    }

    /// Peeks ahead to check if the current `{` starts an `{end_of_tab}` or
    /// `{eot}` directive. This allows the parser to exit tab mode.
    ///
    /// Only checks the next token after `DirectiveOpen` for the directive
    /// name text; the full directive structure (including `DirectiveClose`)
    /// is validated later by `parse_directive_line`.
    fn is_end_of_tab_ahead(&self) -> bool {
        if self.pos + 1 < self.tokens.len() {
            if let TokenKind::Text(ref text) = self.tokens[self.pos + 1].kind {
                let trimmed = text.trim().to_ascii_lowercase();
                return trimmed == "end_of_tab" || trimmed == "eot";
            }
        }
        false
    }

    /// Peeks ahead to check if the current `{` starts an `{end_of_grid}` or
    /// `{eog}` directive. This allows the parser to exit grid mode.
    ///
    /// Only checks the next token after `DirectiveOpen` for the directive
    /// name text; the full directive structure (including `DirectiveClose`)
    /// is validated later by `parse_directive_line`.
    fn is_end_of_grid_ahead(&self) -> bool {
        if self.pos + 1 < self.tokens.len() {
            if let TokenKind::Text(ref text) = self.tokens[self.pos + 1].kind {
                let trimmed = text.trim().to_ascii_lowercase();
                return trimmed == "end_of_grid" || trimmed == "eog";
            }
        }
        false
    }

    /// Parses a verbatim text line (used inside tab and grid sections).
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

        // Classify the directive.
        let kind = DirectiveKind::from_name(&name);

        // Comment directives → Line::Comment with appropriate style.
        if kind.is_comment() {
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
                    };
                    return Ok(Line::Directive(directive));
                }
            }
            // {meta} without value — treat as unknown
            let directive = Directive {
                name: "meta".to_string(),
                value: None,
                kind: DirectiveKind::Unknown("meta".to_string()),
            };
            return Ok(Line::Directive(directive));
        }

        // Build the directive with canonical name and kind.
        let canonical = kind.full_canonical_name();
        let directive = Directive {
            name: canonical,
            value,
            kind,
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
            Ok(Line::Lyrics(LyricsLine { segments }))
        }
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
                crate::token::Position::new(0, 0),
                crate::token::Position::new(0, 0),
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
                    crate::token::Position::new(0, 0),
                    crate::token::Position::new(0, 0),
                ),
            )],
        };
    }
    let tokens = Lexer::new(input).tokenize();
    Parser::new(tokens).parse_lenient()
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
}
