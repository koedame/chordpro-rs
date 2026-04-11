//! Minimal XML DOM builder for MusicXML files.
//!
//! This is a purpose-built parser that handles the subset of XML used by
//! MusicXML 4.0 files. It is not a general-purpose XML parser and does not
//! attempt to implement the full XML specification.
//!
//! # Supported features
//!
//! - XML declarations (`<?xml ...?>`)
//! - DOCTYPE declarations (skipped)
//! - Comments (`<!-- ... -->`) — skipped
//! - Processing instructions (`<?...?>`) — skipped
//! - Element start/end tags
//! - Self-closing elements (`<tag/>`)
//! - Attributes with single or double quoted values
//! - Text content
//! - CDATA sections (`<![CDATA[...]]>`) — treated as text
//! - Standard entity references (`&amp;`, `&lt;`, `&gt;`, `&quot;`, `&apos;`)
//! - Decimal numeric character references (`&#N;`)
//! - Hexadecimal numeric character references (`&#xN;`, `&#XN;`)

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A parsed XML element.
#[derive(Debug, Default)]
pub(crate) struct Element {
    /// The element's tag name (without namespace prefix).
    pub name: String,
    /// Attribute map (namespace prefixes stripped from attribute names).
    pub attrs: HashMap<String, String>,
    /// Concatenated text content of direct text nodes (trimmed).
    pub text: String,
    /// Child elements in document order.
    pub children: Vec<Element>,
}

impl Element {
    /// Returns the first child element with the given local name, if any.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Returns an iterator over all child elements with the given local name.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Navigates `path` from this element and returns the element at that
    /// path, or `None` if any step is missing.
    pub fn at_path(&self, path: &[&str]) -> Option<&Element> {
        match path {
            [] => Some(self),
            [first, rest @ ..] => self.child(first)?.at_path(rest),
        }
    }

    /// Returns the trimmed text content of the element at `path`.
    pub fn text_at(&self, path: &[&str]) -> Option<&str> {
        Some(self.at_path(path)?.text.trim())
    }

    /// Returns the value of the attribute with the given local name.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Maximum XML element nesting depth accepted by the parser.
///
/// Deeply nested documents are rejected with an error rather than overflowing
/// the Rust call stack. Legitimate MusicXML files are shallow (typically under
/// 20 levels); 200 is a generous ceiling that still prevents unbounded recursion
/// from adversarial input.
const MAX_DEPTH: usize = 200;

/// Maximum input size accepted by the parser (50 MiB).
///
/// Inputs larger than this limit are rejected at the `parse()` entry point to
/// prevent memory exhaustion from flat-wide adversarial documents — e.g., a
/// document with millions of siblings, enormous text nodes, or millions of
/// attributes that bypass the depth limit by operating horizontally within a
/// single level. Real MusicXML files are typically well under 10 MB; 50 MiB is
/// a generous ceiling. See #1565.
///
/// This limit also makes the `i32` bracket-depth counter in `skip_doctype`
/// overflow-safe: even if the entire input consists of `[` characters, 50 MiB
/// is well below `i32::MAX` (≈ 2 GiB).
const MAX_INPUT_BYTES: usize = 50 * 1024 * 1024; // 50 MiB

/// Parse an XML string into an element tree.
///
/// Returns the root element, or an error message if parsing fails.
///
/// # Errors
///
/// Returns an error if:
/// - The input exceeds [`MAX_INPUT_BYTES`] (50 MiB)
/// - The XML is malformed
/// - The element nesting depth exceeds [`MAX_DEPTH`]
pub(crate) fn parse(xml: &str) -> Result<Element, String> {
    if xml.len() > MAX_INPUT_BYTES {
        return Err(format!(
            "MusicXML input too large: {} bytes (limit is {} bytes)",
            xml.len(),
            MAX_INPUT_BYTES
        ));
    }
    let mut p = Parser {
        src: xml.as_bytes(),
        pos: 0,
    };
    p.skip_bom();
    p.skip_prolog()?;
    p.skip_whitespace_and_comments()?;
    p.parse_element(0)
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    // --- helpers -----------------------------------------------------------

    fn remaining(&self) -> usize {
        self.src.len().saturating_sub(self.pos)
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn starts_with(&self, pat: &[u8]) -> bool {
        self.src.get(self.pos..).is_some_and(|s| s.starts_with(pat))
    }

    fn advance(&mut self) {
        if self.pos < self.src.len() {
            self.pos += 1;
        }
    }

    fn consume_char(&mut self) -> Option<u8> {
        let b = self.src.get(self.pos).copied()?;
        self.pos += 1;
        Some(b)
    }

    fn expect(&mut self, pat: &[u8]) -> Result<(), String> {
        if self.starts_with(pat) {
            self.pos += pat.len();
            Ok(())
        } else {
            let got = &self.src[self.pos..self.pos.min(self.src.len()).min(self.pos + 8)];
            Err(format!(
                "expected {:?} at offset {} but got {:?}",
                std::str::from_utf8(pat).unwrap_or("?"),
                self.pos,
                std::str::from_utf8(got).unwrap_or("?")
            ))
        }
    }

    fn slice_str(&self, start: usize, end: usize) -> &'a str {
        std::str::from_utf8(&self.src[start..end]).unwrap_or("")
    }

    // --- skip utilities ----------------------------------------------------

    fn skip_bom(&mut self) {
        // UTF-8 BOM: EF BB BF
        if self.starts_with(&[0xEF, 0xBB, 0xBF]) {
            self.pos += 3;
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Skip whitespace and XML comments.
    fn skip_whitespace_and_comments(&mut self) -> Result<(), String> {
        loop {
            self.skip_whitespace();
            if self.starts_with(b"<!--") {
                self.skip_comment()?;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn skip_comment(&mut self) -> Result<(), String> {
        self.expect(b"<!--")?;
        loop {
            if self.remaining() < 3 {
                return Err("unterminated comment".to_string());
            }
            if self.starts_with(b"-->") {
                self.pos += 3;
                return Ok(());
            }
            self.advance();
        }
    }

    /// Skip the XML prolog: declaration, DOCTYPE, and leading comments/PIs.
    fn skip_prolog(&mut self) -> Result<(), String> {
        loop {
            self.skip_whitespace();
            if self.starts_with(b"<?") {
                // Processing instruction or XML declaration — skip to ?>
                self.pos += 2;
                loop {
                    if self.remaining() < 2 {
                        return Err("unterminated processing instruction".to_string());
                    }
                    if self.starts_with(b"?>") {
                        self.pos += 2;
                        break;
                    }
                    self.advance();
                }
            } else if self.starts_with(b"<!--") {
                self.skip_comment()?;
            } else if self.starts_with(b"<!DOCTYPE") || self.starts_with(b"<!doctype") {
                self.skip_doctype()?;
            } else {
                break;
            }
        }
        Ok(())
    }

    fn skip_doctype(&mut self) -> Result<(), String> {
        // Called when the current position is at `<!DOCTYPE` or `<!doctype`.
        // Advance past the opening `<!` and scan to the matching `>`,
        // tracking bracket depth for internal `[...]` subsets.
        debug_assert!(self.starts_with(b"<!"));
        self.advance();
        self.advance();
        let mut depth = 1i32;
        loop {
            match self.consume_char() {
                None => return Err("unterminated DOCTYPE".to_string()),
                Some(b'[') => depth += 1,
                Some(b']') => depth -= 1,
                Some(b'>') if depth <= 1 => return Ok(()),
                _ => {}
            }
        }
    }

    // --- main parser -------------------------------------------------------

    /// Parse an element: `<name attrs> children </name>` or `<name attrs/>`.
    ///
    /// `depth` is the current nesting level (0 for the root element).
    /// Returns an error if `depth` reaches [`MAX_DEPTH`].
    fn parse_element(&mut self, depth: usize) -> Result<Element, String> {
        if depth >= MAX_DEPTH {
            return Err(format!("XML nesting depth limit of {} exceeded", MAX_DEPTH));
        }
        self.skip_whitespace_and_comments()?;

        // Consume `<`
        if self.peek() != Some(b'<') {
            return Err(format!(
                "expected '<' at offset {} but got {:?}",
                self.pos,
                self.peek().map(char::from)
            ));
        }
        self.advance();

        // Read tag name (may contain `:` for namespaced elements — keep it)
        let name_start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':' {
                self.advance();
            } else {
                break;
            }
        }
        if self.pos == name_start {
            return Err(format!("empty tag name at offset {}", self.pos));
        }
        let raw_name = self.slice_str(name_start, self.pos).to_string();
        // Strip namespace prefix for usability (keep local name only)
        let name = local_name(&raw_name).to_string();

        // Read attributes
        let attrs = self.parse_attrs()?;

        self.skip_whitespace();

        // Self-closing?
        if self.starts_with(b"/>") {
            self.pos += 2;
            return Ok(Element {
                name,
                attrs,
                text: String::new(),
                children: Vec::new(),
            });
        }

        self.expect(b">")?;

        // Read children
        let (text, children) = self.parse_children(&raw_name, depth)?;

        Ok(Element {
            name,
            attrs,
            text,
            children,
        })
    }

    fn parse_attrs(&mut self) -> Result<HashMap<String, String>, String> {
        let mut attrs = HashMap::new();
        loop {
            self.skip_whitespace();
            // Stop at `>` or `/>`
            if self.peek() == Some(b'>') || self.starts_with(b"/>") {
                break;
            }
            if self.peek().is_none() {
                return Err("unexpected EOF inside tag".to_string());
            }
            // Read attribute name
            let aname_start = self.pos;
            while let Some(b) = self.peek() {
                if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':' {
                    self.advance();
                } else {
                    break;
                }
            }
            if self.pos == aname_start {
                // Skip any unexpected character
                self.advance();
                continue;
            }
            let raw_aname = self.slice_str(aname_start, self.pos).to_string();
            let aname = local_name(&raw_aname).to_string();

            self.skip_whitespace();
            if self.peek() != Some(b'=') {
                // Boolean attribute (no value) — store empty string
                attrs.insert(aname, String::new());
                continue;
            }
            self.advance(); // consume `=`
            self.skip_whitespace();

            // Read value
            let quote = match self.peek() {
                Some(q @ b'"') | Some(q @ b'\'') => {
                    self.advance();
                    q
                }
                _ => {
                    return Err(format!(
                        "expected quote for attribute value at offset {}",
                        self.pos
                    ));
                }
            };
            let vstart = self.pos;
            while let Some(b) = self.peek() {
                if b == quote {
                    break;
                }
                self.advance();
            }
            let raw_val = self.slice_str(vstart, self.pos).to_string();
            if self.peek() == Some(quote) {
                self.advance(); // consume closing quote
            }
            attrs.insert(aname, decode_entities(&raw_val));
        }
        Ok(attrs)
    }

    /// Parse children of an element with tag `parent_raw_name`.
    ///
    /// `depth` is the nesting level of the *parent* element; child elements are
    /// parsed at `depth + 1`.
    ///
    /// Returns `(text_content, child_elements)`.
    fn parse_children(
        &mut self,
        parent_raw_name: &str,
        depth: usize,
    ) -> Result<(String, Vec<Element>), String> {
        let parent_local = local_name(parent_raw_name);
        let mut text = String::new();
        let mut children = Vec::new();

        loop {
            // CDATA
            if self.starts_with(b"<![CDATA[") {
                self.pos += 9;
                let tstart = self.pos;
                loop {
                    if self.remaining() < 3 {
                        return Err("unterminated CDATA section".to_string());
                    }
                    if self.starts_with(b"]]>") {
                        let t = self.slice_str(tstart, self.pos);
                        text.push_str(t);
                        self.pos += 3;
                        break;
                    }
                    self.advance();
                }
                continue;
            }

            // Comment
            if self.starts_with(b"<!--") {
                self.skip_comment()?;
                continue;
            }

            // Processing instruction
            if self.starts_with(b"<?") {
                self.pos += 2;
                loop {
                    if self.remaining() < 2 {
                        return Err("unterminated PI in element body".to_string());
                    }
                    if self.starts_with(b"?>") {
                        self.pos += 2;
                        break;
                    }
                    self.advance();
                }
                continue;
            }

            // End tag
            if self.starts_with(b"</") {
                self.pos += 2;
                self.skip_whitespace();
                let tstart = self.pos;
                while let Some(b) = self.peek() {
                    if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':'
                    {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let closing_raw = self.slice_str(tstart, self.pos);
                let closing_local = local_name(closing_raw);
                self.skip_whitespace();
                // Consume `>`
                if self.peek() == Some(b'>') {
                    self.advance();
                }
                if closing_local != parent_local {
                    // Mismatched end tag — tolerate by ignoring (robustness)
                }
                break;
            }

            // Child element
            if self.peek() == Some(b'<') {
                let child = self.parse_element(depth + 1)?;
                children.push(child);
                continue;
            }

            // Text
            if self.peek().is_none() {
                break;
            }
            let tstart = self.pos;
            while let Some(b) = self.peek() {
                if b == b'<' {
                    break;
                }
                self.advance();
            }
            let raw_text = self.slice_str(tstart, self.pos);
            let decoded = decode_entities(raw_text);
            text.push_str(&decoded);
        }

        Ok((text.trim().to_string(), children))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip the namespace prefix from a qualified name, returning the local name.
///
/// `"ns:foo"` → `"foo"`, `"foo"` → `"foo"`.
fn local_name(name: &str) -> &str {
    name.rfind(':').map_or(name, |i| &name[i + 1..])
}

/// Returns `true` if `c` is a legal XML 1.0 character (XML 1.0 §2.2).
///
/// XML 1.0 permits: U+0009, U+000A, U+000D, U+0020–U+D7FF, U+E000–U+FFFD,
/// U+10000–U+10FFFF. Everything else — including U+0000, U+0001–U+0008,
/// U+000B–U+000C, U+000E–U+001F, U+FFFE, U+FFFF — is forbidden.
fn xml_char_allowed(c: char) -> bool {
    matches!(c,
        '\t' | '\n' | '\r'
        | '\u{0020}'..='\u{D7FF}'
        | '\u{E000}'..='\u{FFFD}'
        | '\u{10000}'..='\u{10FFFF}'
    )
}

/// Decode XML predefined entity references and numeric character references.
///
/// Handles the five predefined named entities (`&amp;`, `&lt;`, `&gt;`,
/// `&quot;`, `&apos;`) as well as decimal (`&#N;`) and hexadecimal
/// (`&#xN;` / `&#XN;`) numeric character references.
fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '&' {
            // Borrow `chars` as a `&str` only for the lookup; record the
            // result as owned values before mutably advancing the iterator.
            let (skip, replacement) = {
                let rest = chars.as_str();
                // Scan at most 16 *characters* for ';': the longest valid
                // entity body is 8 chars (`#x10FFFF` or `#1114111`) plus ';',
                // so 16 is a safe upper bound.  Using char_indices avoids
                // byte-slicing a &str at an offset that may not be a valid
                // char boundary when the input contains non-ASCII text.
                let semi_opt = rest
                    .char_indices()
                    .take(16)
                    .find_map(|(byte_i, c)| (c == ';').then_some(byte_i));
                if let Some(semi) = semi_opt {
                    let entity = &rest[..semi];
                    let ch = if let Some(code_str) = entity.strip_prefix('#') {
                        // Numeric character reference: &#N; or &#xN; / &#XN;.
                        let code_point = if let Some(hex) = code_str
                            .strip_prefix('x')
                            .or_else(|| code_str.strip_prefix('X'))
                        {
                            u32::from_str_radix(hex, 16).ok()
                        } else {
                            code_str.parse::<u32>().ok()
                        };
                        code_point
                            .and_then(char::from_u32)
                            .filter(|&c| xml_char_allowed(c))
                    } else {
                        // Named entity reference
                        match entity {
                            "amp" => Some('&'),
                            "lt" => Some('<'),
                            "gt" => Some('>'),
                            "quot" => Some('"'),
                            "apos" => Some('\''),
                            _ => None,
                        }
                    };
                    // All recognised entity bodies (named entities and numeric
                    // digit strings) are ASCII-only, so `semi` (a byte offset)
                    // equals the char count up to the semicolon, and
                    // `semi + 1` is the total chars to skip including `;`.
                    debug_assert!(entity.is_ascii(), "entity body must be ASCII");
                    (semi + 1, ch)
                } else {
                    (0, None)
                }
            };
            if let Some(ch) = replacement {
                out.push(ch);
                for _ in 0..skip {
                    chars.next();
                }
            } else {
                // Unknown or malformed reference — emit `&` literally.
                // Subsequent iterations will output the entity name and
                // semicolon (if any) as ordinary characters, preserving the
                // original text verbatim.
                out.push('&');
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_element() {
        let doc = "<root><child>hello</child></root>";
        let root = parse(doc).unwrap();
        assert_eq!(root.name, "root");
        assert_eq!(root.child("child").unwrap().text, "hello");
    }

    #[test]
    fn parse_self_closing() {
        let doc = "<root><br/><hr /></root>";
        let root = parse(doc).unwrap();
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].name, "br");
        assert_eq!(root.children[1].name, "hr");
    }

    #[test]
    fn parse_attributes() {
        let doc = r#"<root id="42" name='hello'></root>"#;
        let root = parse(doc).unwrap();
        assert_eq!(root.attr("id"), Some("42"));
        assert_eq!(root.attr("name"), Some("hello"));
    }

    #[test]
    fn parse_entities() {
        let doc = "<root>&amp;&lt;&gt;&quot;&apos;</root>";
        let root = parse(doc).unwrap();
        assert_eq!(root.text, "&<>\"'");
    }

    #[test]
    fn parse_with_prolog() {
        let doc = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE root PUBLIC "-//Test//DTD//EN" "test.dtd">
<!-- a comment -->
<root><item>42</item></root>"#;
        let root = parse(doc).unwrap();
        assert_eq!(root.name, "root");
        assert_eq!(root.child("item").unwrap().text, "42");
    }

    #[test]
    fn parse_namespace_prefix() {
        let doc = r#"<ns:root xmlns:ns="http://example.com"><ns:child>text</ns:child></ns:root>"#;
        let root = parse(doc).unwrap();
        assert_eq!(root.name, "root");
        assert_eq!(root.child("child").unwrap().text, "text");
    }

    #[test]
    fn parse_cdata() {
        let doc = "<root><![CDATA[<not-a-tag>]]></root>";
        let root = parse(doc).unwrap();
        assert_eq!(root.text, "<not-a-tag>");
    }

    #[test]
    fn at_path() {
        let doc = "<a><b><c>deep</c></b></a>";
        let root = parse(doc).unwrap();
        assert_eq!(root.text_at(&["b", "c"]), Some("deep"));
    }

    #[test]
    fn decode_entities_standalone() {
        assert_eq!(decode_entities("a &amp; b"), "a & b");
        assert_eq!(decode_entities("&lt;tag&gt;"), "<tag>");
        assert_eq!(decode_entities("no entities"), "no entities");
    }

    #[test]
    fn decode_entities_all_named() {
        assert_eq!(decode_entities("&amp;"), "&");
        assert_eq!(decode_entities("&lt;"), "<");
        assert_eq!(decode_entities("&gt;"), ">");
        assert_eq!(decode_entities("&quot;"), "\"");
        assert_eq!(decode_entities("&apos;"), "'");
    }

    #[test]
    fn decode_entities_numeric_decimal() {
        // &#60; = '<', &#62; = '>', &#38; = '&'
        assert_eq!(decode_entities("&#60;"), "<");
        assert_eq!(decode_entities("&#62;"), ">");
        assert_eq!(decode_entities("&#38;"), "&");
        assert_eq!(decode_entities("&#65;"), "A");
    }

    #[test]
    fn decode_entities_numeric_hex() {
        // &#x3C; = '<', &#X3E; = '>' (uppercase X), &#x26; = '&'
        assert_eq!(decode_entities("&#x3C;"), "<");
        assert_eq!(decode_entities("&#X3E;"), ">");
        assert_eq!(decode_entities("&#x26;"), "&");
        assert_eq!(decode_entities("&#xA;"), "\n");
    }

    #[test]
    fn decode_entities_unknown_passed_through() {
        // Unknown named entities are left unchanged
        assert_eq!(decode_entities("&foo;"), "&foo;");
        assert_eq!(decode_entities("&unknown;bar"), "&unknown;bar");
    }

    #[test]
    fn decode_entities_null_char_not_decoded() {
        // XML 1.0 §2.2 forbids U+0000; &#0; must not produce a null byte.
        assert_eq!(decode_entities("&#0;"), "&#0;");
        assert_eq!(decode_entities("&#x0;"), "&#x0;");
        assert_eq!(decode_entities("&#X0;"), "&#X0;");
    }

    #[test]
    fn parse_with_doctype_internal_subset() {
        // DOCTYPE with an internal `[...]` subset should be skipped correctly.
        let doc = r#"<?xml version="1.0"?>
<!DOCTYPE root [
  <!ELEMENT root (item)>
  <!ELEMENT item (#PCDATA)>
]>
<root><item>hello</item></root>"#;
        let root = parse(doc).unwrap();
        assert_eq!(root.name, "root");
        assert_eq!(root.child("item").unwrap().text, "hello");
    }

    #[test]
    fn parse_with_lowercase_doctype() {
        // `<!doctype` (lowercase) should also be skipped.
        let doc = "<!doctype root><root><child>text</child></root>";
        let root = parse(doc).unwrap();
        assert_eq!(root.name, "root");
        assert_eq!(root.child("child").unwrap().text, "text");
    }

    #[test]
    fn decode_entities_xml_forbidden_chars_not_decoded() {
        // XML 1.0 §2.2 forbidden characters must not be decoded.
        // U+0001–U+0008 (control chars)
        assert_eq!(decode_entities("&#1;"), "&#1;");
        assert_eq!(decode_entities("&#8;"), "&#8;");
        // U+000B (vertical tab), U+000C (form feed)
        assert_eq!(decode_entities("&#11;"), "&#11;");
        assert_eq!(decode_entities("&#12;"), "&#12;");
        // U+000E–U+001F (more control chars)
        assert_eq!(decode_entities("&#14;"), "&#14;");
        assert_eq!(decode_entities("&#31;"), "&#31;");
        // U+FFFE and U+FFFF
        assert_eq!(decode_entities("&#xFFFE;"), "&#xFFFE;");
        assert_eq!(decode_entities("&#xFFFF;"), "&#xFFFF;");
    }

    #[test]
    fn decode_entities_adversarial_numeric_refs() {
        // Surrogate codepoint (U+D800): char::from_u32 returns None
        assert_eq!(decode_entities("&#xD800;"), "&#xD800;");
        // Above Unicode range (U+110000): char::from_u32 returns None
        assert_eq!(decode_entities("&#x110000;"), "&#x110000;");
        // Empty numeric reference (&#;): should pass through literally
        assert_eq!(decode_entities("&#;"), "&#;");
        // Invalid hex digits (&#xGG;): parse fails, pass through literally
        assert_eq!(decode_entities("&#xGG;"), "&#xGG;");
        // Overflow beyond u32::MAX: parse fails, pass through literally
        assert_eq!(decode_entities("&#4294967296;"), "&#4294967296;");
    }

    #[test]
    fn decode_entities_no_quadratic_scan() {
        // A string of bare `&` characters (no semicolons) should not cause
        // O(n²) scanning. The 16-char window limit keeps each scan O(1).
        // Just verify the output is correct (all `&` passed through).
        let input = "&".repeat(1000);
        let output = decode_entities(&input);
        assert_eq!(output, input);
    }

    #[test]
    fn parse_depth_limit_rejected() {
        // Build an XML document with MAX_DEPTH + 1 levels of nesting.
        // The parser must return an error rather than overflowing the call stack.
        let open: String = (0..=MAX_DEPTH).map(|i| format!("<a{i}>")).collect();
        let close: String = (0..=MAX_DEPTH).rev().map(|i| format!("</a{i}>")).collect();
        let doc = format!("{open}{close}");
        let result = parse(&doc);
        assert!(
            result.is_err(),
            "expected depth-limit error but got Ok for {}-level nesting",
            MAX_DEPTH + 1
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("depth limit"),
            "error message should mention depth limit: {msg}"
        );
    }

    #[test]
    fn parse_depth_at_limit_accepted() {
        // MAX_DEPTH levels of nesting (indices 0..MAX_DEPTH-1 are depths 0 to 199) must succeed.
        let open: String = (0..MAX_DEPTH).map(|i| format!("<a{i}>")).collect();
        let close: String = (0..MAX_DEPTH).rev().map(|i| format!("</a{i}>")).collect();
        let doc = format!("{open}{close}");
        assert!(
            parse(&doc).is_ok(),
            "expected Ok for {}-level nesting but got an error",
            MAX_DEPTH
        );
    }

    #[test]
    fn decode_entities_multibyte_chars_near_ampersand_no_panic() {
        // Regression test: before the fix, `&rest[..rest.len().min(16)]` would
        // panic when a multi-byte UTF-8 character straddled byte offset 16.
        // E.g. 14 ASCII bytes followed by a 3-byte CJK character — byte 16
        // lands inside the character.  Verify the entity is passed through
        // literally (no replacement) without panicking.
        //
        // "aaaaaaaaaaaaaa" = 14 bytes, '日' = 3 bytes => byte 16 is mid-char.
        let input = "&aaaaaaaaaaaaaa\u{65E5}xyz";
        let output = decode_entities(input);
        assert_eq!(output, input);

        // Also verify a valid named entity after multi-byte text still works.
        let input2 = "café &amp; more";
        assert_eq!(decode_entities(input2), "café & more");
    }

    #[test]
    fn parse_rejects_oversized_input() {
        // An input larger than MAX_INPUT_BYTES must be rejected at the entry
        // point before any allocation, preventing memory exhaustion from
        // flat-wide adversarial documents (see #1565).
        let oversized = "A".repeat(MAX_INPUT_BYTES + 1);
        let result = parse(&oversized);
        assert!(result.is_err(), "expected error for oversized input");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("too large"),
            "error message should mention 'too large': {msg}"
        );
    }

    #[test]
    fn parse_accepts_input_at_limit() {
        // An input of exactly MAX_INPUT_BYTES must not be rejected by the size
        // check (it may still fail for other reasons — malformed XML — but the
        // size guard must not trigger).
        let at_limit = "A".repeat(MAX_INPUT_BYTES);
        let result = parse(&at_limit);
        // The input is not valid XML, so it will fail, but the error must NOT
        // mention "too large" (the size guard should not fire).
        if let Err(msg) = result {
            assert!(
                !msg.contains("too large"),
                "size guard should not fire at exactly MAX_INPUT_BYTES, got: {msg}"
            );
        }
    }
}
