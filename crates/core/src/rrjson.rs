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

impl Value {
    /// Look up a key in an Object value. Returns `Value::Null` if not found
    /// or if `self` is not an Object.
    #[must_use]
    pub fn get(&self, key: &str) -> &Value {
        static NULL: Value = Value::Null;
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
}

impl core::ops::Index<&str> for Value {
    type Output = Value;

    fn index(&self, key: &str) -> &Value {
        self.get(key)
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

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a RRJSON string into a [`Value`].
///
/// Accepts strict JSON, relaxed JSON, and RRJSON formats.
///
/// # Errors
///
/// Returns a [`ParseError`] if the input is malformed.
pub fn parse_rrjson(input: &str) -> Result<Value, ParseError> {
    let mut parser = Parser::new(input);
    parser.skip_ws_and_comments()?;

    // RRJSON: if the input doesn't start with '{' or '[', treat it as
    // bare key-value pairs (implicit object).
    if parser.peek() != Some('{') && parser.peek() != Some('[') {
        return parser.parse_bare_object();
    }

    let value = parser.parse_value()?;
    parser.skip_ws_and_comments()?;
    if parser.pos < parser.input.len() {
        return Err(parser.error("unexpected content after value"));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// Internal parser
// ---------------------------------------------------------------------------

/// Maximum nesting depth for objects and arrays to prevent stack overflow
/// from deeply nested input.
const MAX_NESTING_DEPTH: usize = 64;

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            depth: 0,
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
            self.insert_dotted_key(&mut entries, &key, value);

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
            Some(c) => Err(self.error(&format!("invalid escape character '{c}'"))),
            None => Err(self.error("unterminated escape sequence")),
        }
    }

    fn parse_number(&mut self) -> Result<Value, ParseError> {
        let start = self.pos;
        if self.peek() == Some('-') {
            self.advance();
        }
        // Integer part
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }
        // Fractional part
        if self.peek() == Some('.') {
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
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

            // Skip "include" directives (just consume the line)
            if self.input[self.pos..].starts_with("include") {
                if let Some(end) = self.input[self.pos..].find('\n') {
                    self.pos += end + 1;
                } else {
                    self.pos = self.input.len();
                }
                continue;
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
            self.insert_dotted_key(&mut entries, &key, value);

            // Optional separator (comma, semicolon, or newline-separated)
            self.skip_ws_and_comments()?;
            if self.peek() == Some(',') || self.peek() == Some(';') {
                self.advance();
            }
        }

        Ok(Value::Object(entries))
    }

    // -- Dot-separated key expansion ------------------------------------------

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
    fn test_value_index_missing_key() {
        let v = Value::Object(vec![]);
        assert!(v["missing"].is_null());
    }

    #[test]
    fn test_value_is_null() {
        assert!(Value::Null.is_null());
        assert!(!Value::Bool(false).is_null());
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
}
