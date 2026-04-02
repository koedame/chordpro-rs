//! Abstract Syntax Tree (AST) definitions for the ChordPro format.
//!
//! This module defines the data structures that represent a parsed ChordPro
//! document. The root node is [`Song`], which contains metadata and a sequence
//! of [`Line`] nodes.
//!
//! # Design
//!
//! The AST is intentionally simple and flat. A [`Song`] is a list of lines,
//! where each line is one of several variants (lyrics, directive, comment, or
//! empty). Section boundaries are represented as directives — the parser may
//! later group them, but the AST itself does not enforce nesting.
//!
//! Chord annotations are stored inline within lyrics lines using a segment
//! model: each [`LyricsSegment`] pairs an optional chord with the lyric text
//! that follows it. This preserves the chord-text relationship without
//! requiring offset arithmetic during rendering.
//!
//! # Directive Classification
//!
//! Directives carry a [`DirectiveKind`] that classifies them into metadata,
//! formatting, environment (section), or unknown categories. The parser
//! resolves short aliases (e.g., `t` → `title`) and performs case-insensitive
//! matching per the ChordPro specification.

use crate::inline_markup::TextSpan;

// ---------------------------------------------------------------------------
// Song (root node)
// ---------------------------------------------------------------------------

/// The root AST node representing a complete ChordPro song.
///
/// A song consists of optional metadata (populated from directives such as
/// `{title}`, `{subtitle}`, `{artist}`, etc.) and a sequence of lines that
/// make up the song body.
///
/// # Examples
///
/// ```
/// use chordpro_core::ast::{Song, Metadata};
///
/// let song = Song::new();
/// assert!(song.lines.is_empty());
/// assert_eq!(song.metadata.title, None);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    /// Metadata extracted from directives (title, subtitle, artist, etc.).
    pub metadata: Metadata,
    /// The ordered sequence of lines that make up the song body.
    pub lines: Vec<Line>,
}

impl Song {
    /// Creates a new empty song with no metadata and no lines.
    #[must_use]
    pub fn new() -> Self {
        Self {
            metadata: Metadata::new(),
            lines: Vec::new(),
        }
    }

    /// Extracts `{+config.KEY: VALUE}` overrides from this song's directives.
    ///
    /// Returns a list of `(key, value)` pairs in directive order. The key is
    /// the dot-separated config path (e.g., `"pdf.chorus.indent"`), and the
    /// value is the raw string from the directive.
    ///
    /// These overrides are scoped to this song and should not leak to other
    /// songs in a multi-song file.
    #[must_use]
    pub fn config_overrides(&self) -> Vec<(&str, &str)> {
        let mut overrides = Vec::new();
        for line in &self.lines {
            if let Line::Directive(directive) = line {
                if let DirectiveKind::ConfigOverride(ref key) = directive.kind {
                    if let Some(ref value) = directive.value {
                        overrides.push((key.as_str(), value.as_str()));
                    }
                }
            }
        }
        overrides
    }

    /// Resolves `{define}` display and format attributes to matching chords.
    ///
    /// Scans all `{define}` directives in the song, collecting `display` and
    /// `format` overrides. Then walks every chord in the song and sets
    /// `Chord::display`:
    /// - `display` takes precedence (used as-is).
    /// - `format` is expanded using the chord's parsed components.
    ///
    /// Later definitions override earlier ones for the same chord name.
    pub fn apply_define_displays(&mut self) {
        // First pass: collect chord name -> (display, format) mappings.
        let mut define_map: Vec<(String, Option<String>, Option<String>)> = Vec::new();
        for line in &self.lines {
            if let Line::Directive(directive) = line {
                if directive.kind == DirectiveKind::Define {
                    if let Some(ref value) = directive.value {
                        let def = ChordDefinition::parse_value(value);
                        if def.display.is_some() || def.format.is_some() {
                            if let Some(entry) =
                                define_map.iter_mut().find(|(n, _, _)| *n == def.name)
                            {
                                entry.1 = def.display;
                                entry.2 = def.format;
                            } else {
                                define_map.push((def.name, def.display, def.format));
                            }
                        }
                    }
                }
            }
        }

        if define_map.is_empty() {
            return;
        }

        // Second pass: apply display/format overrides to matching chords.
        for line in &mut self.lines {
            if let Line::Lyrics(lyrics_line) = line {
                for segment in &mut lyrics_line.segments {
                    if let Some(ref mut chord) = segment.chord {
                        if chord.display.is_none() {
                            if let Some((_, display, format)) =
                                define_map.iter().find(|(n, _, _)| *n == chord.name)
                            {
                                if let Some(d) = display {
                                    // display= takes precedence over format=
                                    chord.display = Some(d.clone());
                                } else if let Some(f) = format {
                                    // Expand format pattern using chord components
                                    if let Some(expanded) = chord.expand_format(f) {
                                        chord.display = Some(expanded);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Default for Song {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

/// Metadata extracted from well-known ChordPro directives.
///
/// These fields correspond to standard ChordPro meta-directives such as
/// `{title}`, `{subtitle}`, `{artist}`, `{composer}`, `{album}`, `{year}`,
/// `{key}`, `{tempo}`, `{time}`, and `{capo}`.
///
/// Fields that can logically appear multiple times (e.g., `subtitle`, `artist`,
/// `composer`) are stored as `Vec<String>`. Fields that are expected to appear
/// at most once are stored as `Option<String>`.
///
/// Any meta-directive not covered by these fields can be stored in the
/// `custom` vector as key-value pairs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Metadata {
    /// The song title, from `{title}` / `{t}`.
    pub title: Option<String>,
    /// Subtitles, from `{subtitle}` / `{st}`. May appear multiple times.
    pub subtitles: Vec<String>,
    /// Artist names, from `{artist}`.
    pub artists: Vec<String>,
    /// Composer names, from `{composer}`.
    pub composers: Vec<String>,
    /// Lyricist names, from `{lyricist}`.
    pub lyricists: Vec<String>,
    /// Album name, from `{album}`.
    pub album: Option<String>,
    /// Year or date, from `{year}`.
    pub year: Option<String>,
    /// Musical key, from `{key}`.
    pub key: Option<String>,
    /// Tempo indication, from `{tempo}`.
    pub tempo: Option<String>,
    /// Time signature, from `{time}`.
    pub time: Option<String>,
    /// Capo position, from `{capo}`.
    pub capo: Option<String>,
    /// Sortable title, from `{sorttitle}`.
    pub sort_title: Option<String>,
    /// Sortable artist name, from `{sortartist}`.
    pub sort_artist: Option<String>,
    /// Arranger names, from `{arranger}`. May appear multiple times.
    pub arrangers: Vec<String>,
    /// Copyright notice, from `{copyright}`.
    pub copyright: Option<String>,
    /// Song duration, from `{duration}`.
    pub duration: Option<String>,
    /// Tags for categorization, from `{tag}`. May appear multiple times.
    pub tags: Vec<String>,
    /// Custom metadata directives not covered by the standard fields.
    /// Each entry is a `(name, value)` pair.
    pub custom: Vec<(String, String)>,
}

impl Metadata {
    /// Creates a new empty metadata set with all fields at their defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Line
// ---------------------------------------------------------------------------

/// A single line in the song body.
///
/// ChordPro documents are processed line-by-line. Each line is classified
/// into one of these variants by the parser.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// A lyrics line, possibly containing chord annotations interspersed
    /// with text. An empty lyrics line (no text, no chords) is represented
    /// by [`Line::Empty`] instead.
    Lyrics(LyricsLine),

    /// A directive such as `{title: My Song}` or `{start_of_chorus}`.
    Directive(Directive),

    /// A comment line from a comment directive (`{comment}`, `{comment_italic}`,
    /// `{comment_box}`) or a file-level `#` comment. The [`CommentStyle`]
    /// distinguishes the rendering intent.
    Comment(CommentStyle, String),

    /// An empty line, typically used to separate paragraphs or sections.
    Empty,
}

// ---------------------------------------------------------------------------
// CommentStyle
// ---------------------------------------------------------------------------

/// The visual style of a comment, determined by the directive that produced it.
///
/// ChordPro supports three comment directives, each with a different rendering
/// intent:
///
/// - `{comment}` / `{c}` — normal comment (typically highlighted or boxed)
/// - `{comment_italic}` / `{ci}` — italic comment
/// - `{comment_box}` / `{cb}` — boxed comment
///
/// File-level `#` comments use [`CommentStyle::Normal`] as a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommentStyle {
    /// A normal comment, from `{comment}` / `{c}` or file-level `#`.
    Normal,
    /// An italic comment, from `{comment_italic}` / `{ci}`.
    Italic,
    /// A boxed comment, from `{comment_box}` / `{cb}`.
    Boxed,
}

// ---------------------------------------------------------------------------
// LyricsLine
// ---------------------------------------------------------------------------

/// A lyrics line containing text with optional chord annotations.
///
/// In ChordPro format, chords are written inline with the lyrics:
/// ```text
/// [Am]Hello [G]world
/// ```
///
/// The AST splits this into a sequence of [`LyricsSegment`] items, each
/// optionally preceded by a chord. This preserves the exact relationship
/// between chords and the lyric text they annotate.
///
/// # Examples
///
/// The input `[Am]Hello [G]world` produces:
///
/// ```
/// use chordpro_core::ast::{LyricsLine, LyricsSegment, Chord};
///
/// let line = LyricsLine {
///     segments: vec![
///         LyricsSegment {
///             chord: Some(Chord::new("Am")),
///             text: "Hello ".to_string(),
///             spans: vec![],
///         },
///         LyricsSegment {
///             chord: Some(Chord::new("G")),
///             text: "world".to_string(),
///             spans: vec![],
///         },
///     ],
/// };
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct LyricsLine {
    /// The ordered sequence of segments that make up this lyrics line.
    pub segments: Vec<LyricsSegment>,
}

impl LyricsLine {
    /// Creates a new empty lyrics line with no segments.
    #[must_use]
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Returns the full lyric text with all chord annotations removed.
    #[must_use]
    pub fn text(&self) -> String {
        self.segments.iter().map(|s| s.text.as_str()).collect()
    }

    /// Returns `true` if this lyrics line contains at least one chord.
    #[must_use]
    pub fn has_chords(&self) -> bool {
        self.segments.iter().any(|s| s.chord.is_some())
    }
}

impl Default for LyricsLine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// LyricsSegment
// ---------------------------------------------------------------------------

/// A segment within a lyrics line: an optional chord followed by text.
///
/// Every lyrics line is decomposed into a sequence of segments. Each segment
/// may have a chord placed above the start of its text. A segment with no
/// chord and non-empty text represents plain lyrics. A segment with a chord
/// and empty text represents a chord placed at the end of the line (or
/// between two consecutive chords with no intervening text).
#[derive(Debug, Clone, PartialEq)]
pub struct LyricsSegment {
    /// The chord annotation, if any, placed above the start of `text`.
    pub chord: Option<Chord>,
    /// The lyric text following the chord (may be empty).
    ///
    /// When inline markup is present, this field contains the plain text
    /// content with all markup tags stripped. Renderers that do not support
    /// markup can always use this field directly.
    pub text: String,
    /// Inline markup spans parsed from the text.
    ///
    /// When the text contains inline markup tags (e.g., `<b>`, `<i>`,
    /// `<highlight>`, `<comment>`), this field holds the parsed span tree.
    /// When no markup is present, this vector is empty and renderers should
    /// use the `text` field instead.
    pub spans: Vec<TextSpan>,
}

impl LyricsSegment {
    /// Creates a new segment with the given chord and text.
    #[must_use]
    pub fn new(chord: Option<Chord>, text: impl Into<String>) -> Self {
        Self {
            chord,
            text: text.into(),
            spans: Vec::new(),
        }
    }

    /// Creates a text-only segment with no chord.
    #[must_use]
    pub fn text_only(text: impl Into<String>) -> Self {
        Self {
            chord: None,
            text: text.into(),
            spans: Vec::new(),
        }
    }

    /// Creates a chord-only segment with no text.
    #[must_use]
    pub fn chord_only(chord: Chord) -> Self {
        Self {
            chord: Some(chord),
            text: String::new(),
            spans: Vec::new(),
        }
    }

    /// Creates a new segment with chord, text, and inline markup spans.
    #[must_use]
    pub fn with_spans(chord: Option<Chord>, text: impl Into<String>, spans: Vec<TextSpan>) -> Self {
        Self {
            chord,
            text: text.into(),
            spans,
        }
    }

    /// Returns `true` if this segment has inline markup spans.
    #[must_use]
    pub fn has_markup(&self) -> bool {
        !self.spans.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Chord
// ---------------------------------------------------------------------------

/// A chord annotation such as `Am`, `G7`, `Cmaj7`, or `F#m`.
///
/// The chord stores both the raw string as it appeared in the source and,
/// when parsing succeeds, a structured [`ChordDetail`] with the individual
/// components (root, accidental, quality, extension, bass note).
///
/// If the chord notation cannot be parsed structurally, `detail` is `None`
/// and the raw `name` is still available for display or round-tripping.
///
/// [`ChordDetail`]: crate::chord::ChordDetail
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chord {
    /// The raw chord string as written in the source (e.g., `"Am"`, `"G7"`).
    pub name: String,
    /// The parsed chord components, if the chord notation was recognized.
    pub detail: Option<crate::chord::ChordDetail>,
    /// An alternative display name set by `{define}` with `display` attribute.
    ///
    /// When present, renderers should show this instead of `name`.
    pub display: Option<String>,
}

impl Chord {
    /// Creates a new chord from the given name string.
    ///
    /// The chord notation is automatically parsed into structured components.
    /// If parsing fails (e.g., the chord string is not valid notation), the
    /// `detail` field is `None` but the raw `name` is preserved.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        let detail = crate::chord::parse_chord(&name);
        Self {
            name,
            detail,
            display: None,
        }
    }

    /// Returns the display name for this chord.
    ///
    /// If a `display` attribute was set (via `{define}`), returns that.
    /// Otherwise returns the raw `name`.
    #[must_use]
    pub fn display_name(&self) -> &str {
        self.display.as_deref().unwrap_or(&self.name)
    }

    /// Expand a format pattern using this chord's parsed components.
    ///
    /// Replaces placeholders in the pattern string with chord detail fields:
    /// - `%{root}` — root note with accidental (e.g., `"A"`, `"Bb"`, `"F#"`)
    /// - `%{quality}` — quality string (e.g., `""`, `"m"`, `"dim"`, `"aug"`)
    /// - `%{ext}` — extension (e.g., `"7"`, `"maj7"`, `"sus4"`)
    /// - `%{bass}` — bass note for slash chords (e.g., `"B"`, `"Eb"`)
    ///
    /// Returns `None` if the chord has no parsed `detail`.
    #[must_use]
    pub fn expand_format(&self, pattern: &str) -> Option<String> {
        let detail = self.detail.as_ref()?;

        let root = {
            let mut s = detail.root.to_string();
            if let Some(ref acc) = detail.root_accidental {
                s.push_str(&acc.to_string());
            }
            s
        };
        let quality = detail.quality.to_string();
        let ext = detail.extension.as_deref().unwrap_or("");
        let bass = detail
            .bass_note
            .as_ref()
            .map_or(String::new(), |(note, acc)| {
                let mut s = note.to_string();
                if let Some(a) = acc {
                    s.push_str(&a.to_string());
                }
                s
            });

        let result = pattern
            .replace("%{root}", &root)
            .replace("%{quality}", &quality)
            .replace("%{ext}", ext)
            .replace("%{bass}", &bass);

        Some(result)
    }
}

impl core::fmt::Display for Chord {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.display_name())
    }
}

// ---------------------------------------------------------------------------
// ImageAttributes
// ---------------------------------------------------------------------------

/// Attributes for the `{image}` directive.
///
/// The `{image}` directive embeds an image in the song. The `src` attribute
/// is required; all other attributes are optional.
///
/// Format support is renderer-specific: the PDF renderer supports JPEG only
/// (`.jpg` / `.jpeg`), while the HTML renderer delegates to the browser and
/// can display any web-supported format.
///
/// # Examples
///
/// ```
/// use chordpro_core::ast::ImageAttributes;
///
/// let attrs = ImageAttributes::new("photo.jpg");
/// assert_eq!(attrs.src, "photo.jpg");
/// assert!(attrs.width.is_none());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ImageAttributes {
    /// The image file path (required).
    pub src: String,
    /// Display width (e.g., "100", "50%").
    pub width: Option<String>,
    /// Display height (e.g., "200", "75%").
    pub height: Option<String>,
    /// Scale factor (e.g., "0.5").
    pub scale: Option<String>,
    /// Image title or alt text.
    pub title: Option<String>,
    /// Positioning anchor.
    pub anchor: Option<String>,
}

impl ImageAttributes {
    /// Creates a new `ImageAttributes` with the given source path and all
    /// optional fields set to `None`.
    #[must_use]
    pub fn new(src: impl Into<String>) -> Self {
        Self {
            src: src.into(),
            ..Self::default()
        }
    }

    /// Returns `true` if a non-empty `src` path is present.
    ///
    /// Renderers can use this to skip rendering when no image source was
    /// provided, avoiding duplicated empty-string checks.
    #[must_use]
    pub fn has_src(&self) -> bool {
        !self.src.is_empty()
    }
}

// ---------------------------------------------------------------------------
// ChordDefinition
// ---------------------------------------------------------------------------

/// Extract a `key=value` attribute from a string, removing it in-place.
///
/// Matches `key=` at word boundaries (preceded by whitespace or at start of
/// string). Handles both quoted values (`key="value with spaces"`) and
/// unquoted values (`key=value`).
///
/// Returns `Some(value)` if found (and mutates `s` to remove the attribute),
/// or `None` if not found.
fn extract_attribute(s: &mut String, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let match_pos = s
        .match_indices(&needle)
        .find(|&(pos, _)| pos == 0 || s.as_bytes()[pos - 1].is_ascii_whitespace());

    let (pos, _) = match_pos?;
    let after = &s[pos + needle.len()..];

    let (val, token_end) = if let Some(stripped) = after.strip_prefix('"') {
        // Find the closing quote, if present.
        let close = stripped.find('"').unwrap_or(stripped.len());
        let v = stripped[..close].to_string();
        // Only count the closing quote in token_end when it actually exists.
        let has_close = close < stripped.len();
        (
            Some(v),
            pos + needle.len() + 1 + close + usize::from(has_close),
        )
    } else {
        let v = after.split_whitespace().next().map(|t| t.to_string());
        let len = v.as_ref().map_or(0, |t| t.len());
        (v, pos + needle.len() + len)
    };

    // Remove the attribute token from the string.
    let before = s[..pos].trim_end();
    let after_token = s[token_end..].trim_start();
    *s = if before.is_empty() {
        after_token.to_string()
    } else if after_token.is_empty() {
        before.to_string()
    } else {
        format!("{before} {after_token}")
    };

    val
}

/// A parsed chord definition from `{define}` directives.
///
/// Supports three types:
/// - **Fretted**: `{define: Am base-fret 1 frets x 0 2 2 1 0}` (stored as raw value)
/// - **Keyboard**: `{define: Am keys 0 3 7}` (MIDI key offsets)
/// - **Copy**: `{define: Am copy Amin}` or `{define: Am copyall Amin}`
///
/// # Examples
///
/// ```
/// use chordpro_core::ast::ChordDefinition;
///
/// let def = ChordDefinition::parse_value("Am keys 0 3 7");
/// assert_eq!(def.name, "Am");
/// assert!(def.keys.is_some());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChordDefinition {
    /// The chord name being defined.
    pub name: String,
    /// Keyboard keys (MIDI note numbers, 0-127) for keyboard instrument definitions.
    ///
    /// Values outside 0-127 and non-numeric tokens are silently dropped during parsing.
    /// `None` when no valid key values are provided.
    pub keys: Option<Vec<i32>>,
    /// Source chord name for `copy` definitions.
    pub copy: Option<String>,
    /// Source chord name for `copyall` definitions.
    pub copyall: Option<String>,
    /// Display name override.
    pub display: Option<String>,
    /// Format pattern for chord name rendering (e.g., `"%{root}%{quality}"`).
    ///
    /// When present, the pattern is expanded using the chord's parsed
    /// components. Supported placeholders: `%{root}`, `%{quality}`,
    /// `%{ext}`, `%{bass}`.
    pub format: Option<String>,
    /// The raw definition value (for fretted definitions not yet parsed).
    pub raw: Option<String>,
}

impl ChordDefinition {
    /// Parse a define directive value string.
    ///
    /// Recognizes `keys`, `copy`, `copyall`, and `display` tokens.
    /// Everything else is stored in `raw` for fretted chord definitions.
    #[must_use]
    pub fn parse_value(value: &str) -> Self {
        let value = value.trim();
        let mut parts = value.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or("").to_string();
        let rest = parts.next().unwrap_or("").trim();

        let mut def = Self {
            name,
            keys: None,
            copy: None,
            copyall: None,
            display: None,
            format: None,
            raw: None,
        };

        if rest.is_empty() {
            return def;
        }

        // Extract known attributes (display=, format=) from the value first,
        // so they work with all definition variants (fretted, keys, copy, copyall).
        let mut remaining = rest.to_string();
        def.display = extract_attribute(&mut remaining, "display");
        def.format = extract_attribute(&mut remaining, "format");
        let remaining = remaining.trim();

        if remaining.is_empty() {
            return def;
        }

        // Check for "keys <n1> <n2> ..."
        // Key values are MIDI note numbers (0-127). Non-numeric and
        // out-of-range values are silently dropped.
        if let Some(keys_str) = remaining
            .strip_prefix("keys ")
            .or_else(|| remaining.strip_prefix("keys\t"))
        {
            let keys: Vec<i32> = keys_str
                .split_whitespace()
                .filter_map(|s| s.parse::<i32>().ok())
                .filter(|&v| (0..=127).contains(&v))
                .collect();
            // Treat empty keys (all values invalid) as no keys defined.
            def.keys = if keys.is_empty() { None } else { Some(keys) };
            return def;
        }
        if remaining == "keys" {
            // {define: Am keys} with no values — no keys defined.
            return def;
        }

        // Check for "copy <source>" or "copyall <source>"
        // Only the first token after the prefix is used as the source name.
        if let Some(source) = remaining.strip_prefix("copyall ") {
            let name = source.split_whitespace().next().unwrap_or("").trim();
            if !name.is_empty() {
                def.copyall = Some(name.to_string());
            }
            return def;
        }
        if let Some(source) = remaining.strip_prefix("copy ") {
            let name = source.split_whitespace().next().unwrap_or("").trim();
            if !name.is_empty() {
                def.copy = Some(name.to_string());
            }
            return def;
        }

        def.raw = if remaining.is_empty() {
            None
        } else {
            Some(remaining.to_string())
        };

        def
    }
}

// ---------------------------------------------------------------------------
// DirectiveKind
// ---------------------------------------------------------------------------

/// Classification of a ChordPro directive.
///
/// The parser examines each directive's name (case-insensitively, resolving
/// short aliases) and assigns a `DirectiveKind` to indicate its semantic
/// category. Renderers and downstream consumers can match on this enum
/// rather than performing string comparisons.
///
/// # Categories
///
/// - **Metadata** — directives that set song metadata (`title`, `subtitle`,
///   `artist`, `album`, `year`, `key`, `tempo`, `time`, `capo`, etc.).
/// - **Formatting** — comment directives (`comment`, `comment_italic`,
///   `comment_box`).
/// - **Font/size/color** — legacy rendering directives (`titlefont`,
///   `titlesize`, `titlecolour`, `chorusfont`, etc.).
/// - **Environment** — section start/end directives (`start_of_chorus`,
///   `end_of_chorus`, `start_of_verse`, etc.).
/// - **Unknown** — any directive not recognized as a standard directive.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DirectiveKind {
    // -- Metadata directives ------------------------------------------------
    /// `{title}` / `{t}` — the song title.
    Title,
    /// `{subtitle}` / `{st}` — a subtitle.
    Subtitle,
    /// `{artist}` — the artist name.
    Artist,
    /// `{composer}` — the composer name.
    Composer,
    /// `{lyricist}` — the lyricist name.
    Lyricist,
    /// `{album}` — the album name.
    Album,
    /// `{year}` — the year or date.
    Year,
    /// `{key}` — the musical key.
    Key,
    /// `{tempo}` — the tempo.
    Tempo,
    /// `{time}` — the time signature.
    Time,
    /// `{capo}` — the capo position.
    Capo,
    /// `{sorttitle}` — a sortable title.
    SortTitle,
    /// `{sortartist}` — a sortable artist name.
    SortArtist,
    /// `{arranger}` — the arranger name.
    Arranger,
    /// `{copyright}` — the copyright notice.
    Copyright,
    /// `{duration}` — the song duration.
    Duration,
    /// `{tag}` — a tag for categorization.
    Tag,

    // -- Transpose directive ------------------------------------------------
    /// `{transpose}` — in-file transposition offset in semitones.
    Transpose,

    // -- Formatting directives (comment) ------------------------------------
    /// `{comment}` / `{c}` — a normal comment.
    Comment,
    /// `{comment_italic}` / `{ci}` — an italic comment.
    CommentItalic,
    /// `{comment_box}` / `{cb}` — a boxed comment.
    CommentBox,

    // -- Environment (section) directives -----------------------------------
    /// `{start_of_chorus}` / `{soc}` — begins a chorus section.
    StartOfChorus,
    /// `{end_of_chorus}` / `{eoc}` — ends a chorus section.
    EndOfChorus,
    /// `{start_of_verse}` / `{sov}` — begins a verse section.
    StartOfVerse,
    /// `{end_of_verse}` / `{eov}` — ends a verse section.
    EndOfVerse,
    /// `{start_of_bridge}` / `{sob}` — begins a bridge section.
    StartOfBridge,
    /// `{end_of_bridge}` / `{eob}` — ends a bridge section.
    EndOfBridge,
    /// `{start_of_tab}` / `{sot}` — begins a tab section.
    StartOfTab,
    /// `{end_of_tab}` / `{eot}` — ends a tab section.
    EndOfTab,
    /// `{start_of_grid}` / `{sog}` — begins a grid section.
    StartOfGrid,
    /// `{end_of_grid}` / `{eog}` — ends a grid section.
    EndOfGrid,

    // -- Font, size, and color directives -----------------------------------
    /// `{textfont}` / `{tf}` — sets the font for lyrics text.
    TextFont,
    /// `{textsize}` / `{ts}` — sets the font size for lyrics text.
    TextSize,
    /// `{textcolour}` / `{textcolor}` / `{tc}` — sets the color for lyrics text.
    TextColour,
    /// `{chordfont}` / `{cf}` — sets the font for chord names.
    ChordFont,
    /// `{chordsize}` / `{cs}` — sets the font size for chord names.
    ChordSize,
    /// `{chordcolour}` / `{chordcolor}` / `{cc}` — sets the color for chord names.
    ChordColour,
    /// `{tabfont}` — sets the font for tab sections.
    TabFont,
    /// `{tabsize}` — sets the font size for tab sections.
    TabSize,
    /// `{tabcolour}` / `{tabcolor}` — sets the color for tab sections.
    TabColour,

    // -- Recall directives ---------------------------------------------------
    /// `{chorus}` — recalls (repeats) the most recently defined chorus section.
    ///
    /// An optional label may override the default "Chorus" heading.
    Chorus,

    // -- Page control directives ----------------------------------------------
    /// `{new_page}` / `{np}` — forces a page break.
    NewPage,
    /// `{new_physical_page}` / `{npp}` — forces a physical page break (for duplex printing).
    NewPhysicalPage,
    /// `{column_break}` / `{colb}` — forces a column break.
    ColumnBreak,
    /// `{columns}` / `{col}` — sets the number of columns.
    Columns,

    // -- Extended font, size, and color directives --------------------------
    /// `{titlefont}` — sets the font for song titles.
    TitleFont,
    /// `{titlesize}` — sets the font size for song titles.
    TitleSize,
    /// `{titlecolour}` / `{titlecolor}` — sets the color for song titles.
    TitleColour,
    /// `{chorusfont}` — sets the font for chorus sections.
    ChorusFont,
    /// `{chorussize}` — sets the font size for chorus sections.
    ChorusSize,
    /// `{choruscolour}` / `{choruscolor}` — sets the color for chorus sections.
    ChorusColour,
    /// `{footerfont}` — sets the font for footer text.
    FooterFont,
    /// `{footersize}` — sets the font size for footer text.
    FooterSize,
    /// `{footercolour}` / `{footercolor}` — sets the color for footer text.
    FooterColour,
    /// `{headerfont}` — sets the font for header text.
    HeaderFont,
    /// `{headersize}` — sets the font size for header text.
    HeaderSize,
    /// `{headercolour}` / `{headercolor}` — sets the color for header text.
    HeaderColour,
    /// `{labelfont}` — sets the font for labels.
    LabelFont,
    /// `{labelsize}` — sets the font size for labels.
    LabelSize,
    /// `{labelcolour}` / `{labelcolor}` — sets the color for labels.
    LabelColour,
    /// `{gridfont}` — sets the font for grid sections.
    GridFont,
    /// `{gridsize}` — sets the font size for grid sections.
    GridSize,
    /// `{gridcolour}` / `{gridcolor}` — sets the color for grid sections.
    GridColour,
    /// `{tocfont}` — sets the font for table of contents.
    TocFont,
    /// `{tocsize}` — sets the font size for table of contents.
    TocSize,
    /// `{toccolour}` / `{toccolor}` — sets the color for table of contents.
    TocColour,

    // -- Song boundary directives --------------------------------------------
    /// `{new_song}` / `{ns}` — marks the start of a new song in a multi-song file.
    NewSong,

    // -- Chord definition directives ----------------------------------------
    /// `{define}` — defines a custom chord fingering.
    Define,
    /// `{chord}` — references a custom chord.
    ChordDirective,

    // -- Delegate environment directives -------------------------------------
    /// `{start_of_abc}` — begins an ABC music notation section.
    /// Content is treated as verbatim text (no chord parsing).
    StartOfAbc,
    /// `{end_of_abc}` — ends an ABC music notation section.
    EndOfAbc,
    /// `{start_of_ly}` — begins a Lilypond notation section.
    /// Content is treated as verbatim text (no chord parsing).
    StartOfLy,
    /// `{end_of_ly}` — ends a Lilypond notation section.
    EndOfLy,
    /// `{start_of_svg}` — begins an SVG graphics section.
    /// Content is treated as verbatim text (no chord parsing).
    StartOfSvg,
    /// `{end_of_svg}` — ends an SVG graphics section.
    EndOfSvg,
    /// `{start_of_textblock}` — begins a preformatted text block section.
    /// Content is treated as verbatim text (no chord parsing).
    StartOfTextblock,
    /// `{end_of_textblock}` — ends a preformatted text block section.
    EndOfTextblock,

    // -- Custom section directives -------------------------------------------
    /// `{start_of_X}` — begins a custom section (e.g., intro, outro, solo).
    /// The contained `String` is the section type name (e.g., `"intro"`).
    StartOfSection(String),
    /// `{end_of_X}` — ends a custom section.
    /// The contained `String` is the section type name (e.g., `"intro"`).
    EndOfSection(String),

    // -- Generic metadata directive -----------------------------------------
    /// `{meta: key value}` — a generic metadata directive.
    ///
    /// The first word of the value is the metadata key name (e.g., `"artist"`),
    /// and the remainder is the metadata value. This allows setting any metadata
    /// field using the generic `{meta}` directive syntax.
    Meta(String),

    // -- Chord diagram control ------------------------------------------------
    /// `{diagrams}` / `{diagrams: on}` / `{diagrams: off}` — control chord
    /// diagram visibility. When set to "off", renderers suppress automatic
    /// chord diagrams for the current song.
    Diagrams,

    // -- Image directive ----------------------------------------------------
    /// `{image: src=filename}` — embeds an image with optional attributes.
    Image(ImageAttributes),

    // -- Config override directives -----------------------------------------
    /// `{+config.KEY: VALUE}` — overrides a configuration value for this song.
    ///
    /// The contained `String` is the dot-separated config key path
    /// (e.g., `"pdf.chorus.indent"`). The directive value holds the new
    /// setting (e.g., `"20"`).
    ConfigOverride(String),

    // -- Unknown ------------------------------------------------------------
    /// A directive not recognized as a standard ChordPro directive.
    /// The original directive name (lowercased) is preserved.
    Unknown(String),
}

impl DirectiveKind {
    /// Resolves a directive name to its [`DirectiveKind`].
    ///
    /// The lookup is case-insensitive and recognizes standard short aliases
    /// (e.g., `t` for `title`, `soc` for `start_of_chorus`).
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            // Metadata
            "title" | "t" => Self::Title,
            "subtitle" | "st" => Self::Subtitle,
            "artist" => Self::Artist,
            "composer" => Self::Composer,
            "lyricist" => Self::Lyricist,
            "album" => Self::Album,
            "year" => Self::Year,
            "key" => Self::Key,
            "tempo" => Self::Tempo,
            "time" => Self::Time,
            "capo" => Self::Capo,
            "sorttitle" => Self::SortTitle,
            "sortartist" => Self::SortArtist,
            "arranger" => Self::Arranger,
            "copyright" => Self::Copyright,
            "duration" => Self::Duration,
            "tag" => Self::Tag,

            // Transpose
            "transpose" => Self::Transpose,

            // Song boundary
            "new_song" | "ns" => Self::NewSong,

            // Formatting (comments)
            "comment" | "c" => Self::Comment,
            "comment_italic" | "ci" => Self::CommentItalic,
            "comment_box" | "cb" => Self::CommentBox,

            // Environment (sections)
            "start_of_chorus" | "soc" => Self::StartOfChorus,
            "end_of_chorus" | "eoc" => Self::EndOfChorus,
            "start_of_verse" | "sov" => Self::StartOfVerse,
            "end_of_verse" | "eov" => Self::EndOfVerse,
            "start_of_bridge" | "sob" => Self::StartOfBridge,
            "end_of_bridge" | "eob" => Self::EndOfBridge,
            "start_of_tab" | "sot" => Self::StartOfTab,
            "end_of_tab" | "eot" => Self::EndOfTab,
            "start_of_grid" | "sog" => Self::StartOfGrid,
            "end_of_grid" | "eog" => Self::EndOfGrid,

            // Font, size, and color
            "textfont" | "tf" => Self::TextFont,
            "textsize" | "ts" => Self::TextSize,
            "textcolour" | "textcolor" | "tc" => Self::TextColour,
            "chordfont" | "cf" => Self::ChordFont,
            "chordsize" | "cs" => Self::ChordSize,
            "chordcolour" | "chordcolor" | "cc" => Self::ChordColour,
            "tabfont" => Self::TabFont,
            "tabsize" => Self::TabSize,
            "tabcolour" | "tabcolor" => Self::TabColour,

            // Delegate environments (verbatim sections)
            "start_of_abc" => Self::StartOfAbc,
            "end_of_abc" => Self::EndOfAbc,
            "start_of_ly" => Self::StartOfLy,
            "end_of_ly" => Self::EndOfLy,
            "start_of_svg" => Self::StartOfSvg,
            "end_of_svg" => Self::EndOfSvg,
            "start_of_textblock" => Self::StartOfTextblock,
            "end_of_textblock" => Self::EndOfTextblock,

            // Recall
            "chorus" => Self::Chorus,

            // Page control
            "new_page" | "np" => Self::NewPage,
            "new_physical_page" | "npp" => Self::NewPhysicalPage,
            "column_break" | "colb" => Self::ColumnBreak,
            "columns" | "col" => Self::Columns,

            // Font, size, and color
            "titlefont" => Self::TitleFont,
            "titlesize" => Self::TitleSize,
            "titlecolour" | "titlecolor" => Self::TitleColour,
            "chorusfont" => Self::ChorusFont,
            "chorussize" => Self::ChorusSize,
            "choruscolour" | "choruscolor" => Self::ChorusColour,
            "footerfont" => Self::FooterFont,
            "footersize" => Self::FooterSize,
            "footercolour" | "footercolor" => Self::FooterColour,
            "headerfont" => Self::HeaderFont,
            "headersize" => Self::HeaderSize,
            "headercolour" | "headercolor" => Self::HeaderColour,
            "labelfont" => Self::LabelFont,
            "labelsize" => Self::LabelSize,
            "labelcolour" | "labelcolor" => Self::LabelColour,
            "gridfont" => Self::GridFont,
            "gridsize" => Self::GridSize,
            "gridcolour" | "gridcolor" => Self::GridColour,
            "tocfont" => Self::TocFont,
            "tocsize" => Self::TocSize,
            "toccolour" | "toccolor" => Self::TocColour,

            // Chord definitions and diagrams
            "define" => Self::Define,
            "chord" => Self::ChordDirective,
            "diagrams" => Self::Diagrams,

            // Generic metadata
            "meta" => Self::Meta(String::new()),

            // Image — recognized but requires attribute parsing by the parser.
            // from_name returns a placeholder; the parser replaces it.
            "image" => Self::Image(ImageAttributes::default()),

            // Custom sections (start_of_X / end_of_X)
            other => {
                // Config override: {+config.KEY: VALUE}
                if let Some(key) = other.strip_prefix("+config.") {
                    if !key.is_empty() {
                        return Self::ConfigOverride(key.to_string());
                    }
                }
                if let Some(section) = other.strip_prefix("start_of_") {
                    if !section.is_empty() {
                        return Self::StartOfSection(section.to_string());
                    }
                }
                if let Some(section) = other.strip_prefix("end_of_") {
                    if !section.is_empty() {
                        return Self::EndOfSection(section.to_string());
                    }
                }
                Self::Unknown(other.to_string())
            }
        }
    }

    /// Resolves a directive name to a ([`DirectiveKind`], optional selector) pair.
    ///
    /// The algorithm works as follows:
    ///
    /// 1. First try to match the full name as a known directive (via
    ///    [`from_name`](Self::from_name)). If it resolves to a **known,
    ///    non-`Unknown`, non-custom-section** directive, return it with no
    ///    selector.
    /// 2. Otherwise, split at the **last** hyphen. Re-resolve the prefix
    ///    and, if it matches a known non-`Unknown` directive, treat the
    ///    suffix as the selector.
    /// 3. If neither approach yields a known directive, return the full name
    ///    as an `Unknown` directive with no selector.
    ///
    /// Custom section directives (`StartOfSection`, `EndOfSection`) are
    /// special-cased: `{start_of_chorus-piano}` must resolve as
    /// `StartOfChorus` with selector `"piano"`, not as
    /// `StartOfSection("chorus-piano")`.
    ///
    /// The lookup is case-insensitive, matching the behavior of
    /// [`from_name`](Self::from_name).
    #[must_use]
    pub fn resolve_with_selector(name: &str) -> (Self, Option<String>) {
        let kind = Self::from_name(name);

        // If it resolves to a known directive that is NOT Unknown and NOT
        // a custom section, return it directly — no selector.
        let is_known = !matches!(
            kind,
            Self::Unknown(_) | Self::StartOfSection(_) | Self::EndOfSection(_)
        );
        if is_known {
            return (kind, None);
        }

        // Try splitting at the last hyphen.
        if let Some(last_hyphen) = name.rfind('-') {
            let prefix = &name[..last_hyphen];
            let suffix = &name[last_hyphen + 1..];

            if !prefix.is_empty() && !suffix.is_empty() {
                let prefix_kind = Self::from_name(prefix);
                if !matches!(prefix_kind, Self::Unknown(_)) {
                    return (prefix_kind, Some(suffix.to_ascii_lowercase()));
                }
            }
        }

        // Fall back to the original resolution (Unknown or custom section
        // without a selector).
        (kind, None)
    }

    /// Returns the canonical (long-form) directive name for known directives.
    ///
    /// For [`DirectiveKind::Unknown`], returns the stored name.
    #[must_use]
    pub fn canonical_name(&self) -> &str {
        match self {
            Self::Title => "title",
            Self::Subtitle => "subtitle",
            Self::Artist => "artist",
            Self::Composer => "composer",
            Self::Lyricist => "lyricist",
            Self::Album => "album",
            Self::Year => "year",
            Self::Key => "key",
            Self::Tempo => "tempo",
            Self::Time => "time",
            Self::Capo => "capo",
            Self::SortTitle => "sorttitle",
            Self::SortArtist => "sortartist",
            Self::Arranger => "arranger",
            Self::Copyright => "copyright",
            Self::Duration => "duration",
            Self::Tag => "tag",
            Self::Transpose => "transpose",
            Self::NewSong => "new_song",
            Self::Comment => "comment",
            Self::CommentItalic => "comment_italic",
            Self::CommentBox => "comment_box",
            Self::StartOfChorus => "start_of_chorus",
            Self::EndOfChorus => "end_of_chorus",
            Self::StartOfVerse => "start_of_verse",
            Self::EndOfVerse => "end_of_verse",
            Self::StartOfBridge => "start_of_bridge",
            Self::EndOfBridge => "end_of_bridge",
            Self::StartOfTab => "start_of_tab",
            Self::EndOfTab => "end_of_tab",
            Self::StartOfGrid => "start_of_grid",
            Self::EndOfGrid => "end_of_grid",

            Self::TextFont => "textfont",
            Self::TextSize => "textsize",
            Self::TextColour => "textcolour",
            Self::ChordFont => "chordfont",
            Self::ChordSize => "chordsize",
            Self::ChordColour => "chordcolour",
            Self::TabFont => "tabfont",
            Self::TabSize => "tabsize",
            Self::TabColour => "tabcolour",
            Self::TitleFont => "titlefont",
            Self::TitleSize => "titlesize",
            Self::TitleColour => "titlecolour",
            Self::ChorusFont => "chorusfont",
            Self::ChorusSize => "chorussize",
            Self::ChorusColour => "choruscolour",
            Self::FooterFont => "footerfont",
            Self::FooterSize => "footersize",
            Self::FooterColour => "footercolour",
            Self::HeaderFont => "headerfont",
            Self::HeaderSize => "headersize",
            Self::HeaderColour => "headercolour",
            Self::LabelFont => "labelfont",
            Self::LabelSize => "labelsize",
            Self::LabelColour => "labelcolour",
            Self::GridFont => "gridfont",
            Self::GridSize => "gridsize",
            Self::GridColour => "gridcolour",
            Self::TocFont => "tocfont",
            Self::TocSize => "tocsize",
            Self::TocColour => "toccolour",
            Self::StartOfAbc => "start_of_abc",
            Self::EndOfAbc => "end_of_abc",
            Self::StartOfLy => "start_of_ly",
            Self::EndOfLy => "end_of_ly",
            Self::StartOfSvg => "start_of_svg",
            Self::EndOfSvg => "end_of_svg",
            Self::StartOfTextblock => "start_of_textblock",
            Self::EndOfTextblock => "end_of_textblock",
            Self::Chorus => "chorus",
            Self::NewPage => "new_page",
            Self::NewPhysicalPage => "new_physical_page",
            Self::ColumnBreak => "column_break",
            Self::Columns => "columns",
            Self::Define => "define",
            Self::ChordDirective => "chord",
            Self::Diagrams => "diagrams",
            Self::Meta(_) => "meta",

            Self::Image(_) => "image",
            Self::ConfigOverride(key) => key.as_str(),
            Self::StartOfSection(name) | Self::EndOfSection(name) | Self::Unknown(name) => {
                name.as_str()
            }
        }
    }

    /// Returns the full canonical directive name as an owned `String`.
    ///
    /// For most directives this is the same as [`canonical_name`](Self::canonical_name).
    /// For custom section directives, the section name is prefixed with
    /// `start_of_` or `end_of_` to form the complete directive name.
    #[must_use]
    pub fn full_canonical_name(&self) -> String {
        match self {
            Self::StartOfSection(name) => format!("start_of_{name}"),
            Self::EndOfSection(name) => format!("end_of_{name}"),
            Self::ConfigOverride(key) => format!("+config.{key}"),
            _ => self.canonical_name().to_string(),
        }
    }

    /// Returns `true` if this is a metadata directive.
    #[must_use]
    pub fn is_metadata(&self) -> bool {
        matches!(
            self,
            Self::Title
                | Self::Subtitle
                | Self::Artist
                | Self::Composer
                | Self::Lyricist
                | Self::Album
                | Self::Year
                | Self::Key
                | Self::Tempo
                | Self::Time
                | Self::Capo
                | Self::SortTitle
                | Self::SortArtist
                | Self::Arranger
                | Self::Copyright
                | Self::Duration
                | Self::Tag
                | Self::Meta(_)
        )
    }

    /// Returns `true` if this is a comment/formatting directive.
    #[must_use]
    pub fn is_comment(&self) -> bool {
        matches!(self, Self::Comment | Self::CommentItalic | Self::CommentBox)
    }

    /// Returns `true` if this is a font, size, or color formatting directive.
    #[must_use]
    pub fn is_font_size_color(&self) -> bool {
        matches!(
            self,
            Self::TextFont
                | Self::TextSize
                | Self::TextColour
                | Self::ChordFont
                | Self::ChordSize
                | Self::ChordColour
                | Self::TabFont
                | Self::TabSize
                | Self::TabColour
                | Self::TitleFont
                | Self::TitleSize
                | Self::TitleColour
                | Self::ChorusFont
                | Self::ChorusSize
                | Self::ChorusColour
                | Self::FooterFont
                | Self::FooterSize
                | Self::FooterColour
                | Self::HeaderFont
                | Self::HeaderSize
                | Self::HeaderColour
                | Self::LabelFont
                | Self::LabelSize
                | Self::LabelColour
                | Self::GridFont
                | Self::GridSize
                | Self::GridColour
                | Self::TocFont
                | Self::TocSize
                | Self::TocColour
        )
    }

    /// Returns `true` if this is a section start directive.
    #[must_use]
    pub fn is_section_start(&self) -> bool {
        matches!(
            self,
            Self::StartOfChorus
                | Self::StartOfVerse
                | Self::StartOfBridge
                | Self::StartOfTab
                | Self::StartOfGrid
                | Self::StartOfAbc
                | Self::StartOfLy
                | Self::StartOfSvg
                | Self::StartOfTextblock
                | Self::StartOfSection(_)
        )
    }

    /// Returns `true` if this is a section end directive.
    #[must_use]
    pub fn is_section_end(&self) -> bool {
        matches!(
            self,
            Self::EndOfChorus
                | Self::EndOfVerse
                | Self::EndOfBridge
                | Self::EndOfTab
                | Self::EndOfGrid
                | Self::EndOfAbc
                | Self::EndOfLy
                | Self::EndOfSvg
                | Self::EndOfTextblock
                | Self::EndOfSection(_)
        )
    }

    /// Returns `true` if this is an environment (section start or end) directive.
    #[must_use]
    pub fn is_environment(&self) -> bool {
        self.is_section_start() || self.is_section_end()
    }

    /// Returns `true` if this is the image directive.
    #[must_use]
    pub fn is_image(&self) -> bool {
        matches!(self, Self::Image(_))
    }

    /// Returns `true` if this is a page control directive.
    #[must_use]
    pub fn is_page_control(&self) -> bool {
        matches!(
            self,
            Self::NewPage | Self::NewPhysicalPage | Self::ColumnBreak | Self::Columns
        )
    }
}

// ---------------------------------------------------------------------------
// Directive
// ---------------------------------------------------------------------------

/// A ChordPro directive such as `{title: My Song}` or `{start_of_chorus}`.
///
/// Directives are enclosed in curly braces and consist of a name and an
/// optional value separated by a colon. Some directives have standard short
/// aliases (e.g., `t` for `title`, `st` for `subtitle`).
///
/// The `name` field stores the **canonical** (long-form, lowercase) name after
/// alias resolution. The `kind` field provides a typed classification for
/// pattern matching.
///
/// # Selector Suffixes
///
/// Directives may carry an optional **selector suffix** that targets a specific
/// instrument or user. The selector is separated from the directive name by a
/// hyphen (e.g., `{textfont-piano: Courier}` has selector `"piano"`). The
/// parser splits the raw directive name at the **last** hyphen to detect
/// selectors: if the prefix resolves to a known directive, the suffix is
/// stored in `selector`; otherwise the entire name is treated as a single
/// (possibly unknown) directive with no selector.
///
/// # Examples
///
/// ```
/// use chordpro_core::ast::{Directive, DirectiveKind};
///
/// // {title: My Song}
/// let d = Directive::with_value("title", "My Song");
/// assert_eq!(d.name, "title");
/// assert_eq!(d.value.as_deref(), Some("My Song"));
/// assert_eq!(d.kind, DirectiveKind::Title);
/// assert_eq!(d.selector, None);
///
/// // {start_of_chorus}
/// let d = Directive::name_only("start_of_chorus");
/// assert_eq!(d.name, "start_of_chorus");
/// assert!(d.value.is_none());
/// assert_eq!(d.kind, DirectiveKind::StartOfChorus);
/// assert_eq!(d.selector, None);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Directive {
    /// The canonical directive name (e.g., `"title"`, `"start_of_chorus"`).
    pub name: String,
    /// The optional value after the colon (e.g., `"My Song"` in `{title: My Song}`).
    pub value: Option<String>,
    /// The classified kind of this directive.
    pub kind: DirectiveKind,
    /// An optional selector suffix for instrument/user targeting.
    ///
    /// For example, `{textfont-piano: Courier}` has `selector` = `Some("piano")`.
    /// When no selector suffix is present, this is `None`.
    pub selector: Option<String>,
}

impl Directive {
    /// Creates a directive with both a name and a value.
    ///
    /// The name is resolved to its canonical form and the [`DirectiveKind`]
    /// is determined automatically.
    #[must_use]
    pub fn with_value(name: impl Into<String>, value: impl Into<String>) -> Self {
        let name_str = name.into();
        let kind = DirectiveKind::from_name(&name_str);
        let canonical = kind.full_canonical_name();
        Self {
            name: canonical,
            value: Some(value.into()),
            kind,
            selector: None,
        }
    }

    /// Creates a directive with only a name and no value.
    ///
    /// The name is resolved to its canonical form and the [`DirectiveKind`]
    /// is determined automatically.
    #[must_use]
    pub fn name_only(name: impl Into<String>) -> Self {
        let name_str = name.into();
        let kind = DirectiveKind::from_name(&name_str);
        let canonical = kind.full_canonical_name();
        Self {
            name: canonical,
            value: None,
            kind,
            selector: None,
        }
    }

    /// Creates a directive with a name, value, and selector suffix.
    ///
    /// The name is resolved to its canonical form and the [`DirectiveKind`]
    /// is determined automatically.
    #[must_use]
    pub fn with_selector(
        name: impl Into<String>,
        value: Option<String>,
        selector: impl Into<String>,
    ) -> Self {
        let name_str = name.into();
        let kind = DirectiveKind::from_name(&name_str);
        let canonical = kind.full_canonical_name();
        Self {
            name: canonical,
            value,
            kind,
            selector: Some(selector.into()),
        }
    }

    /// Returns `true` if this directive marks the start of a section
    /// (e.g., `start_of_chorus`, `start_of_verse`, etc.).
    #[must_use]
    pub fn is_section_start(&self) -> bool {
        self.kind.is_section_start()
    }

    /// Returns `true` if this directive marks the end of a section
    /// (e.g., `end_of_chorus`, `end_of_verse`, etc.).
    #[must_use]
    pub fn is_section_end(&self) -> bool {
        self.kind.is_section_end()
    }

    /// If this is a section start or end directive, returns the section name
    /// (e.g., `"chorus"` from `"start_of_chorus"`).
    #[must_use]
    pub fn section_name(&self) -> Option<&str> {
        if let Some(suffix) = self.name.strip_prefix("start_of_") {
            Some(suffix)
        } else if let Some(suffix) = self.name.strip_prefix("end_of_") {
            Some(suffix)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chord::{Accidental, ChordQuality, Note};

    // -- Song ---------------------------------------------------------------

    #[test]
    fn song_new_is_empty() {
        let song = Song::new();
        assert!(song.lines.is_empty());
        assert_eq!(song.metadata.title, None);
    }

    #[test]
    fn song_default_equals_new() {
        assert_eq!(Song::default(), Song::new());
    }

    #[test]
    fn song_with_lines() {
        let mut song = Song::new();
        song.metadata.title = Some("My Song".to_string());
        song.lines.push(Line::Empty);
        song.lines
            .push(Line::Comment(CommentStyle::Normal, "A comment".to_string()));
        assert_eq!(song.lines.len(), 2);
        assert_eq!(song.metadata.title.as_deref(), Some("My Song"));
    }

    // -- Metadata -----------------------------------------------------------

    #[test]
    fn metadata_default_is_empty() {
        let meta = Metadata::new();
        assert_eq!(meta.title, None);
        assert!(meta.subtitles.is_empty());
        assert!(meta.artists.is_empty());
        assert!(meta.composers.is_empty());
        assert!(meta.lyricists.is_empty());
        assert_eq!(meta.album, None);
        assert_eq!(meta.year, None);
        assert_eq!(meta.key, None);
        assert_eq!(meta.tempo, None);
        assert_eq!(meta.time, None);
        assert_eq!(meta.capo, None);
        assert_eq!(meta.sort_title, None);
        assert_eq!(meta.sort_artist, None);
        assert!(meta.arrangers.is_empty());
        assert_eq!(meta.copyright, None);
        assert_eq!(meta.duration, None);
        assert!(meta.tags.is_empty());
        assert!(meta.custom.is_empty());
    }

    // -- LyricsLine ---------------------------------------------------------

    #[test]
    fn lyrics_line_text_concatenation() {
        let line = LyricsLine {
            segments: vec![
                LyricsSegment::new(Some(Chord::new("Am")), "Hello "),
                LyricsSegment::new(Some(Chord::new("G")), "world"),
            ],
        };
        assert_eq!(line.text(), "Hello world");
    }

    #[test]
    fn lyrics_line_has_chords() {
        let with_chords = LyricsLine {
            segments: vec![LyricsSegment::new(Some(Chord::new("C")), "text")],
        };
        assert!(with_chords.has_chords());

        let without_chords = LyricsLine {
            segments: vec![LyricsSegment::text_only("just text")],
        };
        assert!(!without_chords.has_chords());
    }

    #[test]
    fn lyrics_line_empty_default() {
        let line = LyricsLine::new();
        assert!(line.segments.is_empty());
        assert_eq!(line.text(), "");
        assert!(!line.has_chords());
    }

    // -- LyricsSegment ------------------------------------------------------

    #[test]
    fn segment_text_only() {
        let seg = LyricsSegment::text_only("hello");
        assert_eq!(seg.chord, None);
        assert_eq!(seg.text, "hello");
    }

    #[test]
    fn segment_chord_only() {
        let seg = LyricsSegment::chord_only(Chord::new("Dm"));
        assert_eq!(seg.chord, Some(Chord::new("Dm")));
        assert!(seg.text.is_empty());
    }

    #[test]
    fn segment_with_chord_and_text() {
        let seg = LyricsSegment::new(Some(Chord::new("E7")), "lyrics");
        assert_eq!(seg.chord.as_ref().map(|c| c.name.as_str()), Some("E7"));
        assert_eq!(seg.text, "lyrics");
    }

    // -- Chord --------------------------------------------------------------

    #[test]
    fn chord_display() {
        let chord = Chord::new("F#m7");
        assert_eq!(format!("{chord}"), "F#m7");
    }

    #[test]
    fn chord_equality() {
        assert_eq!(Chord::new("Am"), Chord::new("Am"));
        assert_ne!(Chord::new("Am"), Chord::new("Bm"));
    }

    #[test]
    fn chord_detail_parsed() {
        let chord = Chord::new("C#m7");
        let detail = chord.detail.as_ref().expect("should have detail");
        assert_eq!(detail.root, Note::C);
        assert_eq!(detail.root_accidental, Some(Accidental::Sharp));
        assert_eq!(detail.quality, ChordQuality::Minor);
        assert_eq!(detail.extension.as_deref(), Some("7"));
    }

    #[test]
    fn chord_detail_slash_chord() {
        let chord = Chord::new("G/B");
        let detail = chord.detail.as_ref().expect("should have detail");
        assert_eq!(detail.root, Note::G);
        assert_eq!(detail.bass_note, Some((Note::B, None)));
    }

    #[test]
    fn chord_detail_unparseable() {
        let chord = Chord::new("");
        assert!(chord.detail.is_none());
        assert_eq!(chord.name, "");
    }

    #[test]
    fn chord_detail_invalid_notation() {
        let chord = Chord::new("xyz");
        assert!(chord.detail.is_none());
        assert_eq!(chord.name, "xyz");
    }

    // -- DirectiveKind ------------------------------------------------------

    #[test]
    fn directive_kind_from_name_metadata() {
        assert_eq!(DirectiveKind::from_name("title"), DirectiveKind::Title);
        assert_eq!(DirectiveKind::from_name("t"), DirectiveKind::Title);
        assert_eq!(DirectiveKind::from_name("TITLE"), DirectiveKind::Title);
        assert_eq!(DirectiveKind::from_name("Title"), DirectiveKind::Title);
        assert_eq!(
            DirectiveKind::from_name("subtitle"),
            DirectiveKind::Subtitle
        );
        assert_eq!(DirectiveKind::from_name("st"), DirectiveKind::Subtitle);
        assert_eq!(DirectiveKind::from_name("artist"), DirectiveKind::Artist);
        assert_eq!(
            DirectiveKind::from_name("composer"),
            DirectiveKind::Composer
        );
        assert_eq!(
            DirectiveKind::from_name("lyricist"),
            DirectiveKind::Lyricist
        );
        assert_eq!(DirectiveKind::from_name("album"), DirectiveKind::Album);
        assert_eq!(DirectiveKind::from_name("year"), DirectiveKind::Year);
        assert_eq!(DirectiveKind::from_name("key"), DirectiveKind::Key);
        assert_eq!(DirectiveKind::from_name("tempo"), DirectiveKind::Tempo);
        assert_eq!(DirectiveKind::from_name("time"), DirectiveKind::Time);
        assert_eq!(DirectiveKind::from_name("capo"), DirectiveKind::Capo);
        assert_eq!(
            DirectiveKind::from_name("sorttitle"),
            DirectiveKind::SortTitle
        );
        assert_eq!(
            DirectiveKind::from_name("SORTTITLE"),
            DirectiveKind::SortTitle
        );
        assert_eq!(
            DirectiveKind::from_name("sortartist"),
            DirectiveKind::SortArtist
        );
        assert_eq!(
            DirectiveKind::from_name("arranger"),
            DirectiveKind::Arranger
        );
        assert_eq!(
            DirectiveKind::from_name("copyright"),
            DirectiveKind::Copyright
        );
        assert_eq!(
            DirectiveKind::from_name("duration"),
            DirectiveKind::Duration
        );
        assert_eq!(DirectiveKind::from_name("tag"), DirectiveKind::Tag);
    }

    #[test]
    fn directive_kind_from_name_comment() {
        assert_eq!(DirectiveKind::from_name("comment"), DirectiveKind::Comment);
        assert_eq!(DirectiveKind::from_name("c"), DirectiveKind::Comment);
        assert_eq!(
            DirectiveKind::from_name("comment_italic"),
            DirectiveKind::CommentItalic
        );
        assert_eq!(DirectiveKind::from_name("ci"), DirectiveKind::CommentItalic);
        assert_eq!(
            DirectiveKind::from_name("comment_box"),
            DirectiveKind::CommentBox
        );
        assert_eq!(DirectiveKind::from_name("cb"), DirectiveKind::CommentBox);
    }

    #[test]
    fn directive_kind_from_name_environment() {
        assert_eq!(
            DirectiveKind::from_name("start_of_chorus"),
            DirectiveKind::StartOfChorus
        );
        assert_eq!(
            DirectiveKind::from_name("soc"),
            DirectiveKind::StartOfChorus
        );
        assert_eq!(
            DirectiveKind::from_name("end_of_chorus"),
            DirectiveKind::EndOfChorus
        );
        assert_eq!(DirectiveKind::from_name("eoc"), DirectiveKind::EndOfChorus);
        assert_eq!(
            DirectiveKind::from_name("start_of_verse"),
            DirectiveKind::StartOfVerse
        );
        assert_eq!(DirectiveKind::from_name("sov"), DirectiveKind::StartOfVerse);
        assert_eq!(
            DirectiveKind::from_name("end_of_verse"),
            DirectiveKind::EndOfVerse
        );
        assert_eq!(DirectiveKind::from_name("eov"), DirectiveKind::EndOfVerse);
        assert_eq!(
            DirectiveKind::from_name("start_of_bridge"),
            DirectiveKind::StartOfBridge
        );
        assert_eq!(
            DirectiveKind::from_name("sob"),
            DirectiveKind::StartOfBridge
        );
        assert_eq!(
            DirectiveKind::from_name("end_of_bridge"),
            DirectiveKind::EndOfBridge
        );
        assert_eq!(DirectiveKind::from_name("eob"), DirectiveKind::EndOfBridge);
        assert_eq!(
            DirectiveKind::from_name("start_of_tab"),
            DirectiveKind::StartOfTab
        );
        assert_eq!(DirectiveKind::from_name("sot"), DirectiveKind::StartOfTab);
        assert_eq!(
            DirectiveKind::from_name("end_of_tab"),
            DirectiveKind::EndOfTab
        );
        assert_eq!(DirectiveKind::from_name("eot"), DirectiveKind::EndOfTab);
    }

    #[test]
    fn directive_kind_from_name_page_control() {
        assert_eq!(DirectiveKind::from_name("new_page"), DirectiveKind::NewPage);
        assert_eq!(DirectiveKind::from_name("np"), DirectiveKind::NewPage);
        assert_eq!(
            DirectiveKind::from_name("new_physical_page"),
            DirectiveKind::NewPhysicalPage
        );
        assert_eq!(
            DirectiveKind::from_name("npp"),
            DirectiveKind::NewPhysicalPage
        );
        assert_eq!(
            DirectiveKind::from_name("column_break"),
            DirectiveKind::ColumnBreak
        );
        assert_eq!(DirectiveKind::from_name("colb"), DirectiveKind::ColumnBreak);
        assert_eq!(DirectiveKind::from_name("columns"), DirectiveKind::Columns);
        assert_eq!(DirectiveKind::from_name("col"), DirectiveKind::Columns);
    }
    #[test]
    fn directive_kind_from_name_unknown() {
        let kind = DirectiveKind::from_name("custom_thing");
        assert_eq!(kind, DirectiveKind::Unknown("custom_thing".to_string()));
    }

    #[test]
    fn directive_kind_case_insensitive() {
        assert_eq!(DirectiveKind::from_name("TITLE"), DirectiveKind::Title);
        assert_eq!(DirectiveKind::from_name("Title"), DirectiveKind::Title);
        assert_eq!(
            DirectiveKind::from_name("START_OF_CHORUS"),
            DirectiveKind::StartOfChorus
        );
        assert_eq!(
            DirectiveKind::from_name("Comment_Italic"),
            DirectiveKind::CommentItalic
        );
        assert_eq!(DirectiveKind::from_name("NEW_PAGE"), DirectiveKind::NewPage);
        assert_eq!(
            DirectiveKind::from_name("Column_Break"),
            DirectiveKind::ColumnBreak
        );
    }

    #[test]
    fn directive_kind_canonical_name() {
        assert_eq!(DirectiveKind::Title.canonical_name(), "title");
        assert_eq!(
            DirectiveKind::StartOfChorus.canonical_name(),
            "start_of_chorus"
        );
        assert_eq!(DirectiveKind::Comment.canonical_name(), "comment");
        assert_eq!(
            DirectiveKind::Unknown("foo".to_string()).canonical_name(),
            "foo"
        );
        assert_eq!(DirectiveKind::SortTitle.canonical_name(), "sorttitle");
        assert_eq!(DirectiveKind::SortArtist.canonical_name(), "sortartist");
        assert_eq!(DirectiveKind::Arranger.canonical_name(), "arranger");
        assert_eq!(DirectiveKind::Copyright.canonical_name(), "copyright");
        assert_eq!(DirectiveKind::Duration.canonical_name(), "duration");
        assert_eq!(DirectiveKind::Tag.canonical_name(), "tag");
        assert_eq!(DirectiveKind::NewPage.canonical_name(), "new_page");
        assert_eq!(
            DirectiveKind::NewPhysicalPage.canonical_name(),
            "new_physical_page"
        );
        assert_eq!(DirectiveKind::ColumnBreak.canonical_name(), "column_break");
        assert_eq!(DirectiveKind::Columns.canonical_name(), "columns");
    }

    #[test]
    fn directive_kind_category_checks() {
        assert!(DirectiveKind::Title.is_metadata());
        assert!(!DirectiveKind::Title.is_comment());
        assert!(!DirectiveKind::Title.is_environment());

        assert!(DirectiveKind::Comment.is_comment());
        assert!(!DirectiveKind::Comment.is_metadata());

        assert!(DirectiveKind::StartOfChorus.is_section_start());
        assert!(DirectiveKind::StartOfChorus.is_environment());
        assert!(!DirectiveKind::StartOfChorus.is_section_end());

        assert!(DirectiveKind::EndOfChorus.is_section_end());
        assert!(DirectiveKind::EndOfChorus.is_environment());
        assert!(!DirectiveKind::EndOfChorus.is_section_start());

        let unknown = DirectiveKind::Unknown("x".to_string());
        assert!(!unknown.is_metadata());
        assert!(!unknown.is_comment());
        assert!(!unknown.is_environment());

        assert!(DirectiveKind::NewPage.is_page_control());
        assert!(DirectiveKind::NewPhysicalPage.is_page_control());
        assert!(DirectiveKind::ColumnBreak.is_page_control());
        assert!(DirectiveKind::Columns.is_page_control());
        assert!(!DirectiveKind::NewPage.is_metadata());
        assert!(!DirectiveKind::NewPage.is_comment());
        assert!(!DirectiveKind::NewPage.is_environment());
        assert!(!DirectiveKind::Title.is_page_control());
        assert!(!unknown.is_page_control());
    }

    // -- Directive ----------------------------------------------------------

    #[test]
    fn directive_with_value() {
        let d = Directive::with_value("title", "My Song");
        assert_eq!(d.name, "title");
        assert_eq!(d.value.as_deref(), Some("My Song"));
        assert_eq!(d.kind, DirectiveKind::Title);
    }

    #[test]
    fn directive_name_only() {
        let d = Directive::name_only("start_of_chorus");
        assert_eq!(d.name, "start_of_chorus");
        assert!(d.value.is_none());
        assert_eq!(d.kind, DirectiveKind::StartOfChorus);
    }

    #[test]
    fn directive_short_alias_resolution() {
        let d = Directive::with_value("t", "My Song");
        assert_eq!(d.name, "title");
        assert_eq!(d.kind, DirectiveKind::Title);

        let d = Directive::name_only("soc");
        assert_eq!(d.name, "start_of_chorus");
        assert_eq!(d.kind, DirectiveKind::StartOfChorus);

        let d = Directive::with_value("st", "Alternate Title");
        assert_eq!(d.name, "subtitle");
        assert_eq!(d.kind, DirectiveKind::Subtitle);
    }

    #[test]
    fn directive_case_insensitive_resolution() {
        let d = Directive::with_value("TITLE", "My Song");
        assert_eq!(d.name, "title");
        assert_eq!(d.kind, DirectiveKind::Title);

        let d = Directive::name_only("SOC");
        assert_eq!(d.name, "start_of_chorus");
        assert_eq!(d.kind, DirectiveKind::StartOfChorus);
    }

    #[test]
    fn directive_unknown_preserves_name() {
        let d = Directive::with_value("my_custom", "value");
        assert_eq!(d.name, "my_custom");
        assert_eq!(d.kind, DirectiveKind::Unknown("my_custom".to_string()));
    }

    #[test]
    fn directive_section_detection() {
        let soc = Directive::name_only("start_of_chorus");
        assert!(soc.is_section_start());
        assert!(!soc.is_section_end());
        assert_eq!(soc.section_name(), Some("chorus"));

        let eoc = Directive::name_only("end_of_chorus");
        assert!(!eoc.is_section_start());
        assert!(eoc.is_section_end());
        assert_eq!(eoc.section_name(), Some("chorus"));

        let title = Directive::with_value("title", "Test");
        assert!(!title.is_section_start());
        assert!(!title.is_section_end());
        assert_eq!(title.section_name(), None);
    }

    #[test]
    fn directive_section_name_variants() {
        let sov = Directive::name_only("start_of_verse");
        assert_eq!(sov.section_name(), Some("verse"));

        let eob = Directive::name_only("end_of_bridge");
        assert_eq!(eob.section_name(), Some("bridge"));
    }

    #[test]
    fn directive_section_detection_via_short_alias() {
        let soc = Directive::name_only("soc");
        assert!(soc.is_section_start());
        assert_eq!(soc.section_name(), Some("chorus"));

        let eot = Directive::name_only("eot");
        assert!(eot.is_section_end());
        assert_eq!(eot.section_name(), Some("tab"));
    }

    // -- Custom section directives ------------------------------------------

    #[test]
    fn directive_kind_start_of_custom_section() {
        let kind = DirectiveKind::from_name("start_of_intro");
        assert_eq!(kind, DirectiveKind::StartOfSection("intro".to_string()));
        assert!(kind.is_section_start());
        assert!(!kind.is_section_end());
        assert!(kind.is_environment());
    }

    #[test]
    fn directive_kind_end_of_custom_section() {
        let kind = DirectiveKind::from_name("end_of_intro");
        assert_eq!(kind, DirectiveKind::EndOfSection("intro".to_string()));
        assert!(kind.is_section_end());
        assert!(!kind.is_section_start());
        assert!(kind.is_environment());
    }

    #[test]
    fn directive_kind_custom_section_case_insensitive() {
        let kind = DirectiveKind::from_name("Start_Of_Intro");
        assert_eq!(kind, DirectiveKind::StartOfSection("intro".to_string()));
    }

    #[test]
    fn directive_kind_custom_section_various_names() {
        assert_eq!(
            DirectiveKind::from_name("start_of_outro"),
            DirectiveKind::StartOfSection("outro".to_string())
        );
        assert_eq!(
            DirectiveKind::from_name("start_of_solo"),
            DirectiveKind::StartOfSection("solo".to_string())
        );
        assert_eq!(
            DirectiveKind::from_name("end_of_solo"),
            DirectiveKind::EndOfSection("solo".to_string())
        );
        assert_eq!(
            DirectiveKind::from_name("start_of_interlude"),
            DirectiveKind::StartOfSection("interlude".to_string())
        );
    }

    // -- Font, size, and color directives -----------------------------------

    #[test]
    fn directive_kind_from_name_font_size_color() {
        // Text font directives
        assert_eq!(
            DirectiveKind::from_name("textfont"),
            DirectiveKind::TextFont
        );
        assert_eq!(DirectiveKind::from_name("tf"), DirectiveKind::TextFont);
        assert_eq!(
            DirectiveKind::from_name("TEXTFONT"),
            DirectiveKind::TextFont
        );
        assert_eq!(
            DirectiveKind::from_name("textsize"),
            DirectiveKind::TextSize
        );
        assert_eq!(DirectiveKind::from_name("ts"), DirectiveKind::TextSize);
        assert_eq!(
            DirectiveKind::from_name("textcolour"),
            DirectiveKind::TextColour
        );
        assert_eq!(
            DirectiveKind::from_name("textcolor"),
            DirectiveKind::TextColour
        );
        assert_eq!(DirectiveKind::from_name("tc"), DirectiveKind::TextColour);

        // Chord font directives
        assert_eq!(
            DirectiveKind::from_name("chordfont"),
            DirectiveKind::ChordFont
        );
        assert_eq!(DirectiveKind::from_name("cf"), DirectiveKind::ChordFont);
        assert_eq!(
            DirectiveKind::from_name("chordsize"),
            DirectiveKind::ChordSize
        );
        assert_eq!(DirectiveKind::from_name("cs"), DirectiveKind::ChordSize);
        assert_eq!(
            DirectiveKind::from_name("chordcolour"),
            DirectiveKind::ChordColour
        );
        assert_eq!(
            DirectiveKind::from_name("chordcolor"),
            DirectiveKind::ChordColour
        );
        assert_eq!(DirectiveKind::from_name("cc"), DirectiveKind::ChordColour);

        // Tab font directives
        assert_eq!(DirectiveKind::from_name("tabfont"), DirectiveKind::TabFont);
        assert_eq!(DirectiveKind::from_name("tabsize"), DirectiveKind::TabSize);
        assert_eq!(
            DirectiveKind::from_name("tabcolour"),
            DirectiveKind::TabColour
        );
        assert_eq!(
            DirectiveKind::from_name("tabcolor"),
            DirectiveKind::TabColour
        );
    }

    #[test]
    fn directive_custom_section_full_canonical_name() {
        let kind = DirectiveKind::StartOfSection("intro".to_string());
        assert_eq!(kind.full_canonical_name(), "start_of_intro");

        let kind = DirectiveKind::EndOfSection("outro".to_string());
        assert_eq!(kind.full_canonical_name(), "end_of_outro");
    }

    #[test]
    fn directive_custom_section_name_only() {
        let d = Directive::name_only("start_of_intro");
        assert_eq!(d.name, "start_of_intro");
        assert!(d.value.is_none());
        assert_eq!(d.kind, DirectiveKind::StartOfSection("intro".to_string()));
        assert!(d.is_section_start());
        assert_eq!(d.section_name(), Some("intro"));
    }

    #[test]
    fn directive_custom_section_with_label() {
        let d = Directive::with_value("start_of_intro", "Guitar Intro");
        assert_eq!(d.name, "start_of_intro");
        assert_eq!(d.value.as_deref(), Some("Guitar Intro"));
        assert_eq!(d.kind, DirectiveKind::StartOfSection("intro".to_string()));
    }

    #[test]
    fn directive_end_custom_section() {
        let d = Directive::name_only("end_of_intro");
        assert_eq!(d.name, "end_of_intro");
        assert!(d.is_section_end());
        assert_eq!(d.section_name(), Some("intro"));
    }

    #[test]
    fn directive_known_sections_not_custom() {
        // Built-in sections should NOT produce StartOfSection/EndOfSection
        assert_eq!(
            DirectiveKind::from_name("start_of_chorus"),
            DirectiveKind::StartOfChorus
        );
        assert_eq!(
            DirectiveKind::from_name("start_of_verse"),
            DirectiveKind::StartOfVerse
        );
        assert_eq!(
            DirectiveKind::from_name("start_of_bridge"),
            DirectiveKind::StartOfBridge
        );
        assert_eq!(
            DirectiveKind::from_name("start_of_tab"),
            DirectiveKind::StartOfTab
        );
    }

    #[test]
    fn directive_kind_font_size_color_canonical_names() {
        assert_eq!(DirectiveKind::TextFont.canonical_name(), "textfont");
        assert_eq!(DirectiveKind::TextSize.canonical_name(), "textsize");
        assert_eq!(DirectiveKind::TextColour.canonical_name(), "textcolour");
        assert_eq!(DirectiveKind::ChordFont.canonical_name(), "chordfont");
        assert_eq!(DirectiveKind::ChordSize.canonical_name(), "chordsize");
        assert_eq!(DirectiveKind::ChordColour.canonical_name(), "chordcolour");
        assert_eq!(DirectiveKind::TabFont.canonical_name(), "tabfont");
        assert_eq!(DirectiveKind::TabSize.canonical_name(), "tabsize");
        assert_eq!(DirectiveKind::TabColour.canonical_name(), "tabcolour");
    }

    #[test]
    fn directive_kind_font_size_color_category_checks() {
        let font_kinds = [
            DirectiveKind::TextFont,
            DirectiveKind::TextSize,
            DirectiveKind::TextColour,
            DirectiveKind::ChordFont,
            DirectiveKind::ChordSize,
            DirectiveKind::ChordColour,
            DirectiveKind::TabFont,
            DirectiveKind::TabSize,
            DirectiveKind::TabColour,
        ];
        for kind in &font_kinds {
            assert!(
                kind.is_font_size_color(),
                "{kind:?} should be font_size_color"
            );
            assert!(!kind.is_metadata(), "{kind:?} should not be metadata");
            assert!(!kind.is_comment(), "{kind:?} should not be comment");
            assert!(!kind.is_environment(), "{kind:?} should not be environment");
        }
    }

    #[test]
    fn directive_font_alias_resolution() {
        let d = Directive::with_value("tf", "Times");
        assert_eq!(d.name, "textfont");
        assert_eq!(d.kind, DirectiveKind::TextFont);
        assert_eq!(d.value.as_deref(), Some("Times"));

        let d = Directive::with_value("cc", "#FF0000");
        assert_eq!(d.name, "chordcolour");
        assert_eq!(d.kind, DirectiveKind::ChordColour);

        let d = Directive::with_value("textcolor", "blue");
        assert_eq!(d.name, "textcolour");
        assert_eq!(d.kind, DirectiveKind::TextColour);
    }

    // -- CommentStyle -------------------------------------------------------

    #[test]
    fn comment_style_variants() {
        let normal = Line::Comment(CommentStyle::Normal, "text".to_string());
        let italic = Line::Comment(CommentStyle::Italic, "text".to_string());
        let boxed = Line::Comment(CommentStyle::Boxed, "text".to_string());

        assert!(matches!(normal, Line::Comment(CommentStyle::Normal, _)));
        assert!(matches!(italic, Line::Comment(CommentStyle::Italic, _)));
        assert!(matches!(boxed, Line::Comment(CommentStyle::Boxed, _)));
    }

    // -- Line enum ----------------------------------------------------------

    #[test]
    fn line_enum_variants() {
        let lyrics = Line::Lyrics(LyricsLine::new());
        let directive = Line::Directive(Directive::name_only("soc"));
        let comment = Line::Comment(CommentStyle::Normal, "test".to_string());
        let empty = Line::Empty;

        // Ensure they are all distinct variants via pattern matching
        assert!(matches!(lyrics, Line::Lyrics(_)));
        assert!(matches!(directive, Line::Directive(_)));
        assert!(matches!(comment, Line::Comment(..)));
        assert!(matches!(empty, Line::Empty));
    }

    #[test]
    fn line_clone_and_eq() {
        let line = Line::Lyrics(LyricsLine {
            segments: vec![LyricsSegment::new(Some(Chord::new("C")), "hello")],
        });
        let cloned = line.clone();
        assert_eq!(line, cloned);
    }

    // -- Directive selector -------------------------------------------------

    #[test]
    fn directive_with_selector_constructor() {
        let d = Directive::with_selector("title", Some("My Song".to_string()), "piano");
        assert_eq!(d.name, "title");
        assert_eq!(d.value.as_deref(), Some("My Song"));
        assert_eq!(d.kind, DirectiveKind::Title);
        assert_eq!(d.selector.as_deref(), Some("piano"));
    }

    #[test]
    fn directive_with_value_has_no_selector() {
        let d = Directive::with_value("title", "My Song");
        assert_eq!(d.selector, None);
    }

    #[test]
    fn directive_name_only_has_no_selector() {
        let d = Directive::name_only("start_of_chorus");
        assert_eq!(d.selector, None);
    }

    #[test]
    fn resolve_with_selector_plain_directive() {
        let (kind, sel) = DirectiveKind::resolve_with_selector("title");
        assert_eq!(kind, DirectiveKind::Title);
        assert_eq!(sel, None);
    }

    #[test]
    fn resolve_with_selector_with_suffix() {
        let (kind, sel) = DirectiveKind::resolve_with_selector("title-piano");
        assert_eq!(kind, DirectiveKind::Title);
        assert_eq!(sel.as_deref(), Some("piano"));
    }

    #[test]
    fn resolve_with_selector_comment() {
        let (kind, sel) = DirectiveKind::resolve_with_selector("comment-bass");
        assert_eq!(kind, DirectiveKind::Comment);
        assert_eq!(sel.as_deref(), Some("bass"));
    }

    #[test]
    fn resolve_with_selector_comment_italic() {
        let (kind, sel) = DirectiveKind::resolve_with_selector("comment_italic-guitar");
        assert_eq!(kind, DirectiveKind::CommentItalic);
        assert_eq!(sel.as_deref(), Some("guitar"));
    }

    #[test]
    fn resolve_with_selector_environment() {
        let (kind, sel) = DirectiveKind::resolve_with_selector("start_of_chorus-piano");
        assert_eq!(kind, DirectiveKind::StartOfChorus);
        assert_eq!(sel.as_deref(), Some("piano"));
    }

    #[test]
    fn resolve_with_selector_end_of_tab() {
        let (kind, sel) = DirectiveKind::resolve_with_selector("end_of_tab-guitar");
        assert_eq!(kind, DirectiveKind::EndOfTab);
        assert_eq!(sel.as_deref(), Some("guitar"));
    }

    #[test]
    fn resolve_with_selector_custom_section_no_selector() {
        let (kind, sel) = DirectiveKind::resolve_with_selector("start_of_intro");
        assert_eq!(kind, DirectiveKind::StartOfSection("intro".to_string()));
        assert_eq!(sel, None);
    }

    #[test]
    fn resolve_with_selector_custom_section_with_selector() {
        // start_of_intro-piano: "start_of_intro" resolves to StartOfSection("intro"),
        // which is a custom section. The last hyphen splits into "start_of_intro" + "piano".
        // "start_of_intro" is NOT Unknown (it's StartOfSection), so we split successfully.
        let (kind, sel) = DirectiveKind::resolve_with_selector("start_of_intro-piano");
        assert_eq!(kind, DirectiveKind::StartOfSection("intro".to_string()));
        assert_eq!(sel.as_deref(), Some("piano"));
    }

    #[test]
    fn resolve_with_selector_unknown_no_hyphen() {
        let (kind, sel) = DirectiveKind::resolve_with_selector("mything");
        assert_eq!(kind, DirectiveKind::Unknown("mything".to_string()));
        assert_eq!(sel, None);
    }

    #[test]
    fn resolve_with_selector_unknown_with_hyphen() {
        // "my-thing" -> prefix "my" is Unknown, so no selector is detected.
        let (kind, sel) = DirectiveKind::resolve_with_selector("my-thing");
        assert_eq!(kind, DirectiveKind::Unknown("my-thing".to_string()));
        assert_eq!(sel, None);
    }

    #[test]
    fn resolve_with_selector_case_insensitive() {
        let (kind, sel) = DirectiveKind::resolve_with_selector("Title-Piano");
        assert_eq!(kind, DirectiveKind::Title);
        assert_eq!(sel.as_deref(), Some("piano"));
    }

    #[test]
    fn resolve_with_selector_short_alias_with_suffix() {
        let (kind, sel) = DirectiveKind::resolve_with_selector("t-guitar");
        assert_eq!(kind, DirectiveKind::Title);
        assert_eq!(sel.as_deref(), Some("guitar"));
    }

    // -- Integration: full song construction --------------------------------

    #[test]
    fn full_song_construction() {
        let mut song = Song::new();
        song.metadata.title = Some("Amazing Grace".to_string());
        song.metadata.key = Some("G".to_string());
        song.metadata.artists.push("John Newton".to_string());

        // {start_of_verse}
        song.lines
            .push(Line::Directive(Directive::name_only("start_of_verse")));

        // [G]Amazing [G7]grace, how [C]sweet the [G]sound
        song.lines.push(Line::Lyrics(LyricsLine {
            segments: vec![
                LyricsSegment::new(Some(Chord::new("G")), "Amazing "),
                LyricsSegment::new(Some(Chord::new("G7")), "grace, how "),
                LyricsSegment::new(Some(Chord::new("C")), "sweet the "),
                LyricsSegment::new(Some(Chord::new("G")), "sound"),
            ],
        }));

        // {end_of_verse}
        song.lines
            .push(Line::Directive(Directive::name_only("end_of_verse")));

        assert_eq!(song.lines.len(), 3);
        if let Line::Lyrics(ref lyrics) = song.lines[1] {
            assert_eq!(lyrics.text(), "Amazing grace, how sweet the sound");
            assert!(lyrics.has_chords());
            assert_eq!(lyrics.segments.len(), 4);
        } else {
            panic!("Expected Line::Lyrics");
        }
    }

    #[test]
    fn directive_kind_grid_from_name() {
        assert_eq!(
            DirectiveKind::from_name("start_of_grid"),
            DirectiveKind::StartOfGrid
        );
        assert_eq!(
            DirectiveKind::from_name("end_of_grid"),
            DirectiveKind::EndOfGrid
        );
        assert_eq!(DirectiveKind::from_name("sog"), DirectiveKind::StartOfGrid);
        assert_eq!(DirectiveKind::from_name("eog"), DirectiveKind::EndOfGrid);
    }

    #[test]
    fn directive_kind_grid_canonical_name() {
        assert_eq!(DirectiveKind::StartOfGrid.canonical_name(), "start_of_grid");
        assert_eq!(DirectiveKind::EndOfGrid.canonical_name(), "end_of_grid");
    }

    #[test]
    fn directive_kind_grid_is_section() {
        assert!(DirectiveKind::StartOfGrid.is_section_start());
        assert!(!DirectiveKind::StartOfGrid.is_section_end());
        assert!(DirectiveKind::EndOfGrid.is_section_end());
        assert!(!DirectiveKind::EndOfGrid.is_section_start());
        assert!(DirectiveKind::StartOfGrid.is_environment());
        assert!(DirectiveKind::EndOfGrid.is_environment());
    }

    #[test]
    fn directive_grid_section_name() {
        let sog = Directive::name_only("sog");
        assert!(sog.is_section_start());
        assert_eq!(sog.section_name(), Some("grid"));

        let eog = Directive::name_only("eog");
        assert!(eog.is_section_end());
        assert_eq!(eog.section_name(), Some("grid"));
    }

    // -- Font, size, and color directives -----------------------------------

    #[test]
    fn directive_kind_from_name_title_font_size_color() {
        assert_eq!(
            DirectiveKind::from_name("titlefont"),
            DirectiveKind::TitleFont
        );
        assert_eq!(
            DirectiveKind::from_name("TITLEFONT"),
            DirectiveKind::TitleFont
        );
        assert_eq!(
            DirectiveKind::from_name("titlesize"),
            DirectiveKind::TitleSize
        );
        assert_eq!(
            DirectiveKind::from_name("titlecolour"),
            DirectiveKind::TitleColour
        );
        assert_eq!(
            DirectiveKind::from_name("titlecolor"),
            DirectiveKind::TitleColour
        );
    }

    #[test]
    fn directive_kind_from_name_chorus_font_size_color() {
        assert_eq!(
            DirectiveKind::from_name("chorusfont"),
            DirectiveKind::ChorusFont
        );
        assert_eq!(
            DirectiveKind::from_name("chorussize"),
            DirectiveKind::ChorusSize
        );
        assert_eq!(
            DirectiveKind::from_name("choruscolour"),
            DirectiveKind::ChorusColour
        );
        assert_eq!(
            DirectiveKind::from_name("choruscolor"),
            DirectiveKind::ChorusColour
        );
    }

    #[test]
    fn directive_kind_from_name_footer_header_label() {
        assert_eq!(
            DirectiveKind::from_name("footerfont"),
            DirectiveKind::FooterFont
        );
        assert_eq!(
            DirectiveKind::from_name("footersize"),
            DirectiveKind::FooterSize
        );
        assert_eq!(
            DirectiveKind::from_name("footercolour"),
            DirectiveKind::FooterColour
        );
        assert_eq!(
            DirectiveKind::from_name("footercolor"),
            DirectiveKind::FooterColour
        );
        assert_eq!(
            DirectiveKind::from_name("headerfont"),
            DirectiveKind::HeaderFont
        );
        assert_eq!(
            DirectiveKind::from_name("headersize"),
            DirectiveKind::HeaderSize
        );
        assert_eq!(
            DirectiveKind::from_name("headercolour"),
            DirectiveKind::HeaderColour
        );
        assert_eq!(
            DirectiveKind::from_name("headercolor"),
            DirectiveKind::HeaderColour
        );
        assert_eq!(
            DirectiveKind::from_name("labelfont"),
            DirectiveKind::LabelFont
        );
        assert_eq!(
            DirectiveKind::from_name("labelsize"),
            DirectiveKind::LabelSize
        );
        assert_eq!(
            DirectiveKind::from_name("labelcolour"),
            DirectiveKind::LabelColour
        );
        assert_eq!(
            DirectiveKind::from_name("labelcolor"),
            DirectiveKind::LabelColour
        );
    }

    #[test]
    fn directive_kind_from_name_grid_toc() {
        assert_eq!(
            DirectiveKind::from_name("gridfont"),
            DirectiveKind::GridFont
        );
        assert_eq!(
            DirectiveKind::from_name("gridsize"),
            DirectiveKind::GridSize
        );
        assert_eq!(
            DirectiveKind::from_name("gridcolour"),
            DirectiveKind::GridColour
        );
        assert_eq!(
            DirectiveKind::from_name("gridcolor"),
            DirectiveKind::GridColour
        );
        assert_eq!(DirectiveKind::from_name("tocfont"), DirectiveKind::TocFont);
        assert_eq!(DirectiveKind::from_name("tocsize"), DirectiveKind::TocSize);
        assert_eq!(
            DirectiveKind::from_name("toccolour"),
            DirectiveKind::TocColour
        );
        assert_eq!(
            DirectiveKind::from_name("toccolor"),
            DirectiveKind::TocColour
        );
    }

    #[test]
    fn directive_kind_extra_font_size_color_canonical_names() {
        assert_eq!(DirectiveKind::TitleFont.canonical_name(), "titlefont");
        assert_eq!(DirectiveKind::TitleSize.canonical_name(), "titlesize");
        assert_eq!(DirectiveKind::TitleColour.canonical_name(), "titlecolour");
        assert_eq!(DirectiveKind::ChorusFont.canonical_name(), "chorusfont");
        assert_eq!(DirectiveKind::ChorusSize.canonical_name(), "chorussize");
        assert_eq!(DirectiveKind::ChorusColour.canonical_name(), "choruscolour");
        assert_eq!(DirectiveKind::FooterFont.canonical_name(), "footerfont");
        assert_eq!(DirectiveKind::FooterSize.canonical_name(), "footersize");
        assert_eq!(DirectiveKind::FooterColour.canonical_name(), "footercolour");
        assert_eq!(DirectiveKind::HeaderFont.canonical_name(), "headerfont");
        assert_eq!(DirectiveKind::HeaderSize.canonical_name(), "headersize");
        assert_eq!(DirectiveKind::HeaderColour.canonical_name(), "headercolour");
        assert_eq!(DirectiveKind::LabelFont.canonical_name(), "labelfont");
        assert_eq!(DirectiveKind::LabelSize.canonical_name(), "labelsize");
        assert_eq!(DirectiveKind::LabelColour.canonical_name(), "labelcolour");
        assert_eq!(DirectiveKind::GridFont.canonical_name(), "gridfont");
        assert_eq!(DirectiveKind::GridSize.canonical_name(), "gridsize");
        assert_eq!(DirectiveKind::GridColour.canonical_name(), "gridcolour");
        assert_eq!(DirectiveKind::TocFont.canonical_name(), "tocfont");
        assert_eq!(DirectiveKind::TocSize.canonical_name(), "tocsize");
        assert_eq!(DirectiveKind::TocColour.canonical_name(), "toccolour");
    }

    #[test]
    fn directive_kind_extra_font_size_color_category_checks() {
        let font_kinds = [
            DirectiveKind::TitleFont,
            DirectiveKind::TitleSize,
            DirectiveKind::TitleColour,
            DirectiveKind::ChorusFont,
            DirectiveKind::ChorusSize,
            DirectiveKind::ChorusColour,
            DirectiveKind::FooterFont,
            DirectiveKind::FooterSize,
            DirectiveKind::FooterColour,
            DirectiveKind::HeaderFont,
            DirectiveKind::HeaderSize,
            DirectiveKind::HeaderColour,
            DirectiveKind::LabelFont,
            DirectiveKind::LabelSize,
            DirectiveKind::LabelColour,
            DirectiveKind::GridFont,
            DirectiveKind::GridSize,
            DirectiveKind::GridColour,
            DirectiveKind::TocFont,
            DirectiveKind::TocSize,
            DirectiveKind::TocColour,
        ];
        for kind in &font_kinds {
            assert!(
                kind.is_font_size_color(),
                "{kind:?} should be font_size_color"
            );
            assert!(!kind.is_metadata(), "{kind:?} should not be metadata");
            assert!(!kind.is_comment(), "{kind:?} should not be comment");
            assert!(!kind.is_environment(), "{kind:?} should not be environment");
        }
    }

    #[test]
    fn directive_font_size_color_alias_resolution() {
        let d = Directive::with_value("titlefont", "Times");
        assert_eq!(d.name, "titlefont");
        assert_eq!(d.kind, DirectiveKind::TitleFont);
        assert_eq!(d.value.as_deref(), Some("Times"));

        let d = Directive::with_value("choruscolor", "#FF0000");
        assert_eq!(d.name, "choruscolour");
        assert_eq!(d.kind, DirectiveKind::ChorusColour);

        let d = Directive::with_value("titlecolor", "blue");
        assert_eq!(d.name, "titlecolour");
        assert_eq!(d.kind, DirectiveKind::TitleColour);

        let d = Directive::with_value("gridsize", "12");
        assert_eq!(d.name, "gridsize");
        assert_eq!(d.kind, DirectiveKind::GridSize);
        assert_eq!(d.value.as_deref(), Some("12"));
    }
}

#[cfg(test)]
mod delegate_tests {
    use super::*;

    #[test]
    fn directive_kind_from_name_delegate_abc() {
        assert_eq!(
            DirectiveKind::from_name("start_of_abc"),
            DirectiveKind::StartOfAbc
        );
        assert_eq!(
            DirectiveKind::from_name("end_of_abc"),
            DirectiveKind::EndOfAbc
        );
    }

    #[test]
    fn directive_kind_from_name_delegate_ly() {
        assert_eq!(
            DirectiveKind::from_name("start_of_ly"),
            DirectiveKind::StartOfLy
        );
        assert_eq!(
            DirectiveKind::from_name("end_of_ly"),
            DirectiveKind::EndOfLy
        );
    }

    #[test]
    fn directive_kind_from_name_delegate_svg() {
        assert_eq!(
            DirectiveKind::from_name("start_of_svg"),
            DirectiveKind::StartOfSvg
        );
        assert_eq!(
            DirectiveKind::from_name("end_of_svg"),
            DirectiveKind::EndOfSvg
        );
    }

    #[test]
    fn directive_kind_from_name_delegate_textblock() {
        assert_eq!(
            DirectiveKind::from_name("start_of_textblock"),
            DirectiveKind::StartOfTextblock
        );
        assert_eq!(
            DirectiveKind::from_name("end_of_textblock"),
            DirectiveKind::EndOfTextblock
        );
    }

    #[test]
    fn delegate_environments_case_insensitive() {
        assert_eq!(
            DirectiveKind::from_name("START_OF_ABC"),
            DirectiveKind::StartOfAbc
        );
        assert_eq!(
            DirectiveKind::from_name("End_Of_Ly"),
            DirectiveKind::EndOfLy
        );
        assert_eq!(
            DirectiveKind::from_name("START_OF_SVG"),
            DirectiveKind::StartOfSvg
        );
        assert_eq!(
            DirectiveKind::from_name("End_Of_Textblock"),
            DirectiveKind::EndOfTextblock
        );
    }

    #[test]
    fn delegate_environments_are_section_start() {
        assert!(DirectiveKind::StartOfAbc.is_section_start());
        assert!(DirectiveKind::StartOfLy.is_section_start());
        assert!(DirectiveKind::StartOfSvg.is_section_start());
        assert!(DirectiveKind::StartOfTextblock.is_section_start());
    }

    #[test]
    fn delegate_environments_are_section_end() {
        assert!(DirectiveKind::EndOfAbc.is_section_end());
        assert!(DirectiveKind::EndOfLy.is_section_end());
        assert!(DirectiveKind::EndOfSvg.is_section_end());
        assert!(DirectiveKind::EndOfTextblock.is_section_end());
    }

    #[test]
    fn delegate_environments_are_environments() {
        assert!(DirectiveKind::StartOfAbc.is_environment());
        assert!(DirectiveKind::EndOfAbc.is_environment());
        assert!(DirectiveKind::StartOfLy.is_environment());
        assert!(DirectiveKind::EndOfLy.is_environment());
        assert!(DirectiveKind::StartOfSvg.is_environment());
        assert!(DirectiveKind::EndOfSvg.is_environment());
        assert!(DirectiveKind::StartOfTextblock.is_environment());
        assert!(DirectiveKind::EndOfTextblock.is_environment());
    }

    #[test]
    fn delegate_environments_canonical_names() {
        assert_eq!(DirectiveKind::StartOfAbc.canonical_name(), "start_of_abc");
        assert_eq!(DirectiveKind::EndOfAbc.canonical_name(), "end_of_abc");
        assert_eq!(DirectiveKind::StartOfLy.canonical_name(), "start_of_ly");
        assert_eq!(DirectiveKind::EndOfLy.canonical_name(), "end_of_ly");
        assert_eq!(DirectiveKind::StartOfSvg.canonical_name(), "start_of_svg");
        assert_eq!(DirectiveKind::EndOfSvg.canonical_name(), "end_of_svg");
        assert_eq!(
            DirectiveKind::StartOfTextblock.canonical_name(),
            "start_of_textblock"
        );
        assert_eq!(
            DirectiveKind::EndOfTextblock.canonical_name(),
            "end_of_textblock"
        );
    }

    #[test]
    fn delegate_not_metadata() {
        assert!(!DirectiveKind::StartOfAbc.is_metadata());
        assert!(!DirectiveKind::EndOfLy.is_metadata());
        assert!(!DirectiveKind::StartOfSvg.is_metadata());
        assert!(!DirectiveKind::EndOfTextblock.is_metadata());
    }

    #[test]
    fn delegate_not_comment() {
        assert!(!DirectiveKind::StartOfAbc.is_comment());
        assert!(!DirectiveKind::EndOfLy.is_comment());
    }

    #[test]
    fn delegate_directive_section_name() {
        let d = Directive::name_only("start_of_abc");
        assert_eq!(d.section_name(), Some("abc"));

        let d = Directive::name_only("end_of_ly");
        assert_eq!(d.section_name(), Some("ly"));

        let d = Directive::name_only("start_of_svg");
        assert_eq!(d.section_name(), Some("svg"));

        let d = Directive::name_only("end_of_textblock");
        assert_eq!(d.section_name(), Some("textblock"));
    }

    #[test]
    fn delegate_directive_with_label() {
        let d = Directive::with_value("start_of_abc", "Melody");
        assert_eq!(d.name, "start_of_abc");
        assert_eq!(d.value.as_deref(), Some("Melody"));
        assert_eq!(d.kind, DirectiveKind::StartOfAbc);
    }

    #[test]
    fn delegate_sections_not_custom() {
        // Delegate section names must NOT produce StartOfSection variants
        assert!(!matches!(
            DirectiveKind::from_name("start_of_abc"),
            DirectiveKind::StartOfSection(_)
        ));
        assert!(!matches!(
            DirectiveKind::from_name("start_of_ly"),
            DirectiveKind::StartOfSection(_)
        ));
        assert!(!matches!(
            DirectiveKind::from_name("start_of_svg"),
            DirectiveKind::StartOfSection(_)
        ));
        assert!(!matches!(
            DirectiveKind::from_name("start_of_textblock"),
            DirectiveKind::StartOfSection(_)
        ));
    }
}

#[cfg(test)]
mod chord_definition_tests {
    use super::*;

    #[test]
    fn test_parse_keyboard_definition() {
        let def = ChordDefinition::parse_value("Am keys 0 3 7");
        assert_eq!(def.name, "Am");
        assert_eq!(def.keys, Some(vec![0, 3, 7]));
        assert!(def.copy.is_none());
    }

    #[test]
    fn test_parse_keyboard_empty_keys() {
        // {define: Am keys} with no values produces None (no valid keys).
        let def = ChordDefinition::parse_value("Am keys");
        assert_eq!(def.name, "Am");
        assert_eq!(def.keys, None);
    }

    #[test]
    fn test_parse_keyboard_keys_midi_range() {
        // Values within MIDI range (0-127) are accepted.
        let def = ChordDefinition::parse_value("Am keys 0 60 127");
        assert_eq!(def.keys, Some(vec![0, 60, 127]));
    }

    #[test]
    fn test_parse_keyboard_keys_out_of_range_dropped() {
        // Values outside 0-127 are silently dropped.
        let def = ChordDefinition::parse_value("Am keys -1 0 128 60");
        assert_eq!(def.keys, Some(vec![0, 60]));
    }

    #[test]
    fn test_parse_keyboard_keys_all_invalid() {
        // All values invalid -> None (not Some(vec![])).
        let def = ChordDefinition::parse_value("Am keys abc def");
        assert_eq!(def.keys, None);
    }

    #[test]
    fn test_parse_keyboard_keys_non_numeric_dropped() {
        // Non-numeric tokens are silently dropped.
        let def = ChordDefinition::parse_value("Am keys 0 abc 7 xyz 12");
        assert_eq!(def.keys, Some(vec![0, 7, 12]));
    }

    #[test]
    fn test_parse_copy() {
        let def = ChordDefinition::parse_value("Am copy Amin");
        assert_eq!(def.name, "Am");
        assert_eq!(def.copy, Some("Amin".to_string()));
        assert!(def.keys.is_none());
    }

    #[test]
    fn test_parse_copyall() {
        let def = ChordDefinition::parse_value("Am copyall Amin");
        assert_eq!(def.name, "Am");
        assert_eq!(def.copyall, Some("Amin".to_string()));
    }

    #[test]
    fn test_parse_copy_first_token_only() {
        // Only the first token after "copy" is the source name (#607).
        let def = ChordDefinition::parse_value("Am copy Amin extra stuff");
        assert_eq!(def.copy, Some("Amin".to_string()));
    }

    #[test]
    fn test_parse_copyall_first_token_only() {
        let def = ChordDefinition::parse_value("Am copyall Amin extra stuff");
        assert_eq!(def.copyall, Some("Amin".to_string()));
    }

    #[test]
    fn test_parse_copy_with_display() {
        // display= should be extracted even on copy definitions (#601).
        let def = ChordDefinition::parse_value("Am copy Bm display=\"Alt\"");
        assert_eq!(def.copy, Some("Bm".to_string()));
        assert_eq!(def.display, Some("Alt".to_string()));
    }

    #[test]
    fn test_parse_copyall_with_display() {
        let def = ChordDefinition::parse_value("Am copyall Bm display=\"Alt\"");
        assert_eq!(def.copyall, Some("Bm".to_string()));
        assert_eq!(def.display, Some("Alt".to_string()));
    }

    #[test]
    fn test_parse_copy_with_format() {
        let def = ChordDefinition::parse_value("Am copy Bm format=\"%{root}m\"");
        assert_eq!(def.copy, Some("Bm".to_string()));
        assert_eq!(def.format, Some("%{root}m".to_string()));
    }

    #[test]
    fn test_parse_keys_with_display() {
        // display= should be extracted even on keys definitions (#601).
        let def = ChordDefinition::parse_value("Am keys 0 3 7 display=\"A minor\"");
        assert_eq!(def.keys, Some(vec![0, 3, 7]));
        assert_eq!(def.display, Some("A minor".to_string()));
    }

    #[test]
    fn test_parse_keys_with_format() {
        let def = ChordDefinition::parse_value("Am keys 0 3 7 format=\"%{root}m\"");
        assert_eq!(def.keys, Some(vec![0, 3, 7]));
        assert_eq!(def.format, Some("%{root}m".to_string()));
    }

    #[test]
    fn test_parse_fretted_definition() {
        let def = ChordDefinition::parse_value("Am base-fret 1 frets x 0 2 2 1 0");
        assert_eq!(def.name, "Am");
        assert!(def.raw.is_some());
        assert!(def.raw.unwrap().contains("base-fret"));
    }

    #[test]
    fn test_parse_name_only() {
        let def = ChordDefinition::parse_value("Am");
        assert_eq!(def.name, "Am");
        assert!(def.keys.is_none());
        assert!(def.copy.is_none());
        assert!(def.raw.is_none());
    }

    #[test]
    fn test_parse_display_attribute() {
        let def =
            ChordDefinition::parse_value("Am base-fret 1 frets x 0 2 2 1 0 display=\"A minor\"");
        assert_eq!(def.name, "Am");
        assert_eq!(def.display, Some("A minor".to_string()));
        // display= should be stripped from raw
        let raw = def.raw.unwrap();
        assert!(
            !raw.contains("display="),
            "display= should be stripped from raw, got: {raw}"
        );
        assert!(raw.contains("base-fret"));
    }

    #[test]
    fn test_parse_display_attribute_at_start() {
        let def =
            ChordDefinition::parse_value("Am display=\"A minor\" base-fret 1 frets x 0 2 2 1 0");
        assert_eq!(def.display, Some("A minor".to_string()));
        let raw = def.raw.unwrap();
        assert!(!raw.contains("display="));
        assert!(raw.contains("base-fret"));
    }

    #[test]
    fn test_parse_display_attribute_middle() {
        let def =
            ChordDefinition::parse_value("Am base-fret 1 display=\"A minor\" frets x 0 2 2 1 0");
        assert_eq!(def.display, Some("A minor".to_string()));
        let raw = def.raw.unwrap();
        assert!(!raw.contains("display="));
        assert!(raw.contains("base-fret"));
        assert!(raw.contains("frets"));
    }

    #[test]
    fn test_parse_display_unquoted() {
        let def = ChordDefinition::parse_value("Am base-fret 1 frets x 0 2 2 1 0 display=Aminor");
        assert_eq!(def.display, Some("Aminor".to_string()));
        let raw = def.raw.unwrap();
        assert!(!raw.contains("display="));
    }

    #[test]
    fn test_parse_display_no_false_match() {
        // "undisplay=" should not match as "display="
        let def = ChordDefinition::parse_value("Am undisplay=foo base-fret 1 frets x 0 2 2 1 0");
        assert_eq!(def.display, None);
        let raw = def.raw.unwrap();
        assert!(raw.contains("undisplay=foo"));
    }

    #[test]
    fn test_parse_display_only() {
        // display-only definition (no fret data)
        let def = ChordDefinition::parse_value("Am display=\"A minor\"");
        assert_eq!(def.display, Some("A minor".to_string()));
        assert!(def.raw.is_none());
    }

    #[test]
    fn test_parse_format_attribute() {
        let def = ChordDefinition::parse_value(
            "Am base-fret 1 frets x 0 2 2 1 0 format=\"%{root}%{quality}\"",
        );
        assert_eq!(def.format, Some("%{root}%{quality}".to_string()));
        let raw = def.raw.unwrap();
        assert!(!raw.contains("format="));
        assert!(raw.contains("base-fret"));
    }

    #[test]
    fn test_parse_both_display_and_format() {
        let def = ChordDefinition::parse_value(
            "Am display=\"A minor\" format=\"%{root}%{quality}\" base-fret 1 frets x 0 2 2 1 0",
        );
        assert_eq!(def.display, Some("A minor".to_string()));
        assert_eq!(def.format, Some("%{root}%{quality}".to_string()));
        let raw = def.raw.unwrap();
        assert!(!raw.contains("display="));
        assert!(!raw.contains("format="));
    }

    #[test]
    fn test_parse_format_only() {
        let def = ChordDefinition::parse_value("Am format=\"%{root}-%{quality}\"");
        assert_eq!(def.format, Some("%{root}-%{quality}".to_string()));
        assert!(def.raw.is_none());
    }

    #[test]
    fn test_parse_format_unclosed_quote_no_panic() {
        // Malformed input: missing closing quote must not panic.
        let def = ChordDefinition::parse_value("Am display=\"unclosed");
        assert_eq!(def.display, Some("unclosed".to_string()));
        assert!(def.raw.is_none());
    }

    #[test]
    fn test_parse_format_unclosed_quote_format_attr() {
        let def = ChordDefinition::parse_value("Am format=\"%{root}%{quality}");
        assert_eq!(def.format, Some("%{root}%{quality}".to_string()));
    }

    #[test]
    fn test_parse_keyboard_negative_keys_dropped() {
        // Negative values are outside MIDI range (0-127) and are dropped.
        let def = ChordDefinition::parse_value("Cm keys -1 0 3 7");
        assert_eq!(def.keys, Some(vec![0, 3, 7]));
    }

    // -- ImageAttributes ----------------------------------------------------

    #[test]
    fn has_src_returns_true_for_non_empty() {
        let attrs = ImageAttributes::new("photo.jpg");
        assert!(attrs.has_src());
    }

    #[test]
    fn has_src_returns_false_for_empty() {
        let attrs = ImageAttributes::default();
        assert!(!attrs.has_src());
    }

    #[test]
    fn has_src_returns_false_for_explicit_empty_string() {
        let attrs = ImageAttributes::new("");
        assert!(!attrs.has_src());
    }
}

#[cfg(test)]
mod apply_define_displays_tests {
    use super::*;

    fn make_song_with_define_and_chords(define_value: &str, chord_names: &[&str]) -> Song {
        let mut song = Song::new();

        // Add {define} directive
        song.lines.push(Line::Directive(Directive::with_value(
            "define",
            define_value,
        )));

        // Add a lyrics line with the given chords
        let mut lyrics = LyricsLine::new();
        for name in chord_names {
            lyrics
                .segments
                .push(LyricsSegment::new(Some(Chord::new(*name)), "word "));
        }
        song.lines.push(Line::Lyrics(lyrics));

        song
    }

    #[test]
    fn applies_display_to_matching_chords() {
        let mut song = make_song_with_define_and_chords(
            "Am base-fret 1 frets x 0 2 2 1 0 display=\"A minor\"",
            &["Am", "G", "Am"],
        );
        song.apply_define_displays();

        if let Line::Lyrics(ref lyrics) = song.lines[1] {
            assert_eq!(
                lyrics.segments[0].chord.as_ref().unwrap().display_name(),
                "A minor"
            );
            assert_eq!(
                lyrics.segments[1].chord.as_ref().unwrap().display_name(),
                "G"
            );
            assert_eq!(
                lyrics.segments[2].chord.as_ref().unwrap().display_name(),
                "A minor"
            );
        } else {
            panic!("expected lyrics line");
        }
    }

    #[test]
    fn no_display_when_not_defined() {
        let mut song =
            make_song_with_define_and_chords("Am base-fret 1 frets x 0 2 2 1 0", &["Am"]);
        song.apply_define_displays();

        if let Line::Lyrics(ref lyrics) = song.lines[1] {
            assert_eq!(lyrics.segments[0].chord.as_ref().unwrap().display, None);
        } else {
            panic!("expected lyrics line");
        }
    }

    #[test]
    fn later_define_overrides_earlier() {
        let mut song = Song::new();
        song.lines.push(Line::Directive(Directive::with_value(
            "define",
            "Am display=\"first\"",
        )));
        song.lines.push(Line::Directive(Directive::with_value(
            "define",
            "Am display=\"second\"",
        )));
        let mut lyrics = LyricsLine::new();
        lyrics
            .segments
            .push(LyricsSegment::new(Some(Chord::new("Am")), "text"));
        song.lines.push(Line::Lyrics(lyrics));

        song.apply_define_displays();

        if let Line::Lyrics(ref lyrics) = song.lines[2] {
            assert_eq!(
                lyrics.segments[0].chord.as_ref().unwrap().display_name(),
                "second"
            );
        } else {
            panic!("expected lyrics line");
        }
    }

    #[test]
    fn does_not_overwrite_existing_display() {
        let mut song = Song::new();
        song.lines.push(Line::Directive(Directive::with_value(
            "define",
            "Am display=\"from define\"",
        )));
        let mut lyrics = LyricsLine::new();
        let mut chord = Chord::new("Am");
        chord.display = Some("already set".to_string());
        lyrics
            .segments
            .push(LyricsSegment::new(Some(chord), "text"));
        song.lines.push(Line::Lyrics(lyrics));

        song.apply_define_displays();

        if let Line::Lyrics(ref lyrics) = song.lines[1] {
            assert_eq!(
                lyrics.segments[0].chord.as_ref().unwrap().display_name(),
                "already set"
            );
        } else {
            panic!("expected lyrics line");
        }
    }

    #[test]
    fn format_expands_chord_components() {
        let mut song =
            make_song_with_define_and_chords("Am format=\"%{root} %{quality}\"", &["Am"]);
        song.apply_define_displays();

        if let Line::Lyrics(ref lyrics) = song.lines[1] {
            assert_eq!(
                lyrics.segments[0].chord.as_ref().unwrap().display_name(),
                "A m"
            );
        } else {
            panic!("expected lyrics line");
        }
    }

    #[test]
    fn format_with_extension() {
        let mut song =
            make_song_with_define_and_chords("Am7 format=\"%{root}%{quality}%{ext}\"", &["Am7"]);
        song.apply_define_displays();

        if let Line::Lyrics(ref lyrics) = song.lines[1] {
            assert_eq!(
                lyrics.segments[0].chord.as_ref().unwrap().display_name(),
                "Am7"
            );
        } else {
            panic!("expected lyrics line");
        }
    }

    #[test]
    fn format_with_bass_note() {
        let mut song = make_song_with_define_and_chords("G/B format=\"%{root}/%{bass}\"", &["G/B"]);
        song.apply_define_displays();

        if let Line::Lyrics(ref lyrics) = song.lines[1] {
            assert_eq!(
                lyrics.segments[0].chord.as_ref().unwrap().display_name(),
                "G/B"
            );
        } else {
            panic!("expected lyrics line");
        }
    }

    #[test]
    fn display_takes_precedence_over_format() {
        let mut song = make_song_with_define_and_chords(
            "Am display=\"A minor\" format=\"%{root}%{quality}\"",
            &["Am"],
        );
        song.apply_define_displays();

        if let Line::Lyrics(ref lyrics) = song.lines[1] {
            assert_eq!(
                lyrics.segments[0].chord.as_ref().unwrap().display_name(),
                "A minor"
            );
        } else {
            panic!("expected lyrics line");
        }
    }
}

#[cfg(test)]
mod expand_format_tests {
    use super::*;

    #[test]
    fn basic_root_quality() {
        let chord = Chord::new("Am");
        assert_eq!(
            chord.expand_format("%{root}%{quality}"),
            Some("Am".to_string())
        );
    }

    #[test]
    fn with_accidental() {
        let chord = Chord::new("Bb");
        assert_eq!(chord.expand_format("%{root}"), Some("Bb".to_string()));
    }

    #[test]
    fn with_extension() {
        let chord = Chord::new("Cmaj7");
        let result = chord.expand_format("%{root}%{quality}%{ext}");
        assert_eq!(result, Some("Cmaj7".to_string()));
    }

    #[test]
    fn with_bass() {
        let chord = Chord::new("Am/G");
        let result = chord.expand_format("%{root}%{quality}/%{bass}");
        assert_eq!(result, Some("Am/G".to_string()));
    }

    #[test]
    fn custom_format() {
        let chord = Chord::new("Am");
        let result = chord.expand_format("[%{root} minor]");
        assert_eq!(result, Some("[A minor]".to_string()));
    }

    #[test]
    fn returns_none_for_unparsed_chord() {
        let chord = Chord {
            name: "???".to_string(),
            detail: None,
            display: None,
        };
        assert_eq!(chord.expand_format("%{root}"), None);
    }

    #[test]
    fn unknown_placeholder_passes_through() {
        let chord = Chord::new("Am");
        let result = chord.expand_format("%{root}%{unknown}");
        assert_eq!(result, Some("A%{unknown}".to_string()));
    }

    #[test]
    fn empty_format_string() {
        let chord = Chord::new("Am");
        assert_eq!(chord.expand_format(""), Some(String::new()));
    }

    #[test]
    fn slash_chord_bass_with_accidental() {
        let chord = Chord::new("G/Bb");
        let result = chord.expand_format("%{root}/%{bass}");
        assert_eq!(result, Some("G/Bb".to_string()));
    }

    #[test]
    fn no_bass_produces_empty_string() {
        let chord = Chord::new("Am");
        let result = chord.expand_format("%{root}%{quality} (bass: %{bass})");
        assert_eq!(result, Some("Am (bass: )".to_string()));
    }

    #[test]
    fn all_placeholders_combined() {
        let chord = Chord::new("Bbm7/Eb");
        let result = chord.expand_format("%{root}%{quality}%{ext}/%{bass}");
        assert_eq!(result, Some("Bbm7/Eb".to_string()));
    }

    #[test]
    fn literal_text_with_no_placeholders() {
        let chord = Chord::new("Am");
        let result = chord.expand_format("just text");
        assert_eq!(result, Some("just text".to_string()));
    }
}
