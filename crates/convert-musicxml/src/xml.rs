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

/// Parse an XML string into an element tree.
///
/// Returns the root element, or an error message if parsing fails.
pub(crate) fn parse(xml: &str) -> Result<Element, String> {
    let mut p = Parser {
        src: xml.as_bytes(),
        pos: 0,
    };
    p.skip_bom();
    p.skip_prolog()?;
    p.skip_whitespace_and_comments()?;
    p.parse_element()
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
    fn parse_element(&mut self) -> Result<Element, String> {
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
        let (text, children) = self.parse_children(&raw_name)?;

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
    /// Returns `(text_content, child_elements)`.
    fn parse_children(&mut self, parent_raw_name: &str) -> Result<(String, Vec<Element>), String> {
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
                let child = self.parse_element()?;
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
                if let Some(semi) = rest.find(';') {
                    let entity = &rest[..semi];
                    let ch = if let Some(code_str) = entity.strip_prefix('#') {
                        // Numeric character reference: &#N; or &#xN; / &#XN;.
                        // XML 1.0 §2.2 forbids U+0000, so reject it explicitly.
                        let code_point = if let Some(hex) = code_str
                            .strip_prefix('x')
                            .or_else(|| code_str.strip_prefix('X'))
                        {
                            u32::from_str_radix(hex, 16).ok()
                        } else {
                            code_str.parse::<u32>().ok()
                        };
                        code_point.and_then(char::from_u32).filter(|&c| c != '\0')
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
}
