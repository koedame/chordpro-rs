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
///         },
///         LyricsSegment {
///             chord: Some(Chord::new("G")),
///             text: "world".to_string(),
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
    pub text: String,
}

impl LyricsSegment {
    /// Creates a new segment with the given chord and text.
    #[must_use]
    pub fn new(chord: Option<Chord>, text: impl Into<String>) -> Self {
        Self {
            chord,
            text: text.into(),
        }
    }

    /// Creates a text-only segment with no chord.
    #[must_use]
    pub fn text_only(text: impl Into<String>) -> Self {
        Self {
            chord: None,
            text: text.into(),
        }
    }

    /// Creates a chord-only segment with no text.
    #[must_use]
    pub fn chord_only(chord: Chord) -> Self {
        Self {
            chord: Some(chord),
            text: String::new(),
        }
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
        Self { name, detail }
    }
}

impl core::fmt::Display for Chord {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.name)
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

    // -- Chord definition directives ----------------------------------------
    /// `{define}` — defines a custom chord fingering.
    Define,
    /// `{chord}` — references a custom chord.
    ChordDirective,

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

            // Chord definitions
            "define" => Self::Define,
            "chord" => Self::ChordDirective,

            // Generic metadata
            "meta" => Self::Meta(String::new()),

            // Custom sections (start_of_X / end_of_X)
            other => {
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
            Self::Define => "define",
            Self::ChordDirective => "chord",
            Self::Meta(_) => "meta",
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
                | Self::EndOfSection(_)
        )
    }

    /// Returns `true` if this is an environment (section start or end) directive.
    #[must_use]
    pub fn is_environment(&self) -> bool {
        self.is_section_start() || self.is_section_end()
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
///
/// // {start_of_chorus}
/// let d = Directive::name_only("start_of_chorus");
/// assert_eq!(d.name, "start_of_chorus");
/// assert!(d.value.is_none());
/// assert_eq!(d.kind, DirectiveKind::StartOfChorus);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Directive {
    /// The canonical directive name (e.g., `"title"`, `"start_of_chorus"`).
    pub name: String,
    /// The optional value after the colon (e.g., `"My Song"` in `{title: My Song}`).
    pub value: Option<String>,
    /// The classified kind of this directive.
    pub kind: DirectiveKind,
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
}
