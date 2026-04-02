//! Really Relaxed JSON (RRJSON) parser for ChordPro configuration files.
//!
//! Supports three levels of JSON relaxation:
//!
//! 1. **Strict JSON** — standard JSON
//! 2. **Relaxed JSON** — comments (`//`, `/* */`), trailing commas, unquoted
//!    keys, single-quoted strings
//! 3. **RRJSON** — dot-separated keys (`pdf.chorus.indent = 20`), optional
//!    outer braces, `include` directives
//!
//! # Examples
//!
//! ```
//! use chordpro_core::rrjson::{parse_rrjson, Value};
//!
//! let input = r#"{ "key": "value", "num": 42 }"#;
//! let value = parse_rrjson(input).unwrap();
//! assert_eq!(value["key"], Value::String("value".to_string()));
//! ```

use core::fmt;

// ---------------------------------------------------------------------------
// Value type
// ---------------------------------------------------------------------------

/// A configuration value parsed from JSON/RRJSON.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A null value.
    Null,
    /// A boolean value.
    Bool(bool),
    /// A numeric value (stored as f64).
    Number(f64),
    /// A string value.
    String(String),
    /// An ordered list of values.
    Array(Vec<Value>),
    /// A map of string keys to values (insertion order preserved).
    Object(Vec<(String, Value)>),
}

/// Shared sentinel for returning references to `Value::Null` from indexing
/// and lookup methods.
pub(crate) static NULL: Value = Value::Null;

impl Value {
    /// Look up a key in an Object value. Returns `Value::Null` if not found
    /// or if `self` is not an Object.
    #[must_use]
    pub fn get(&self, key: &str) -> &Value {
        match self {
            Value::Object(entries) => entries
                .iter()
                .find(|(k, _)| k == key)
                .map_or(&NULL, |(_, v)| v),
            _ => &NULL,
        }
    }

    /// Returns true if this is a Null value.
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Returns the string value if this is a String, or None.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the number value if this is a Number, or None.
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// Returns the bool value if this is a Bool, or None.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns a slice of the array elements if this is an Array, or None.
    ///
    /// # Examples
    ///
    /// ```
    /// use chordpro_core::rrjson::Value;
    ///
    /// let arr = Value::Array(vec![Value::Number(1.0), Value::Number(2.0)]);
    /// assert_eq!(arr.as_array().unwrap().len(), 2);
    /// assert!(Value::Null.as_array().is_none());
    /// ```
    #[must_use]
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Returns a slice of the object entries if this is an Object, or None.
    ///
    /// # Examples
    ///
    /// ```
    /// use chordpro_core::rrjson::Value;
    ///
    /// let obj = Value::Object(vec![("key".to_string(), Value::Bool(true))]);
    /// assert_eq!(obj.as_object().unwrap().len(), 1);
    /// assert!(Value::Null.as_object().is_none());
    /// ```
    #[must_use]
    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Object(entries) => Some(entries),
            _ => None,
        }
    }
}

impl core::ops::Index<&str> for Value {
    type Output = Value;

    fn index(&self, key: &str) -> &Value {
        self.get(key)
    }
}

impl core::ops::Index<usize> for Value {
    type Output = Value;

    /// Index into an Array by position. Returns `Value::Null` for
    /// out-of-bounds indices or non-array values.
    fn index(&self, index: usize) -> &Value {
        match self {
            Value::Array(items) => items.get(index).unwrap_or(&NULL),
            _ => &NULL,
        }
    }
}

/// Writes a JSON-escaped string (including surrounding quotes) to the formatter.
fn write_json_string(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    write!(f, "\"")?;
    for c in s.chars() {
        match c {
            '"' => write!(f, "\\\"")?,
            '\\' => write!(f, "\\\\")?,
            '\n' => write!(f, "\\n")?,
            '\r' => write!(f, "\\r")?,
            '\t' => write!(f, "\\t")?,
            '\u{08}' => write!(f, "\\b")?,
            '\u{0C}' => write!(f, "\\f")?,
            c if c < '\u{20}' => write!(f, "\\u{:04x}", c as u32)?,
            c => write!(f, "{c}")?,
        }
    }
    write!(f, "\"")
}

impl fmt::Display for Value {
    /// Formats the value as valid JSON.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Number(n) => {
                // Display whole numbers without a decimal point. The upper
                // bound uses strict less-than because `i64::MAX as f64`
                // rounds up to a value larger than `i64::MAX`, and casting
                // such a value `as i64` would overflow to `i64::MIN`.
                if n.fract() == 0.0
                    && n.is_finite()
                    && *n >= i64::MIN as f64
                    && *n < i64::MAX as f64
                {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{n}")
                }
            }
            Value::String(s) => write_json_string(f, s),
            Value::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Object(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write_json_string(f, k)?;
                    write!(f, ":{v}")?;
                }
                write!(f, "}}")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// An error encountered during RRJSON parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable error message.
    pub message: String,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub column: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RRJSON parse error at line {} column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Result of parsing RRJSON, including any non-fatal warnings.
#[derive(Debug)]
pub struct ParseResult {
    /// The parsed value.
    pub value: Value,
    /// Non-fatal warnings encountered during parsing (e.g., unsupported
    /// include directives).
    pub warnings: Vec<String>,
}

/// Parse a RRJSON string into a [`Value`].
///
/// Accepts strict JSON, relaxed JSON, and RRJSON formats.
/// Non-fatal warnings (e.g., unsupported include directives) are silently
/// discarded. Use [`parse_rrjson_with_warnings`] to collect them.
///
/// # Errors
///
/// Returns a [`ParseError`] if the input is malformed.
pub fn parse_rrjson(input: &str) -> Result<Value, ParseError> {
    parse_rrjson_with_warnings(input).map(|r| r.value)
}

/// Parse a RRJSON string into a [`ParseResult`] containing the value and
/// any non-fatal warnings.
///
/// # Errors
///
/// Returns a [`ParseError`] if the input is malformed.
pub fn parse_rrjson_with_warnings(input: &str) -> Result<ParseResult, ParseError> {
    let mut parser = Parser::new(input);
    parser.skip_ws_and_comments()?;

    // RRJSON: if the input doesn't start with '{' or '[', treat it as
    // bare key-value pairs (implicit object).
    let value = if parser.peek() != Some('{') && parser.peek() != Some('[') {
        parser.parse_bare_object()?
    } else {
        let value = parser.parse_value()?;
        parser.skip_ws_and_comments()?;
        if parser.pos < parser.input.len() {
            return Err(parser.error("unexpected content after value"));
        }
        value
    };

    Ok(ParseResult {
        value,
        warnings: parser.warnings,
    })
}

// ---------------------------------------------------------------------------
// Internal parser
// ---------------------------------------------------------------------------

/// Maximum nesting depth for objects and arrays to prevent stack overflow
/// from deeply nested input.
const MAX_NESTING_DEPTH: usize = 64;

/// Maximum number of entries allowed in a single array or object.
/// Prevents memory amplification from large flat collections within the
/// 10 MB file size limit.
const MAX_ENTRIES: usize = 10_000;

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    depth: usize,
    warnings: Vec<String>,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            depth: 0,
            warnings: Vec::new(),
        }
    }

    fn error(&self, message: &str) -> ParseError {
        let (line, column) = self.line_col();
        ParseError {
            message: message.to_string(),
            line,
            column,
        }
    }

    fn line_col(&self) -> (usize, usize) {
        let consumed = &self.input[..self.pos];
        let line = consumed.matches('\n').count() + 1;
        let col = consumed.rfind('\n').map_or(self.pos, |i| self.pos - i - 1) + 1;
        (line, col)
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn expect(&mut self, expected: char) -> Result<(), ParseError> {
        match self.advance() {
            Some(c) if c == expected => Ok(()),
            Some(c) => Err(self.error(&format!("expected '{expected}', found '{c}'"))),
            None => Err(self.error(&format!("expected '{expected}', found end of input"))),
        }
    }

    fn skip_ws_and_comments(&mut self) -> Result<(), ParseError> {
        loop {
            // Skip whitespace
            while self.pos < self.input.len() {
                let ch = self.input.as_bytes()[self.pos];
                if ch == b' ' || ch == b'\t' || ch == b'\n' || ch == b'\r' {
                    self.pos += 1;
                } else {
                    break;
                }
            }

            // Skip // line comments
            if self.input[self.pos..].starts_with("//") {
                if let Some(end) = self.input[self.pos..].find('\n') {
                    self.pos += end + 1;
                    continue;
                } else {
                    self.pos = self.input.len();
                    break;
                }
            }

            // Skip /* block comments */
            if self.input[self.pos..].starts_with("/*") {
                let open_pos = self.pos;
                self.pos += 2;
                if let Some(end) = self.input[self.pos..].find("*/") {
                    self.pos += end + 2;
                    continue;
                } else {
                    // Record position of the opening /* for the error message.
                    self.pos = open_pos;
                    return Err(self.error("unterminated block comment"));
                }
            }

            // Skip # line comments (RRJSON extension)
            if self.pos < self.input.len() && self.input.as_bytes()[self.pos] == b'#' {
                if let Some(end) = self.input[self.pos..].find('\n') {
                    self.pos += end + 1;
                    continue;
                } else {
                    self.pos = self.input.len();
                    break;
                }
            }

            break;
        }
        Ok(())
    }

    // -- Value parsing --------------------------------------------------------

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        self.skip_ws_and_comments()?;
        match self.peek() {
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('"') => self.parse_string().map(Value::String),
            Some('\'') => self.parse_single_quoted_string().map(Value::String),
            Some('t') | Some('f') => self.parse_bool(),
            Some('n') => self.parse_null(),
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            Some(c) => Err(self.error(&format!("unexpected character '{c}'"))),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn parse_object(&mut self) -> Result<Value, ParseError> {
        if self.depth >= MAX_NESTING_DEPTH {
            return Err(self.error("maximum nesting depth exceeded"));
        }
        self.depth += 1;

        self.expect('{')?;
        let mut entries = Vec::new();

        self.skip_ws_and_comments()?;
        if self.peek() == Some('}') {
            self.advance();
            self.depth -= 1;
            return Ok(Value::Object(entries));
        }

        loop {
            self.skip_ws_and_comments()?;

            // Allow trailing comma before '}'
            if self.peek() == Some('}') {
                break;
            }

            let key = self.parse_key()?;
            self.skip_ws_and_comments()?;

            // Accept both ':' and '=' as key-value separator
            match self.peek() {
                Some(':') | Some('=') => {
                    self.advance();
                }
                _ => return Err(self.error("expected ':' or '=' after object key")),
            }

            let value = self.parse_value()?;
            let dot_segments = key.chars().filter(|&c| c == '.').count();
            if dot_segments >= MAX_NESTING_DEPTH {
                return Err(self.error("dotted key exceeds maximum nesting depth"));
            }
            self.validate_dotted_key(&key)?;
            self.insert_dotted_key(&mut entries, &key, value);

            if entries.len() > MAX_ENTRIES {
                return Err(self.error("maximum object entry count exceeded"));
            }

            self.skip_ws_and_comments()?;
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some('}') => {}
                _ => return Err(self.error("expected ',' or '}' in object")),
            }
        }

        self.expect('}')?;
        self.depth -= 1;
        Ok(Value::Object(entries))
    }

    fn parse_array(&mut self) -> Result<Value, ParseError> {
        if self.depth >= MAX_NESTING_DEPTH {
            return Err(self.error("maximum nesting depth exceeded"));
        }
        self.depth += 1;

        self.expect('[')?;
        let mut items = Vec::new();

        self.skip_ws_and_comments()?;
        if self.peek() == Some(']') {
            self.advance();
            self.depth -= 1;
            return Ok(Value::Array(items));
        }

        loop {
            self.skip_ws_and_comments()?;

            // Allow trailing comma before ']'
            if self.peek() == Some(']') {
                break;
            }

            items.push(self.parse_value()?);

            if items.len() > MAX_ENTRIES {
                return Err(self.error("maximum array entry count exceeded"));
            }

            self.skip_ws_and_comments()?;
            match self.peek() {
                Some(',') => {
                    self.advance();
                }
                Some(']') => {}
                _ => return Err(self.error("expected ',' or ']' in array")),
            }
        }

        self.expect(']')?;
        self.depth -= 1;
        Ok(Value::Array(items))
    }

    /// Parse an object key: quoted string or unquoted identifier (relaxed JSON).
    fn parse_key(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Some('"') => self.parse_string(),
            Some('\'') => self.parse_single_quoted_string(),
            Some(c) if c.is_ascii_alphabetic() || c == '_' => self.parse_unquoted_key(),
            _ => Err(self.error("expected object key")),
        }
    }

    /// Parse an unquoted key (relaxed JSON). Allows alphanumeric, '_', '-', '.'.
    fn parse_unquoted_key(&mut self) -> Result<String, ParseError> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                self.advance();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(self.error("expected key"));
        }
        Ok(self.input[start..self.pos].to_string())
    }

    fn parse_string(&mut self) -> Result<String, ParseError> {
        self.expect('"')?;
        let mut s = String::new();
        loop {
            match self.advance() {
                Some('\\') => {
                    s.push(self.parse_escape()?);
                }
                Some('"') => return Ok(s),
                Some(c) => s.push(c),
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    fn parse_single_quoted_string(&mut self) -> Result<String, ParseError> {
        self.expect('\'')?;
        let mut s = String::new();
        loop {
            match self.advance() {
                Some('\\') => {
                    s.push(self.parse_escape()?);
                }
                Some('\'') => return Ok(s),
                Some(c) => s.push(c),
                None => return Err(self.error("unterminated string")),
            }
        }
    }

    fn parse_escape(&mut self) -> Result<char, ParseError> {
        match self.advance() {
            Some('"') => Ok('"'),
            Some('\'') => Ok('\''),
            Some('\\') => Ok('\\'),
            Some('/') => Ok('/'),
            Some('n') => Ok('\n'),
            Some('r') => Ok('\r'),
            Some('t') => Ok('\t'),
            Some('b') => Ok('\u{08}'),
            Some('f') => Ok('\u{0C}'),
            Some('u') => self.parse_unicode_escape(),
            Some(c) => Err(self.error(&format!("invalid escape character '{c}'"))),
            None => Err(self.error("unterminated escape sequence")),
        }
    }

    /// Parses a `\uXXXX` Unicode escape sequence. Handles surrogate pairs
    /// (`\uD800`–`\uDBFF` followed by `\uDC00`–`\uDFFF`).
    fn parse_unicode_escape(&mut self) -> Result<char, ParseError> {
        let high = self.parse_hex4()?;

        // Check for high surrogate (U+D800..U+DBFF)
        if (0xD800..=0xDBFF).contains(&high) {
            // Expect a low surrogate: \uDC00..\uDFFF
            if self.peek() != Some('\\') {
                return Err(self.error("expected low surrogate pair after high surrogate"));
            }
            self.advance(); // consume '\'
            if self.peek() != Some('u') {
                return Err(self.error("expected low surrogate pair after high surrogate"));
            }
            self.advance(); // consume 'u'

            let low = self.parse_hex4()?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(self.error(&format!(
                    "invalid low surrogate U+{low:04X}, expected U+DC00..U+DFFF"
                )));
            }

            let code_point = 0x10000 + ((high as u32 - 0xD800) << 10) + (low as u32 - 0xDC00);
            char::from_u32(code_point).ok_or_else(|| {
                self.error(&format!("invalid Unicode code point U+{code_point:04X}"))
            })
        } else if (0xDC00..=0xDFFF).contains(&high) {
            Err(self.error(&format!(
                "unexpected low surrogate U+{high:04X} without preceding high surrogate"
            )))
        } else {
            char::from_u32(high as u32)
                .ok_or_else(|| self.error(&format!("invalid Unicode code point U+{high:04X}")))
        }
    }

    /// Reads exactly 4 hex digits and returns the value as a `u16`.
    fn parse_hex4(&mut self) -> Result<u16, ParseError> {
        let mut value: u16 = 0;
        for i in 0..4 {
            match self.advance() {
                Some(c) if c.is_ascii_hexdigit() => {
                    let digit = c.to_digit(16).expect("validated by is_ascii_hexdigit") as u16;
                    value = value << 4 | digit;
                }
                Some(c) => {
                    return Err(self.error(&format!(
                        "invalid hex digit '{c}' in Unicode escape (digit {}/4)",
                        i + 1
                    )));
                }
                None => {
                    return Err(self.error("unterminated Unicode escape sequence"));
                }
            }
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<Value, ParseError> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.advance();
        }
        // Integer part — require at least one digit.
        let integer_start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        let has_integer_digits = self.pos > integer_start;
        // Fractional part
        let mut has_fraction_digits = false;
        if self.peek() == Some('.') {
            self.advance();
            let frac_start = self.pos;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
            has_fraction_digits = self.pos > frac_start;
        }
        // Require at least one digit somewhere in the number.
        if !has_integer_digits && !has_fraction_digits {
            let num_str = &self.input[start..self.pos];
            return Err(self.error(&format!("expected digit after minus sign: {num_str}")));
        }
        // Exponent
        if self.peek() == Some('e') || self.peek() == Some('E') {
            self.advance();
            if self.peek() == Some('+') || self.peek() == Some('-') {
                self.advance();
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let num_str = &self.input[start..self.pos];
        let n: f64 = num_str
            .parse()
            .map_err(|_| self.error(&format!("invalid number: {num_str}")))?;
        if !n.is_finite() {
            return Err(self.error(&format!("number out of range: {num_str}")));
        }
        Ok(Value::Number(n))
    }

    /// Check whether the character at `self.pos + offset` is a word character
    /// (alphanumeric or `_`). Returns `false` if at end of input.
    fn is_word_char_at(&self, offset: usize) -> bool {
        self.input[self.pos + offset..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    fn parse_bool(&mut self) -> Result<Value, ParseError> {
        if self.input[self.pos..].starts_with("true") && !self.is_word_char_at(4) {
            self.pos += 4;
            Ok(Value::Bool(true))
        } else if self.input[self.pos..].starts_with("false") && !self.is_word_char_at(5) {
            self.pos += 5;
            Ok(Value::Bool(false))
        } else {
            Err(self.error("expected 'true' or 'false'"))
        }
    }

    fn parse_null(&mut self) -> Result<Value, ParseError> {
        if self.input[self.pos..].starts_with("null") && !self.is_word_char_at(4) {
            self.pos += 4;
            Ok(Value::Null)
        } else {
            Err(self.error("expected 'null'"))
        }
    }

    // -- RRJSON bare object parsing -------------------------------------------

    /// Parse bare key-value pairs without outer braces.
    fn parse_bare_object(&mut self) -> Result<Value, ParseError> {
        let mut entries = Vec::new();

        loop {
            self.skip_ws_and_comments()?;
            if self.pos >= self.input.len() {
                break;
            }

            // Skip "include" directives (not yet supported — warn and consume the line).
            // Check word boundary after "include" so keys like "includes" or
            // "include_path" are parsed normally as object keys.
            // Also check that no ':' or '=' follows on the same line, which
            // would indicate a valid key-value pair with key "include".
            if self.input[self.pos..].starts_with("include")
                && self
                    .input
                    .as_bytes()
                    .get(self.pos + 7)
                    .is_none_or(|&b| b == b' ' || b == b'\t' || b == b'"' || b == b'\'')
            {
                // Look ahead on the current line for ':' or '=' separator.
                let rest_of_line = self.input[self.pos..].split('\n').next().unwrap_or("");
                let has_separator = rest_of_line.contains(':') || rest_of_line.contains('=');

                if !has_separator {
                    let (line, col) = self.line_col();
                    let directive_line = if let Some(end) = self.input[self.pos..].find('\n') {
                        let line_text = self.input[self.pos..self.pos + end].trim();
                        self.pos += end + 1;
                        line_text.to_string()
                    } else {
                        let line_text = self.input[self.pos..].trim().to_string();
                        self.pos = self.input.len();
                        line_text
                    };
                    self.warnings.push(format!(
                        "RRJSON include directives are not supported (line {line} column {col}): {directive_line}"
                    ));
                    continue;
                }
            }

            let key = self.parse_key()?;
            self.skip_ws_and_comments()?;

            // Accept ':' or '='
            match self.peek() {
                Some(':') | Some('=') => {
                    self.advance();
                }
                _ => return Err(self.error("expected ':' or '=' after key")),
            }

            let value = self.parse_value()?;
            let dot_segments = key.chars().filter(|&c| c == '.').count();
            if dot_segments >= MAX_NESTING_DEPTH {
                return Err(self.error("dotted key exceeds maximum nesting depth"));
            }
            self.validate_dotted_key(&key)?;
            self.insert_dotted_key(&mut entries, &key, value);

            if entries.len() > MAX_ENTRIES {
                return Err(self.error("maximum object entry count exceeded"));
            }

            // Optional separator (comma, semicolon, or newline-separated)
            self.skip_ws_and_comments()?;
            if self.peek() == Some(',') || self.peek() == Some(';') {
                self.advance();
            }
        }

        Ok(Value::Object(entries))
    }

    // -- Dot-separated key expansion ------------------------------------------

    /// Validate that a dotted key has no empty segments.
    ///
    /// Keys like `a..b`, `.a`, `a.`, and `.` are rejected because they would
    /// produce objects with empty-string keys.
    fn validate_dotted_key(&self, key: &str) -> Result<(), ParseError> {
        if key.contains('.') {
            for segment in key.split('.') {
                if segment.is_empty() {
                    return Err(self.error(&format!("dotted key has empty segment: \"{key}\"")));
                }
            }
        }
        Ok(())
    }

    /// Insert a value at a potentially dot-separated key path.
    ///
    /// For example, `pdf.chorus.indent` with value `20` becomes:
    /// `{"pdf": {"chorus": {"indent": 20}}}`
    fn insert_dotted_key(&self, entries: &mut Vec<(String, Value)>, key: &str, value: Value) {
        if let Some(dot_pos) = key.find('.') {
            let first = &key[..dot_pos];
            let rest = &key[dot_pos + 1..];

            // Find existing entry with the same key.
            let existing = entries.iter_mut().find(|(k, _)| k == first);

            match existing {
                Some((_, Value::Object(inner))) => {
                    // Existing object: recurse into it.
                    self.insert_dotted_key(inner, rest, value);
                }
                Some((_, existing_val)) => {
                    // Existing non-object (scalar): replace with an object.
                    let mut inner = Vec::new();
                    self.insert_dotted_key(&mut inner, rest, value);
                    *existing_val = Value::Object(inner);
                }
                None => {
                    // No existing entry: create a new nested object.
                    let mut inner = Vec::new();
                    self.insert_dotted_key(&mut inner, rest, value);
                    entries.push((first.to_string(), Value::Object(inner)));
                }
            }
        } else {
            // Leaf key: replace existing entry or append new one.
            if let Some((_, existing_val)) = entries.iter_mut().find(|(k, _)| k == key) {
                *existing_val = value;
            } else {
                entries.push((key.to_string(), value));
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- Strict JSON ----------------------------------------------------------

    #[test]
    fn test_empty_object() {
        let v = parse_rrjson("{}").unwrap();
        assert_eq!(v, Value::Object(vec![]));
    }

    #[test]
    fn test_empty_array() {
        let v = parse_rrjson("[]").unwrap();
        assert_eq!(v, Value::Array(vec![]));
    }

    #[test]
    fn test_string_value() {
        let v = parse_rrjson(r#"{"key": "value"}"#).unwrap();
        assert_eq!(v["key"], Value::String("value".to_string()));
    }

    #[test]
    fn test_number_value() {
        let v = parse_rrjson(r#"{"n": 42}"#).unwrap();
        assert_eq!(v["n"], Value::Number(42.0));
    }

    #[test]
    fn test_float_value() {
        let v = parse_rrjson(r#"{"n": 2.75}"#).unwrap();
        assert_eq!(v["n"], Value::Number(2.75));
    }

    #[test]
    fn test_negative_number() {
        let v = parse_rrjson(r#"{"n": -5}"#).unwrap();
        assert_eq!(v["n"], Value::Number(-5.0));
    }

    #[test]
    fn test_bool_values() {
        let v = parse_rrjson(r#"{"a": true, "b": false}"#).unwrap();
        assert_eq!(v["a"], Value::Bool(true));
        assert_eq!(v["b"], Value::Bool(false));
    }

    #[test]
    fn test_null_value() {
        let v = parse_rrjson(r#"{"x": null}"#).unwrap();
        assert_eq!(v["x"], Value::Null);
    }

    #[test]
    fn test_bool_word_boundary() {
        assert!(parse_rrjson(r#"{"a": truex}"#).is_err());
        assert!(parse_rrjson(r#"{"a": falsehood}"#).is_err());
        assert!(parse_rrjson(r#"{"a": true_val}"#).is_err());
        assert!(parse_rrjson(r#"{"a": false0}"#).is_err());
    }

    #[test]
    fn test_null_word_boundary() {
        assert!(parse_rrjson(r#"{"a": nullify}"#).is_err());
        assert!(parse_rrjson(r#"{"a": null_val}"#).is_err());
        assert!(parse_rrjson(r#"{"a": null0}"#).is_err());
    }

    #[test]
    fn test_nested_object() {
        let v = parse_rrjson(r#"{"a": {"b": 1}}"#).unwrap();
        assert_eq!(v["a"]["b"], Value::Number(1.0));
    }

    #[test]
    fn test_array_values() {
        let v = parse_rrjson(r#"{"a": [1, 2, 3]}"#).unwrap();
        assert_eq!(
            v["a"],
            Value::Array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn test_escape_sequences() {
        let v = parse_rrjson(r#"{"s": "a\nb\tc"}"#).unwrap();
        assert_eq!(v["s"], Value::String("a\nb\tc".to_string()));
    }

    // -- Unicode escape sequences (\uXXXX) ------------------------------------

    #[test]
    fn test_unicode_escape_basic_bmp() {
        // \u00E9 = é (Latin Small Letter E with Acute)
        let v = parse_rrjson(r#"{"s": "\u00E9"}"#).unwrap();
        assert_eq!(v["s"], Value::String("é".to_string()));
    }

    #[test]
    fn test_unicode_escape_lowercase_hex() {
        let v = parse_rrjson(r#"{"s": "\u00e9"}"#).unwrap();
        assert_eq!(v["s"], Value::String("é".to_string()));
    }

    #[test]
    fn test_unicode_escape_ascii_range() {
        // \u0041 = 'A'
        let v = parse_rrjson(r#"{"s": "\u0041\u0042\u0043"}"#).unwrap();
        assert_eq!(v["s"], Value::String("ABC".to_string()));
    }

    #[test]
    fn test_unicode_escape_cjk() {
        // \u4E16 = 世, \u754C = 界
        let v = parse_rrjson(r#"{"s": "\u4E16\u754C"}"#).unwrap();
        assert_eq!(v["s"], Value::String("世界".to_string()));
    }

    #[test]
    fn test_unicode_escape_surrogate_pair() {
        // U+1F600 (😀) encoded as surrogate pair: \uD83D\uDE00
        let v = parse_rrjson(r#"{"s": "\uD83D\uDE00"}"#).unwrap();
        assert_eq!(v["s"], Value::String("😀".to_string()));
    }

    #[test]
    fn test_unicode_escape_surrogate_pair_musical_symbol() {
        // U+1D11E (𝄞 Musical Symbol G Clef) = \uD834\uDD1E
        let v = parse_rrjson(r#"{"s": "\uD834\uDD1E"}"#).unwrap();
        assert_eq!(v["s"], Value::String("𝄞".to_string()));
    }

    #[test]
    fn test_unicode_escape_mixed_with_text() {
        let v = parse_rrjson(r#"{"s": "caf\u00E9 \u2603 snow"}"#).unwrap();
        assert_eq!(v["s"], Value::String("café ☃ snow".to_string()));
    }

    #[test]
    fn test_unicode_escape_null_char() {
        // \u0000 is a valid Unicode escape
        let v = parse_rrjson(r#"{"s": "\u0000"}"#).unwrap();
        assert_eq!(v["s"], Value::String("\0".to_string()));
    }

    #[test]
    fn test_unicode_escape_in_single_quoted_string() {
        let v = parse_rrjson(r"{'s': '\u00E9'}").unwrap();
        assert_eq!(v["s"], Value::String("é".to_string()));
    }

    #[test]
    fn test_unicode_escape_incomplete_hex() {
        let result = parse_rrjson(r#"{"s": "\u00E"}"#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("hex digit"),
            "expected hex digit error, got: {err}"
        );
    }

    #[test]
    fn test_unicode_escape_non_hex_chars() {
        let result = parse_rrjson(r#"{"s": "\u00GG"}"#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("hex digit"),
            "expected hex digit error, got: {err}"
        );
    }

    #[test]
    fn test_unicode_escape_truncated() {
        let result = parse_rrjson(r#"{"s": "\u00"#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("unterminated"),
            "expected unterminated error, got: {err}"
        );
    }

    #[test]
    fn test_unicode_escape_lone_high_surrogate() {
        // \uD83D without a following \uDExx
        let result = parse_rrjson(r#"{"s": "\uD83D"}"#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("surrogate"),
            "expected surrogate error, got: {err}"
        );
    }

    #[test]
    fn test_unicode_escape_lone_low_surrogate() {
        let result = parse_rrjson(r#"{"s": "\uDE00"}"#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("surrogate"),
            "expected surrogate error, got: {err}"
        );
    }

    #[test]
    fn test_unicode_escape_invalid_low_surrogate() {
        // High surrogate followed by non-surrogate \u
        let result = parse_rrjson(r#"{"s": "\uD83D\u0041"}"#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("surrogate"),
            "expected surrogate error, got: {err}"
        );
    }

    // -- Relaxed JSON ---------------------------------------------------------

    #[test]
    fn test_line_comments() {
        let input = "{\n// comment\n\"key\": \"value\"\n}";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["key"], Value::String("value".to_string()));
    }

    #[test]
    fn test_block_comments() {
        let input = "{/* comment */\"key\": \"value\"}";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["key"], Value::String("value".to_string()));
    }

    #[test]
    fn test_unterminated_block_comment() {
        let input = "{/* this comment never closes\n\"key\": \"value\"}";
        let err = parse_rrjson(input).unwrap_err();
        assert!(
            err.message.contains("unterminated block comment"),
            "expected unterminated block comment error, got: {err}"
        );
        assert_eq!(err.line, 1);
        assert_eq!(err.column, 2);
    }

    #[test]
    fn test_unterminated_block_comment_at_eof() {
        let err = parse_rrjson("/*").unwrap_err();
        assert!(err.message.contains("unterminated block comment"));
    }

    #[test]
    fn test_hash_comments() {
        let input = "{\n# comment\n\"key\": \"value\"\n}";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["key"], Value::String("value".to_string()));
    }

    #[test]
    fn test_trailing_comma_object() {
        let v = parse_rrjson(r#"{"a": 1, "b": 2,}"#).unwrap();
        assert_eq!(v["a"], Value::Number(1.0));
        assert_eq!(v["b"], Value::Number(2.0));
    }

    #[test]
    fn test_trailing_comma_array() {
        let v = parse_rrjson("[1, 2, 3,]").unwrap();
        assert_eq!(
            v,
            Value::Array(vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0)
            ])
        );
    }

    #[test]
    fn test_unquoted_keys() {
        let v = parse_rrjson("{key: \"value\"}").unwrap();
        assert_eq!(v["key"], Value::String("value".to_string()));
    }

    #[test]
    fn test_single_quoted_strings() {
        let v = parse_rrjson("{'key': 'value'}").unwrap();
        assert_eq!(v["key"], Value::String("value".to_string()));
    }

    #[test]
    fn test_equals_separator() {
        let v = parse_rrjson("{key = \"value\"}").unwrap();
        assert_eq!(v["key"], Value::String("value".to_string()));
    }

    // -- RRJSON ---------------------------------------------------------------

    #[test]
    fn test_bare_key_value_pairs() {
        let input = "key = \"value\"\nnum = 42";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["key"], Value::String("value".to_string()));
        assert_eq!(v["num"], Value::Number(42.0));
    }

    #[test]
    fn test_dot_separated_keys() {
        let input = r#"{"pdf.chorus.indent": 20}"#;
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["pdf"]["chorus"]["indent"], Value::Number(20.0));
    }

    #[test]
    fn test_dot_keys_bare_format() {
        let input = "pdf.chorus.indent = 20\npdf.chorus.bar = 10";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["pdf"]["chorus"]["indent"], Value::Number(20.0));
        assert_eq!(v["pdf"]["chorus"]["bar"], Value::Number(10.0));
    }

    #[test]
    fn test_dotted_key_overwrites_scalar() {
        // a = 1 then a.b = 2 should replace scalar with object
        let input = "a = 1\na.b = 2";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["a"]["b"], Value::Number(2.0));
    }

    #[test]
    fn test_scalar_overwrites_dotted_key() {
        // a.b = 1 then a = 2 should replace object with scalar
        let input = "a.b = 1\na = 2";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["a"], Value::Number(2.0));
    }

    #[test]
    fn test_dotted_key_merges_siblings() {
        // a.b = 1 then a.c = 2 should merge into same object
        let input = "a.b = 1\na.c = 2";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["a"]["b"], Value::Number(1.0));
        assert_eq!(v["a"]["c"], Value::Number(2.0));
    }

    #[test]
    fn test_dotted_key_deep_overwrite() {
        // a.b.c = 1 then a.b = 2 should replace nested object
        let input = "a.b.c = 1\na.b = 2";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["a"]["b"], Value::Number(2.0));
    }

    #[test]
    fn test_include_directive_skipped() {
        let input = "include \"base.json\"\nkey = \"value\"";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["key"], Value::String("value".to_string()));
    }

    #[test]
    fn test_include_directive_single_quoted() {
        let input = "include 'base.json'\nkey = \"value\"";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["key"], Value::String("value".to_string()));
    }

    #[test]
    fn test_include_at_eof() {
        let input = "include \"base.json\"";
        let v = parse_rrjson(input).unwrap();
        // No keys — include is skipped and nothing else remains
        assert_eq!(v, Value::Object(vec![]));
    }

    #[test]
    fn test_includes_key_not_treated_as_include() {
        let input = "includes = \"base.json\"";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["includes"], Value::String("base.json".to_string()));
    }

    #[test]
    fn test_include_path_key_not_treated_as_include() {
        let input = "include_path = \"/etc/chordpro\"";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(
            v["include_path"],
            Value::String("/etc/chordpro".to_string())
        );
    }

    #[test]
    fn test_included_key_not_treated_as_include() {
        let input = "included = true";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["included"], Value::Bool(true));
    }

    #[test]
    fn test_include_key_with_colon_separator() {
        // "include" with a ':' separator should be parsed as a key-value pair
        let input = "include : \"somefile.json\"";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["include"], Value::String("somefile.json".to_string()));
    }

    #[test]
    fn test_include_key_with_equals_separator() {
        // "include" with an '=' separator should be parsed as a key-value pair
        let input = "include = \"somefile.json\"";
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["include"], Value::String("somefile.json".to_string()));
    }

    #[test]
    fn test_include_directive_produces_warning() {
        let input = "include \"base.json\"\nkey = \"value\"";
        let result = parse_rrjson_with_warnings(input).unwrap();
        assert_eq!(result.value["key"], Value::String("value".to_string()));
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].contains("include directives are not supported"));
    }

    #[test]
    fn test_no_warnings_for_normal_input() {
        let input = r#"{"key": "value"}"#;
        let result = parse_rrjson_with_warnings(input).unwrap();
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_mixed_relaxed_features() {
        let input = r#"{
            // Configuration file
            title: 'My Song',
            pdf.font.size: 12,
            chords: [
                "Am",
                "G",
            ],
        }"#;
        let v = parse_rrjson(input).unwrap();
        assert_eq!(v["title"], Value::String("My Song".to_string()));
        assert_eq!(v["pdf"]["font"]["size"], Value::Number(12.0));
    }

    // -- Error handling -------------------------------------------------------

    #[test]
    fn test_error_line_column() {
        let result = parse_rrjson("{\n  \"key\": }");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.line, 2);
        assert!(err.column > 0);
    }

    #[test]
    fn test_unterminated_string() {
        let result = parse_rrjson(r#"{"key": "unterminated}"#);
        assert!(result.is_err());
    }

    // -- Value methods --------------------------------------------------------

    #[test]
    fn test_value_as_str() {
        let v = Value::String("hello".to_string());
        assert_eq!(v.as_str(), Some("hello"));
        assert_eq!(Value::Null.as_str(), None);
    }

    #[test]
    fn test_value_as_f64() {
        let v = Value::Number(2.75);
        assert_eq!(v.as_f64(), Some(2.75));
        assert_eq!(Value::Null.as_f64(), None);
    }

    #[test]
    fn test_value_as_bool() {
        assert_eq!(Value::Bool(true).as_bool(), Some(true));
        assert_eq!(Value::Null.as_bool(), None);
    }

    #[test]
    fn test_value_as_array() {
        let v = Value::Array(vec![Value::Number(1.0), Value::Number(2.0)]);
        let arr = v.as_array();
        assert!(arr.is_some());
        assert_eq!(arr.unwrap().len(), 2);
        assert_eq!(arr.unwrap()[0], Value::Number(1.0));
        // Non-array returns None
        assert!(Value::Null.as_array().is_none());
        assert!(Value::String("x".to_string()).as_array().is_none());
    }

    #[test]
    fn test_value_as_object() {
        let v = Value::Object(vec![("key".to_string(), Value::Bool(true))]);
        let obj = v.as_object();
        assert!(obj.is_some());
        assert_eq!(obj.unwrap().len(), 1);
        assert_eq!(obj.unwrap()[0].0, "key");
        // Non-object returns None
        assert!(Value::Null.as_object().is_none());
        assert!(Value::Array(vec![]).as_object().is_none());
    }

    #[test]
    fn test_value_index_missing_key() {
        let v = Value::Object(vec![]);
        assert!(v["missing"].is_null());
    }

    #[test]
    fn test_value_index_usize() {
        let v = Value::Array(vec![
            Value::Number(10.0),
            Value::String("hello".to_string()),
            Value::Bool(true),
        ]);
        assert_eq!(v[0], Value::Number(10.0));
        assert_eq!(v[1], Value::String("hello".to_string()));
        assert_eq!(v[2], Value::Bool(true));
    }

    #[test]
    fn test_value_index_usize_out_of_bounds() {
        let v = Value::Array(vec![Value::Number(1.0)]);
        assert!(v[5].is_null());
    }

    #[test]
    fn test_value_index_usize_non_array() {
        let v = Value::String("hello".to_string());
        assert!(v[0].is_null());
    }

    #[test]
    fn test_value_is_null() {
        assert!(Value::Null.is_null());
        assert!(!Value::Bool(false).is_null());
    }

    // -- Display implementation -----------------------------------------------

    #[test]
    fn test_display_null() {
        assert_eq!(Value::Null.to_string(), "null");
    }

    #[test]
    fn test_display_bool() {
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Bool(false).to_string(), "false");
    }

    #[test]
    fn test_display_number_integer() {
        assert_eq!(Value::Number(42.0).to_string(), "42");
    }

    #[test]
    fn test_display_number_float() {
        assert_eq!(Value::Number(2.75).to_string(), "2.75");
    }

    #[test]
    fn test_display_string() {
        assert_eq!(Value::String("hello".to_string()).to_string(), r#""hello""#);
    }

    #[test]
    fn test_display_string_escapes() {
        let v = Value::String("a\"b\\c\n\r\t".to_string());
        assert_eq!(v.to_string(), r#""a\"b\\c\n\r\t""#);
    }

    #[test]
    fn test_display_string_control_chars() {
        let v = Value::String("\u{08}\u{0C}\u{01}".to_string());
        assert_eq!(v.to_string(), r#""\b\f\u0001""#);
    }

    #[test]
    fn test_display_array() {
        let v = Value::Array(vec![
            Value::Number(1.0),
            Value::String("two".to_string()),
            Value::Null,
        ]);
        assert_eq!(v.to_string(), r#"[1,"two",null]"#);
    }

    #[test]
    fn test_display_empty_array() {
        assert_eq!(Value::Array(vec![]).to_string(), "[]");
    }

    #[test]
    fn test_display_object() {
        let v = Value::Object(vec![
            ("a".to_string(), Value::Number(1.0)),
            ("b".to_string(), Value::Bool(true)),
        ]);
        assert_eq!(v.to_string(), r#"{"a":1,"b":true}"#);
    }

    #[test]
    fn test_display_empty_object() {
        assert_eq!(Value::Object(vec![]).to_string(), "{}");
    }

    #[test]
    fn test_display_number_large_f64() {
        // Values outside i64 range should not be cast to i64
        assert_eq!(Value::Number(1e20).to_string(), "100000000000000000000");
        assert_eq!(Value::Number(-1e20).to_string(), "-100000000000000000000");
        // Non-finite values cannot be produced by the parser (rejected at parse time),
        // but Display still handles them if constructed directly.
        assert_eq!(Value::Number(f64::INFINITY).to_string(), "inf");
        assert_eq!(Value::Number(f64::NEG_INFINITY).to_string(), "-inf");
    }

    #[test]
    fn test_display_object_key_escapes() {
        let v = Value::Object(vec![
            ("a\"b".to_string(), Value::Number(1.0)),
            ("c\\d".to_string(), Value::Number(2.0)),
            ("e\nf".to_string(), Value::Number(3.0)),
        ]);
        assert_eq!(v.to_string(), r#"{"a\"b":1,"c\\d":2,"e\nf":3}"#);
    }

    #[test]
    fn test_display_object_key_control_chars() {
        let v = Value::Object(vec![("\u{08}\u{0C}\u{01}".to_string(), Value::Null)]);
        assert_eq!(v.to_string(), r#"{"\b\f\u0001":null}"#);
    }

    #[test]
    fn test_display_roundtrip() {
        let input = r#"{"name":"test","values":[1,2,3],"nested":{"flag":true}}"#;
        let v = parse_rrjson(input).unwrap();
        let output = v.to_string();
        let reparsed = parse_rrjson(&output).unwrap();
        assert_eq!(v, reparsed);
    }

    #[test]
    fn test_scientific_notation() {
        let v = parse_rrjson(r#"{"n": 1.5e2}"#).unwrap();
        assert_eq!(v["n"], Value::Number(150.0));
    }

    // -- Nesting depth limits ----------------------------------------------------

    #[test]
    fn test_deeply_nested_objects_rejected() {
        // Build nesting that exceeds MAX_NESTING_DEPTH
        let open: String = "{\"a\":".repeat(MAX_NESTING_DEPTH + 1);
        let close: String = "}".repeat(MAX_NESTING_DEPTH + 1);
        let input = format!("{open}1{close}");
        let result = parse_rrjson(&input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("nesting depth"),
            "error should mention nesting depth: {}",
            err.message
        );
    }

    #[test]
    fn test_deeply_nested_arrays_rejected() {
        let open: String = "[".repeat(MAX_NESTING_DEPTH + 1);
        let close: String = "]".repeat(MAX_NESTING_DEPTH + 1);
        let input = format!("{open}1{close}");
        let result = parse_rrjson(&input);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("nesting depth"),
            "error should mention nesting depth: {}",
            err.message
        );
    }

    #[test]
    fn test_nesting_at_max_depth_accepted() {
        // Exactly MAX_NESTING_DEPTH levels should be accepted
        let open: String = "{\"a\":".repeat(MAX_NESTING_DEPTH);
        let close: String = "}".repeat(MAX_NESTING_DEPTH);
        let input = format!("{open}1{close}");
        let result = parse_rrjson(&input);
        assert!(result.is_ok(), "nesting at exactly max depth should work");
    }

    #[test]
    fn test_mixed_nesting_depth_rejected() {
        // Mix objects and arrays to exceed depth
        let mut input = String::new();
        for i in 0..=MAX_NESTING_DEPTH {
            if i % 2 == 0 {
                input.push_str("{\"a\":");
            } else {
                input.push('[');
            }
        }
        input.push('1');
        for i in (0..=MAX_NESTING_DEPTH).rev() {
            if i % 2 == 0 {
                input.push('}');
            } else {
                input.push(']');
            }
        }
        let result = parse_rrjson(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_dotted_key_exceeding_depth_is_rejected() {
        // Build a dotted key with MAX_NESTING_DEPTH dots (= MAX_NESTING_DEPTH + 1 segments).
        let segments: Vec<String> = (0..=MAX_NESTING_DEPTH).map(|i| format!("k{i}")).collect();
        let deep_key = segments.join(".");
        let input = format!("{deep_key} = 1");
        let result = parse_rrjson(&input);
        assert!(
            result.is_err(),
            "dotted key exceeding depth limit should fail"
        );
    }

    #[test]
    fn test_dotted_key_at_limit_is_accepted() {
        // MAX_NESTING_DEPTH - 1 dots = MAX_NESTING_DEPTH segments — should be accepted.
        let segments: Vec<String> = (0..MAX_NESTING_DEPTH).map(|i| format!("k{i}")).collect();
        let deep_key = segments.join(".");
        let input = format!("{deep_key} = 1");
        let result = parse_rrjson(&input);
        assert!(
            result.is_ok(),
            "dotted key at depth limit should be accepted"
        );
    }

    // --- Non-finite number rejection (#452) ---

    #[test]
    fn test_parse_rejects_infinity() {
        let result = parse_rrjson(r#"{"x": 1e309}"#);
        assert!(result.is_err(), "1e309 should be rejected as infinite");
        assert!(result.unwrap_err().message.contains("out of range"));
    }

    #[test]
    fn test_parse_rejects_negative_infinity() {
        let result = parse_rrjson(r#"{"x": -1e309}"#);
        assert!(result.is_err(), "-1e309 should be rejected as infinite");
    }

    #[test]
    fn test_parse_accepts_large_but_finite_number() {
        let result = parse_rrjson(r#"{"x": 1e308}"#);
        assert!(result.is_ok(), "1e308 is large but finite");
    }

    // --- Entry count limit (#458) ---

    #[test]
    fn test_array_entry_limit_exceeded() {
        assert_eq!(MAX_ENTRIES, 10_000);

        // Build a compact array with MAX_ENTRIES + 1 entries.
        let mut input = String::with_capacity(MAX_ENTRIES * 2 + 2);
        input.push('[');
        for i in 0..=MAX_ENTRIES {
            if i > 0 {
                input.push(',');
            }
            input.push('1');
        }
        input.push(']');
        let result = parse_rrjson(&input);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("maximum array entry count")
        );
    }

    #[test]
    fn test_object_entry_limit_exceeded() {
        assert_eq!(MAX_ENTRIES, 10_000);

        // Build a compact object with MAX_ENTRIES + 1 entries using unquoted keys.
        let mut input = String::with_capacity(MAX_ENTRIES * 10 + 2);
        input.push('{');
        for i in 0..=MAX_ENTRIES {
            if i > 0 {
                input.push(',');
            }
            input.push_str(&format!("k{i}:1"));
        }
        input.push('}');
        let result = parse_rrjson(&input);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .contains("maximum object entry count")
        );
    }

    // --- Empty dotted key segments (#462) ---

    #[test]
    fn test_dotted_key_empty_segment_double_dot() {
        let result = parse_rrjson("a..b = 1");
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("empty segment"));
    }

    #[test]
    fn test_dotted_key_empty_segment_leading_dot() {
        let result = parse_rrjson(r#"{".a": 1}"#);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("empty segment"));
    }

    #[test]
    fn test_dotted_key_empty_segment_trailing_dot() {
        let result = parse_rrjson(r#"{"a.": 1}"#);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("empty segment"));
    }

    #[test]
    fn test_dotted_key_single_dot() {
        let result = parse_rrjson(r#"{".": 1}"#);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("empty segment"));
    }

    #[test]
    fn test_dotted_key_valid_segments() {
        let result = parse_rrjson("a.b.c = 1");
        assert!(result.is_ok());
    }

    // --- Number parser edge cases (#575) ---

    #[test]
    fn test_bare_minus_rejected() {
        let result = parse_rrjson(r#"{"x": -}"#);
        assert!(result.is_err());
        let msg = result.unwrap_err().message;
        assert!(
            msg.contains("expected digit"),
            "bare minus should produce digit error, got: {msg}"
        );
    }

    #[test]
    fn test_minus_dot_rejected() {
        let result = parse_rrjson(r#"{"x": -.}"#);
        assert!(result.is_err());
        let msg = result.unwrap_err().message;
        assert!(
            msg.contains("expected digit"),
            "minus-dot should produce digit error, got: {msg}"
        );
    }

    #[test]
    fn test_minus_dot_five_accepted() {
        // -.5 has digits after the decimal point, so it is a valid number.
        let result = parse_rrjson(r#"{"x": -.5}"#);
        assert!(result.is_ok());
        let obj = result.unwrap();
        if let Value::Object(entries) = obj {
            let val = entries.iter().find(|(k, _)| k == "x").map(|(_, v)| v);
            assert_eq!(val, Some(&Value::Number(-0.5)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_negative_number_accepted() {
        let result = parse_rrjson(r#"{"x": -42}"#);
        assert!(result.is_ok());
        let obj = result.unwrap();
        if let Value::Object(entries) = obj {
            let val = entries.iter().find(|(k, _)| k == "x").map(|(_, v)| v);
            assert_eq!(val, Some(&Value::Number(-42.0)));
        } else {
            panic!("expected object");
        }
    }

    // --- Value::Display i64 boundary (#576) ---

    #[test]
    fn test_display_i64_max_boundary_no_overflow() {
        // 9223372036854775808.0 is just above i64::MAX (9223372036854775807).
        // It must NOT be displayed as a negative number from i64 overflow.
        let val = Value::Number(9_223_372_036_854_775_808.0);
        let s = val.to_string();
        assert!(
            !s.starts_with('-'),
            "value near i64::MAX must not overflow to negative: {s}"
        );
    }

    #[test]
    fn test_display_i64_min_boundary() {
        // i64::MIN as f64 is exact, so it should display as an integer.
        let val = Value::Number(i64::MIN as f64);
        assert_eq!(val.to_string(), "-9223372036854775808");
    }

    #[test]
    fn test_display_normal_integer() {
        let val = Value::Number(42.0);
        assert_eq!(val.to_string(), "42");
    }

    #[test]
    fn test_display_fractional() {
        let val = Value::Number(1.5);
        assert_eq!(val.to_string(), "1.5");
    }

    // --- Additional edge case coverage (#580) ---

    #[test]
    fn test_display_nan() {
        let val = Value::Number(f64::NAN);
        let s = val.to_string();
        assert_eq!(s, "NaN");
    }

    #[test]
    fn test_empty_input() {
        let result = parse_rrjson("");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Object(vec![]));
    }

    #[test]
    fn test_whitespace_only_input() {
        let result = parse_rrjson("   \n\t  ");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Object(vec![]));
    }

    #[test]
    fn test_comment_only_input() {
        let result = parse_rrjson("// just a comment\n# another comment");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Object(vec![]));
    }

    #[test]
    fn test_duplicate_keys_last_wins() {
        let result = parse_rrjson(r#"{"a": 1, "a": 2}"#).unwrap();
        if let Value::Object(entries) = result {
            // Duplicate key should be deduplicated to exactly one entry.
            let vals: Vec<_> = entries
                .iter()
                .filter(|(k, _)| k == "a")
                .map(|(_, v)| v)
                .collect();
            assert_eq!(vals.len(), 1);
            assert_eq!(vals[0], &Value::Number(2.0));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_semicolon_separator_in_bare_object() {
        // RRJSON allows semicolons as separators in bare key-value pairs.
        let result = parse_rrjson("a = 1; b = 2");
        assert!(result.is_ok());
        let obj = result.unwrap();
        if let Value::Object(entries) = obj {
            assert!(
                entries
                    .iter()
                    .any(|(k, v)| k == "a" && *v == Value::Number(1.0))
            );
            assert!(
                entries
                    .iter()
                    .any(|(k, v)| k == "b" && *v == Value::Number(2.0))
            );
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_uppercase_exponent() {
        let result = parse_rrjson(r#"{"x": 1E10}"#).unwrap();
        if let Value::Object(entries) = result {
            let val = entries.iter().find(|(k, _)| k == "x").map(|(_, v)| v);
            assert_eq!(val, Some(&Value::Number(1e10)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_display_roundtrip_unicode() {
        let original = r#"{"emoji":"🎵","cjk":"日本語"}"#;
        let parsed = parse_rrjson(original).unwrap();
        let output = parsed.to_string();
        let reparsed = parse_rrjson(&output).unwrap();
        assert_eq!(parsed, reparsed);
    }

    #[test]
    fn test_display_roundtrip_escape_sequences() {
        let original = r#"{"tab":"\t","newline":"\n","backslash":"\\"}"#;
        let parsed = parse_rrjson(original).unwrap();
        let output = parsed.to_string();
        let reparsed = parse_rrjson(&output).unwrap();
        assert_eq!(parsed, reparsed);
    }
}
