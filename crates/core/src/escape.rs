//! XML/HTML character escaping utilities.
//!
//! Provides a single shared escape function used by both the SVG chord diagram
//! generator in `chordpro-core` and the HTML renderer.

/// Escape the five XML special characters.
///
/// Replaces `&`, `<`, `>`, `"`, and `'` with their corresponding XML
/// character references. This is sufficient for both XML attribute values
/// and HTML text content.
///
/// # Examples
///
/// ```
/// use chordpro_core::escape::escape_xml;
///
/// assert_eq!(escape_xml("A&B"), "A&amp;B");
/// assert_eq!(escape_xml("it's"), "it&apos;s");
/// ```
#[must_use]
pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_ampersand() {
        assert_eq!(escape_xml("A&B"), "A&amp;B");
    }

    #[test]
    fn escapes_angle_brackets() {
        assert_eq!(escape_xml("A<B>C"), "A&lt;B&gt;C");
    }

    #[test]
    fn escapes_double_quote() {
        assert_eq!(escape_xml(r#"say "hi""#), "say &quot;hi&quot;");
    }

    #[test]
    fn escapes_single_quote() {
        assert_eq!(escape_xml("it's"), "it&apos;s");
    }

    #[test]
    fn no_escaping_needed() {
        assert_eq!(escape_xml("Am7"), "Am7");
    }

    #[test]
    fn empty_string() {
        assert_eq!(escape_xml(""), "");
    }

    #[test]
    fn all_special_chars() {
        assert_eq!(escape_xml("&<>\"'"), "&amp;&lt;&gt;&quot;&apos;");
    }
}
