//! Inline markup parser for ChordPro lyrics text.
//!
//! ChordPro lyrics can contain inline markup tags for formatting:
//!
//! - `<bold>text</bold>` or `<b>text</b>` — bold text
//! - `<italic>text</italic>` or `<i>text</i>` — italic text
//! - `<highlight>text</highlight>` — highlighted text
//! - `<comment>text</comment>` — comment-styled text
//! - `<span font_family="..." size="..." foreground="...">text</span>` — styled text
//!
//! Tags may be nested: `<b><i>text</i></b>`.
//!
//! Unclosed tags wrap all remaining text (per ChordPro spec). Unrecognized or
//! malformed tags are treated as plain text (graceful degradation).
//!
//! # Examples
//!
//! ```
//! use chordpro_core::inline_markup::{TextSpan, parse_inline_markup};
//!
//! let spans = parse_inline_markup("<b>Hello</b> world");
//! assert_eq!(spans, vec![
//!     TextSpan::Bold(vec![TextSpan::Plain("Hello".to_string())]),
//!     TextSpan::Plain(" world".to_string()),
//! ]);
//! ```

/// Attributes for a `<span>` inline markup tag.
///
/// Each field corresponds to a style attribute that can be specified on the
/// `<span>` tag. All fields are optional — only attributes present in the
/// source markup are populated.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpanAttributes {
    /// Font family name (e.g., `"Serif"`, `"Monospace"`).
    pub font_family: Option<String>,
    /// Font size (e.g., `"12"`, `"120%"`).
    pub size: Option<String>,
    /// Foreground (text) color (e.g., `"red"`, `"#FF0000"`).
    pub foreground: Option<String>,
    /// Background color (e.g., `"yellow"`, `"#FFFF00"`).
    pub background: Option<String>,
    /// Font weight (e.g., `"bold"`, `"normal"`).
    pub weight: Option<String>,
    /// Font style (e.g., `"italic"`, `"normal"`).
    pub style: Option<String>,
}

/// A segment of text that may contain inline markup formatting.
///
/// `TextSpan` represents a tree structure where each node is either plain text
/// or a formatting tag wrapping child spans. This allows arbitrary nesting of
/// inline markup.
#[derive(Debug, Clone, PartialEq)]
pub enum TextSpan {
    /// Plain text with no formatting.
    Plain(String),
    /// Bold text (`<bold>` / `<b>`).
    Bold(Vec<TextSpan>),
    /// Italic text (`<italic>` / `<i>`).
    Italic(Vec<TextSpan>),
    /// Highlighted text (`<highlight>`).
    Highlight(Vec<TextSpan>),
    /// Comment-styled text (`<comment>`).
    Comment(Vec<TextSpan>),
    /// Styled text with attributes (`<span ...>`).
    Span(SpanAttributes, Vec<TextSpan>),
}

impl TextSpan {
    /// Extracts the plain text content of this span, stripping all formatting.
    ///
    /// This recursively collects all `Plain` text within nested spans.
    #[must_use]
    pub fn plain_text(&self) -> String {
        match self {
            TextSpan::Plain(s) => s.clone(),
            TextSpan::Bold(children)
            | TextSpan::Italic(children)
            | TextSpan::Highlight(children)
            | TextSpan::Comment(children)
            | TextSpan::Span(_, children) => children.iter().map(TextSpan::plain_text).collect(),
        }
    }
}

/// Returns `true` if the input text contains any inline markup tags.
///
/// This performs a quick scan for `<` followed by a known tag name and `>`.
/// It is used to decide whether full markup parsing is needed.
#[must_use]
pub fn has_inline_markup(text: &str) -> bool {
    let mut remaining = text;
    while let Some(pos) = remaining.find('<') {
        let after = &remaining[pos + 1..];
        if tag_name_at_start(after).is_some() {
            return true;
        }
        // Check for <span with attributes
        if span_tag_at_start(after).is_some() {
            return true;
        }
        // Also check for closing tags: </tagname>
        if let Some(rest) = after.strip_prefix('/') {
            if tag_name_at_start(rest).is_some() {
                return true;
            }
            // Check for </span>
            if rest.len() >= 5 && rest[..5].eq_ignore_ascii_case("span>") {
                return true;
            }
        }
        remaining = &remaining[pos + 1..];
    }
    false
}

/// Parses inline markup tags from text and returns a list of [`TextSpan`]s.
///
/// When the text contains no markup, a single `TextSpan::Plain` is returned.
/// Unclosed tags wrap all remaining text (per ChordPro spec). Unrecognized
/// tags are treated as plain text.
///
/// # Examples
///
/// ```
/// use chordpro_core::inline_markup::{TextSpan, parse_inline_markup};
///
/// // No markup
/// let spans = parse_inline_markup("plain text");
/// assert_eq!(spans, vec![TextSpan::Plain("plain text".to_string())]);
///
/// // Bold markup
/// let spans = parse_inline_markup("<b>bold</b>");
/// assert_eq!(spans, vec![TextSpan::Bold(vec![TextSpan::Plain("bold".to_string())])]);
///
/// // Nested markup
/// let spans = parse_inline_markup("<b><i>both</i></b>");
/// assert_eq!(spans, vec![
///     TextSpan::Bold(vec![
///         TextSpan::Italic(vec![TextSpan::Plain("both".to_string())])
///     ])
/// ]);
/// ```
#[must_use]
pub fn parse_inline_markup(text: &str) -> Vec<TextSpan> {
    if !has_inline_markup(text) {
        if text.is_empty() {
            return Vec::new();
        }
        return vec![TextSpan::Plain(text.to_string())];
    }

    let mut parser = InlineMarkupParser::new(text);
    let spans = parser.parse_spans(&[]);

    // If parsing resulted in no spans but there was text, return plain text
    if spans.is_empty() && !text.is_empty() {
        return vec![TextSpan::Plain(text.to_string())];
    }

    normalize_spans(spans)
}

/// Extracts the plain text content from a list of spans, stripping all markup.
///
/// This is used to populate the backward-compatible `text` field in
/// `LyricsSegment` when markup is present.
#[must_use]
pub fn spans_to_plain_text(spans: &[TextSpan]) -> String {
    spans.iter().map(TextSpan::plain_text).collect()
}

// ---------------------------------------------------------------------------
// Tag types
// ---------------------------------------------------------------------------

/// The recognized inline markup tag types.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TagType {
    Bold,
    Italic,
    Highlight,
    Comment,
    Span(SpanAttributes),
}

/// Attempts to match a known tag name at the start of the string.
///
/// Returns the tag type and the length consumed (including the closing `>`).
/// The input should start after the `<` (or `</`).
///
/// Note: This does NOT match `<span ...>` tags — those are handled by
/// [`span_tag_at_start`] because they require attribute parsing.
fn tag_name_at_start(s: &str) -> Option<(TagType, usize)> {
    // Try each tag name (longest first to avoid prefix conflicts)
    let tags: &[(&str, TagType)] = &[
        ("highlight>", TagType::Highlight),
        ("comment>", TagType::Comment),
        ("italic>", TagType::Italic),
        ("bold>", TagType::Bold),
        ("b>", TagType::Bold),
        ("i>", TagType::Italic),
    ];

    for (name, tag_type) in tags {
        if s.len() >= name.len() {
            // Case-insensitive comparison
            let candidate = &s[..name.len()];
            if candidate.eq_ignore_ascii_case(name) {
                return Some((tag_type.clone(), name.len()));
            }
        }
    }

    None
}

/// Attempts to match `<span ...>` or `<span>` at the start of the string.
///
/// The input should start after the `<`. Returns `SpanAttributes` and the
/// length consumed (including the closing `>`).
fn span_tag_at_start(s: &str) -> Option<(SpanAttributes, usize)> {
    // Must start with "span" (case-insensitive)
    if s.len() < 4 {
        return None;
    }
    if !s[..4].eq_ignore_ascii_case("span") {
        return None;
    }

    let after_name = &s[4..];

    // <span> with no attributes
    if after_name.starts_with('>') {
        return Some((SpanAttributes::default(), 5)); // "span>"
    }

    // Must be followed by whitespace for attributes
    if !after_name.starts_with(|c: char| c.is_ascii_whitespace()) {
        return None;
    }

    // Find the closing '>'
    let closing = s.find('>')?;

    // Parse attributes from the region between "span " and ">"
    let attr_str = &s[4..closing].trim();
    let attrs = parse_span_attributes(attr_str);

    Some((attrs, closing + 1))
}

/// Parses key="value" or key='value' attribute pairs from a span tag.
fn parse_span_attributes(s: &str) -> SpanAttributes {
    let mut attrs = SpanAttributes::default();
    let mut remaining = s.trim();

    while !remaining.is_empty() {
        // Skip whitespace
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }

        // Find '='
        let eq_pos = match remaining.find('=') {
            Some(pos) => pos,
            None => break,
        };

        let key = remaining[..eq_pos].trim();
        let after_eq = remaining[eq_pos + 1..].trim_start();

        // Value must be quoted
        let (quote_char, after_quote) = if let Some(rest) = after_eq.strip_prefix('"') {
            ('"', rest)
        } else if let Some(rest) = after_eq.strip_prefix('\'') {
            ('\'', rest)
        } else {
            // No quote — skip to next whitespace or end
            break;
        };

        // Find closing quote
        let end_quote = match after_quote.find(quote_char) {
            Some(pos) => pos,
            None => break,
        };

        let value = &after_quote[..end_quote];

        // Set the attribute (case-insensitive key matching)
        let key_lower = key.to_ascii_lowercase();
        match key_lower.as_str() {
            "font_family" => attrs.font_family = Some(value.to_string()),
            "size" => attrs.size = Some(value.to_string()),
            "foreground" | "color" => attrs.foreground = Some(value.to_string()),
            "background" => attrs.background = Some(value.to_string()),
            "weight" => attrs.weight = Some(value.to_string()),
            "style" => attrs.style = Some(value.to_string()),
            _ => {} // Ignore unknown attributes
        }

        remaining = &after_quote[end_quote + 1..];
    }

    attrs
}

/// Attempts to match a closing tag at the start of the string.
///
/// The input should start after the `</`. Matches both simple tags and `</span>`.
fn closing_tag_at_start(s: &str) -> Option<(TagType, usize)> {
    // Check for </span> first
    if s.len() >= 5 && s[..5].eq_ignore_ascii_case("span>") {
        return Some((TagType::Span(SpanAttributes::default()), 5));
    }
    tag_name_at_start(s)
}

// ---------------------------------------------------------------------------
// Internal parser
// ---------------------------------------------------------------------------

/// Converts a `TagType` and its children into a `TextSpan`.
fn tag_type_to_span(tag_type: TagType, children: Vec<TextSpan>) -> TextSpan {
    match tag_type {
        TagType::Bold => TextSpan::Bold(children),
        TagType::Italic => TextSpan::Italic(children),
        TagType::Highlight => TextSpan::Highlight(children),
        TagType::Comment => TextSpan::Comment(children),
        TagType::Span(attrs) => TextSpan::Span(attrs, children),
    }
}

/// Checks whether a closing tag matches any of the expected closers.
///
/// For `Span` tags, only the tag type matters — attributes are not compared
/// because `</span>` has no attributes.
fn closers_contain(closers: &[TagType], tag: &TagType) -> bool {
    closers.iter().any(|c| match (c, tag) {
        (TagType::Span(_), TagType::Span(_)) => true,
        (a, b) => a == b,
    })
}

/// Maximum nesting depth for inline markup tags.
///
/// Tags nested beyond this limit are treated as plain text to prevent
/// stack overflow on adversarial input.
const MAX_NESTING_DEPTH: usize = 32;

/// Internal parser state for inline markup.
struct InlineMarkupParser<'a> {
    /// The input text being parsed.
    input: &'a str,
    /// Current byte position in the input.
    pos: usize,
}

impl<'a> InlineMarkupParser<'a> {
    /// Creates a new parser for the given input.
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    /// Returns the remaining unparsed input.
    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    /// Parses spans until end of input or a closing tag matching one of the
    /// `expected_closers` is found. Returns the parsed spans.
    ///
    /// `expected_closers` is the stack of tag types we're inside. When we find
    /// a closing tag that matches one of them, we stop and let the caller
    /// handle it.
    fn parse_spans(&mut self, expected_closers: &[TagType]) -> Vec<TextSpan> {
        let mut spans: Vec<TextSpan> = Vec::new();
        let mut plain_start = self.pos;

        while self.pos < self.input.len() {
            let remaining = self.remaining();

            if remaining.starts_with('<') {
                let after_lt = &self.input[self.pos + 1..];

                // Check for closing tag
                if after_lt.starts_with('/') {
                    let after_slash = &self.input[self.pos + 2..];
                    if let Some((tag_type, name_len)) = closing_tag_at_start(after_slash) {
                        // If this closing tag matches one of our expected closers,
                        // flush plain text and return
                        if closers_contain(expected_closers, &tag_type) {
                            // Flush accumulated plain text
                            if plain_start < self.pos {
                                spans.push(TextSpan::Plain(
                                    self.input[plain_start..self.pos].to_string(),
                                ));
                            }
                            // Consume the closing tag: </ + name_len
                            self.pos += 2 + name_len;
                            return spans;
                        }
                        // Not our closer — treat as plain text
                        self.pos += 1;
                        continue;
                    }
                    // Not a recognized closing tag
                    self.pos += 1;
                    continue;
                }

                // Enforce depth limit to prevent stack overflow on adversarial input
                if expected_closers.len() >= MAX_NESTING_DEPTH {
                    // Treat `<` as plain text — don't recurse deeper
                    self.pos += 1;
                    continue;
                }

                // Check for <span ...> opening tag (before simple tags, since
                // "span" is not in the simple-tag list)
                if let Some((attrs, tag_len)) = span_tag_at_start(after_lt) {
                    // Flush accumulated plain text before this tag
                    if plain_start < self.pos {
                        spans.push(TextSpan::Plain(
                            self.input[plain_start..self.pos].to_string(),
                        ));
                    }

                    // Consume the opening tag: < + tag_len
                    self.pos += 1 + tag_len;

                    let mut closers = expected_closers.to_vec();
                    closers.push(TagType::Span(attrs.clone()));
                    let children = self.parse_spans(&closers);
                    spans.push(TextSpan::Span(attrs, children));

                    plain_start = self.pos;
                    continue;
                }

                // Check for simple opening tag
                if let Some((tag_type, name_len)) = tag_name_at_start(after_lt) {
                    // Flush accumulated plain text before this tag
                    if plain_start < self.pos {
                        spans.push(TextSpan::Plain(
                            self.input[plain_start..self.pos].to_string(),
                        ));
                    }

                    // Consume the opening tag: < + name_len
                    self.pos += 1 + name_len;

                    // Parse children, expecting a closing tag for this type.
                    // Per ChordPro spec, unclosed tags apply to all remaining text,
                    // so we always wrap whatever children were collected.
                    let mut closers = expected_closers.to_vec();
                    closers.push(tag_type.clone());
                    let children = self.parse_spans(&closers);
                    let span = tag_type_to_span(tag_type, children);
                    spans.push(span);

                    plain_start = self.pos;
                    continue;
                }

                // Not a recognized tag — treat `<` as plain text
                self.pos += 1;
                continue;
            }

            let ch_len = remaining.chars().next().map_or(1, |c| c.len_utf8());
            self.pos += ch_len;
        }

        // Flush remaining plain text
        if plain_start < self.pos {
            spans.push(TextSpan::Plain(
                self.input[plain_start..self.pos].to_string(),
            ));
        }

        spans
    }
}

/// Merges adjacent `Plain` spans into a single `Plain` span.
fn normalize_spans(spans: Vec<TextSpan>) -> Vec<TextSpan> {
    let mut result: Vec<TextSpan> = Vec::new();

    for span in spans {
        match span {
            TextSpan::Plain(text) => {
                if let Some(TextSpan::Plain(prev)) = result.last_mut() {
                    prev.push_str(&text);
                } else {
                    result.push(TextSpan::Plain(text));
                }
            }
            TextSpan::Bold(children) => {
                result.push(TextSpan::Bold(normalize_spans(children)));
            }
            TextSpan::Italic(children) => {
                result.push(TextSpan::Italic(normalize_spans(children)));
            }
            TextSpan::Highlight(children) => {
                result.push(TextSpan::Highlight(normalize_spans(children)));
            }
            TextSpan::Comment(children) => {
                result.push(TextSpan::Comment(normalize_spans(children)));
            }
            TextSpan::Span(attrs, children) => {
                result.push(TextSpan::Span(attrs, normalize_spans(children)));
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- has_inline_markup --------------------------------------------------

    #[test]
    fn no_markup_plain_text() {
        assert!(!has_inline_markup("Hello world"));
    }

    #[test]
    fn no_markup_with_angle_bracket() {
        assert!(!has_inline_markup("x < y"));
    }

    #[test]
    fn has_bold_tag() {
        assert!(has_inline_markup("<b>bold</b>"));
    }

    #[test]
    fn has_italic_tag() {
        assert!(has_inline_markup("<i>italic</i>"));
    }

    #[test]
    fn has_long_bold_tag() {
        assert!(has_inline_markup("<bold>text</bold>"));
    }

    #[test]
    fn has_long_italic_tag() {
        assert!(has_inline_markup("<italic>text</italic>"));
    }

    #[test]
    fn has_highlight_tag() {
        assert!(has_inline_markup("<highlight>text</highlight>"));
    }

    #[test]
    fn has_comment_tag() {
        assert!(has_inline_markup("<comment>text</comment>"));
    }

    // -- parse_inline_markup: plain text ------------------------------------

    #[test]
    fn parse_plain_text() {
        let spans = parse_inline_markup("Hello world");
        assert_eq!(spans, vec![TextSpan::Plain("Hello world".to_string())]);
    }

    #[test]
    fn parse_empty_text() {
        let spans = parse_inline_markup("");
        assert_eq!(spans, Vec::<TextSpan>::new());
    }

    // -- parse_inline_markup: simple tags -----------------------------------

    #[test]
    fn parse_bold_short() {
        let spans = parse_inline_markup("<b>bold</b>");
        assert_eq!(
            spans,
            vec![TextSpan::Bold(vec![TextSpan::Plain("bold".to_string())])]
        );
    }

    #[test]
    fn parse_bold_long() {
        let spans = parse_inline_markup("<bold>bold</bold>");
        assert_eq!(
            spans,
            vec![TextSpan::Bold(vec![TextSpan::Plain("bold".to_string())])]
        );
    }

    #[test]
    fn parse_italic_short() {
        let spans = parse_inline_markup("<i>italic</i>");
        assert_eq!(
            spans,
            vec![TextSpan::Italic(vec![TextSpan::Plain(
                "italic".to_string()
            )])]
        );
    }

    #[test]
    fn parse_italic_long() {
        let spans = parse_inline_markup("<italic>italic</italic>");
        assert_eq!(
            spans,
            vec![TextSpan::Italic(vec![TextSpan::Plain(
                "italic".to_string()
            )])]
        );
    }

    #[test]
    fn parse_highlight() {
        let spans = parse_inline_markup("<highlight>highlighted</highlight>");
        assert_eq!(
            spans,
            vec![TextSpan::Highlight(vec![TextSpan::Plain(
                "highlighted".to_string()
            )])]
        );
    }

    #[test]
    fn parse_comment() {
        let spans = parse_inline_markup("<comment>commented</comment>");
        assert_eq!(
            spans,
            vec![TextSpan::Comment(vec![TextSpan::Plain(
                "commented".to_string()
            )])]
        );
    }

    // -- parse_inline_markup: mixed content ---------------------------------

    #[test]
    fn parse_text_before_and_after_tag() {
        let spans = parse_inline_markup("Hello <b>world</b> foo");
        assert_eq!(
            spans,
            vec![
                TextSpan::Plain("Hello ".to_string()),
                TextSpan::Bold(vec![TextSpan::Plain("world".to_string())]),
                TextSpan::Plain(" foo".to_string()),
            ]
        );
    }

    #[test]
    fn parse_multiple_tags() {
        let spans = parse_inline_markup("<b>bold</b> and <i>italic</i>");
        assert_eq!(
            spans,
            vec![
                TextSpan::Bold(vec![TextSpan::Plain("bold".to_string())]),
                TextSpan::Plain(" and ".to_string()),
                TextSpan::Italic(vec![TextSpan::Plain("italic".to_string())]),
            ]
        );
    }

    // -- parse_inline_markup: nested tags -----------------------------------

    #[test]
    fn parse_nested_bold_italic() {
        let spans = parse_inline_markup("<b><i>both</i></b>");
        assert_eq!(
            spans,
            vec![TextSpan::Bold(vec![TextSpan::Italic(vec![
                TextSpan::Plain("both".to_string())
            ])])]
        );
    }

    #[test]
    fn parse_nested_with_surrounding_text() {
        let spans = parse_inline_markup("<b>bold <i>and italic</i> text</b>");
        assert_eq!(
            spans,
            vec![TextSpan::Bold(vec![
                TextSpan::Plain("bold ".to_string()),
                TextSpan::Italic(vec![TextSpan::Plain("and italic".to_string())]),
                TextSpan::Plain(" text".to_string()),
            ])]
        );
    }

    // -- parse_inline_markup: case insensitive ------------------------------

    #[test]
    fn parse_case_insensitive_tags() {
        let spans = parse_inline_markup("<B>bold</B>");
        assert_eq!(
            spans,
            vec![TextSpan::Bold(vec![TextSpan::Plain("bold".to_string())])]
        );
    }

    #[test]
    fn parse_mixed_case_tags() {
        let spans = parse_inline_markup("<Bold>text</Bold>");
        assert_eq!(
            spans,
            vec![TextSpan::Bold(vec![TextSpan::Plain("text".to_string())])]
        );
    }

    // -- parse_inline_markup: graceful degradation --------------------------

    #[test]
    fn unclosed_tag_wraps_remaining_text() {
        // Per ChordPro spec, unclosed tags apply to all remaining text.
        let spans = parse_inline_markup("<b>unclosed");
        assert_eq!(
            spans,
            vec![TextSpan::Bold(vec![TextSpan::Plain(
                "unclosed".to_string()
            )])]
        );
    }

    #[test]
    fn depth_limit_prevents_stack_overflow() {
        // Deeply nested tags beyond MAX_NESTING_DEPTH are treated as plain text.
        let open_tags: String = "<b>".repeat(MAX_NESTING_DEPTH + 1);
        let close_tags: String = "</b>".repeat(MAX_NESTING_DEPTH + 1);
        let input = format!("{}text{}", open_tags, close_tags);
        // Must not panic/overflow; just verify it returns something reasonable.
        let spans = parse_inline_markup(&input);
        assert!(!spans.is_empty());
    }

    #[test]
    fn unrecognized_tag_treated_as_plain() {
        let spans = parse_inline_markup("<unknown>text</unknown>");
        assert_eq!(
            spans,
            vec![TextSpan::Plain("<unknown>text</unknown>".to_string())]
        );
    }

    #[test]
    fn lone_angle_bracket_is_plain() {
        let spans = parse_inline_markup("x < y");
        assert_eq!(spans, vec![TextSpan::Plain("x < y".to_string())]);
    }

    #[test]
    fn stray_closing_tag_is_plain() {
        let spans = parse_inline_markup("text </b> more");
        assert_eq!(spans, vec![TextSpan::Plain("text </b> more".to_string())]);
    }

    // -- has_inline_markup: span tags -----------------------------------------

    #[test]
    fn has_span_tag_no_attrs() {
        assert!(has_inline_markup("<span>text</span>"));
    }

    #[test]
    fn has_span_tag_with_attrs() {
        assert!(has_inline_markup(r#"<span foreground="red">text</span>"#));
    }

    #[test]
    fn has_span_closing_tag_only() {
        assert!(has_inline_markup("text </span> more"));
    }

    // -- parse_inline_markup: span tags ---------------------------------------

    #[test]
    fn parse_span_no_attrs() {
        let spans = parse_inline_markup("<span>styled</span>");
        assert_eq!(
            spans,
            vec![TextSpan::Span(
                SpanAttributes::default(),
                vec![TextSpan::Plain("styled".to_string())]
            )]
        );
    }

    #[test]
    fn parse_span_single_attr() {
        let spans = parse_inline_markup(r#"<span foreground="red">text</span>"#);
        assert_eq!(
            spans,
            vec![TextSpan::Span(
                SpanAttributes {
                    foreground: Some("red".to_string()),
                    ..Default::default()
                },
                vec![TextSpan::Plain("text".to_string())]
            )]
        );
    }

    #[test]
    fn parse_span_multiple_attrs() {
        let spans = parse_inline_markup(
            r#"<span font_family="Serif" size="12" foreground="blue" background="yellow" weight="bold" style="italic">text</span>"#,
        );
        assert_eq!(
            spans,
            vec![TextSpan::Span(
                SpanAttributes {
                    font_family: Some("Serif".to_string()),
                    size: Some("12".to_string()),
                    foreground: Some("blue".to_string()),
                    background: Some("yellow".to_string()),
                    weight: Some("bold".to_string()),
                    style: Some("italic".to_string()),
                },
                vec![TextSpan::Plain("text".to_string())]
            )]
        );
    }

    #[test]
    fn parse_span_single_quoted_attrs() {
        let spans = parse_inline_markup("<span foreground='green'>text</span>");
        assert_eq!(
            spans,
            vec![TextSpan::Span(
                SpanAttributes {
                    foreground: Some("green".to_string()),
                    ..Default::default()
                },
                vec![TextSpan::Plain("text".to_string())]
            )]
        );
    }

    #[test]
    fn parse_span_color_alias() {
        let spans = parse_inline_markup(r#"<span color="red">text</span>"#);
        assert_eq!(
            spans,
            vec![TextSpan::Span(
                SpanAttributes {
                    foreground: Some("red".to_string()),
                    ..Default::default()
                },
                vec![TextSpan::Plain("text".to_string())]
            )]
        );
    }

    #[test]
    fn parse_span_case_insensitive() {
        let spans = parse_inline_markup(r#"<SPAN Foreground="red">text</SPAN>"#);
        assert_eq!(
            spans,
            vec![TextSpan::Span(
                SpanAttributes {
                    foreground: Some("red".to_string()),
                    ..Default::default()
                },
                vec![TextSpan::Plain("text".to_string())]
            )]
        );
    }

    #[test]
    fn parse_span_nested_inside_bold() {
        let spans = parse_inline_markup(r#"<b><span foreground="red">text</span></b>"#);
        assert_eq!(
            spans,
            vec![TextSpan::Bold(vec![TextSpan::Span(
                SpanAttributes {
                    foreground: Some("red".to_string()),
                    ..Default::default()
                },
                vec![TextSpan::Plain("text".to_string())]
            )])]
        );
    }

    #[test]
    fn parse_bold_nested_inside_span() {
        let spans = parse_inline_markup(r#"<span foreground="red"><b>text</b></span>"#);
        assert_eq!(
            spans,
            vec![TextSpan::Span(
                SpanAttributes {
                    foreground: Some("red".to_string()),
                    ..Default::default()
                },
                vec![TextSpan::Bold(vec![TextSpan::Plain("text".to_string())])]
            )]
        );
    }

    #[test]
    fn parse_span_with_surrounding_text() {
        let spans = parse_inline_markup(r#"Hello <span foreground="red">world</span> foo"#);
        assert_eq!(
            spans,
            vec![
                TextSpan::Plain("Hello ".to_string()),
                TextSpan::Span(
                    SpanAttributes {
                        foreground: Some("red".to_string()),
                        ..Default::default()
                    },
                    vec![TextSpan::Plain("world".to_string())]
                ),
                TextSpan::Plain(" foo".to_string()),
            ]
        );
    }

    #[test]
    fn parse_span_unclosed_wraps_remaining() {
        let spans = parse_inline_markup(r#"<span foreground="red">unclosed"#);
        assert_eq!(
            spans,
            vec![TextSpan::Span(
                SpanAttributes {
                    foreground: Some("red".to_string()),
                    ..Default::default()
                },
                vec![TextSpan::Plain("unclosed".to_string())]
            )]
        );
    }

    #[test]
    fn parse_span_unknown_attrs_ignored() {
        let spans = parse_inline_markup(r#"<span unknown="val" foreground="red">text</span>"#);
        assert_eq!(
            spans,
            vec![TextSpan::Span(
                SpanAttributes {
                    foreground: Some("red".to_string()),
                    ..Default::default()
                },
                vec![TextSpan::Plain("text".to_string())]
            )]
        );
    }

    // -- spans_to_plain_text ------------------------------------------------

    #[test]
    fn plain_text_extraction_simple() {
        let spans = vec![TextSpan::Plain("hello".to_string())];
        assert_eq!(spans_to_plain_text(&spans), "hello");
    }

    #[test]
    fn plain_text_extraction_with_markup() {
        let spans = vec![
            TextSpan::Plain("Hello ".to_string()),
            TextSpan::Bold(vec![TextSpan::Plain("world".to_string())]),
        ];
        assert_eq!(spans_to_plain_text(&spans), "Hello world");
    }

    #[test]
    fn plain_text_extraction_nested() {
        let spans = vec![TextSpan::Bold(vec![TextSpan::Italic(vec![
            TextSpan::Plain("nested".to_string()),
        ])])];
        assert_eq!(spans_to_plain_text(&spans), "nested");
    }

    #[test]
    fn plain_text_extraction_span() {
        let spans = vec![TextSpan::Span(
            SpanAttributes {
                foreground: Some("red".to_string()),
                ..Default::default()
            },
            vec![TextSpan::Plain("colored".to_string())],
        )];
        assert_eq!(spans_to_plain_text(&spans), "colored");
    }
}
