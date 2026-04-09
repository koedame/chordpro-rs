//! LSP completion support for ChordPro files.
//!
//! Completion items are built from static, compile-time data — no external
//! files or network access. The three completion contexts are:
//!
//! - **Directive name**: inside `{...}` before or at the `:`
//! - **Chord name**: inside `[...]`
//! - **Metadata key**: inside `{meta: ...}` after the space
//!
//! Context detection works by scanning the line text up to the cursor column
//! and finding the innermost open delimiter.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind,
};

// ---------------------------------------------------------------------------
// Completion context detection
// ---------------------------------------------------------------------------

/// The kind of completion triggered at a given cursor position.
#[derive(Debug, PartialEq)]
pub enum CompletionContext {
    /// Inside `{...}` before or at the colon: complete directive names.
    DirectiveName {
        /// Characters typed so far (lowercased prefix).
        prefix: String,
    },
    /// After `{meta: `: complete known metadata keys.
    MetadataKey { prefix: String },
    /// Inside `[...]`: complete chord names.
    ChordName { prefix: String },
    /// Not inside a recognized completion context.
    None,
}

/// Detects the completion context for `line` at 0-based `col`.
///
/// Scans backwards from `col` to find the innermost open delimiter.
/// Returns [`CompletionContext::None`] if the cursor is not inside a
/// recognized completion region.
#[must_use]
pub fn detect_context(line: &str, col: usize) -> CompletionContext {
    // Work with char boundaries; col is a 0-based character offset.
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len());
    let prefix_chars = &chars[..col];

    // Track the innermost open delimiter scanning left from the cursor.
    let mut in_bracket = false; // inside [
    let mut in_brace = false; // inside {
    let mut brace_colon_pos: Option<usize> = None; // position of `:` inside `{`
    let mut directive_name = String::new();

    for (i, &ch) in prefix_chars.iter().enumerate() {
        match ch {
            '{' => {
                in_brace = true;
                in_bracket = false;
                brace_colon_pos = None;
                directive_name.clear();
            }
            '}' => {
                in_brace = false;
                brace_colon_pos = None;
                directive_name.clear();
            }
            '[' => {
                in_bracket = true;
                in_brace = false;
            }
            ']' => {
                in_bracket = false;
            }
            ':' if in_brace => {
                brace_colon_pos = Some(i);
                // Extract the directive name as text between the most-recent
                // `{` and the colon, ignoring any text before the brace.
                let brace_start = chars[..i]
                    .iter()
                    .rposition(|&c| c == '{')
                    .map(|p| p + 1)
                    .unwrap_or(0);
                directive_name = chars[brace_start..i]
                    .iter()
                    .collect::<String>()
                    .trim()
                    .to_ascii_lowercase();
            }
            _ => {}
        }
    }

    if in_bracket {
        // Collect text after `[` up to cursor.
        // in_bracket is true only after a `[` was scanned — rposition always finds it.
        let open_bracket = prefix_chars
            .iter()
            .rposition(|&c| c == '[')
            .expect("invariant: in_bracket requires a scanned `[`");
        let prefix: String = chars[open_bracket + 1..col].iter().collect();
        return CompletionContext::ChordName { prefix };
    }

    if in_brace {
        if let Some(colon_pos) = brace_colon_pos {
            let prefix: String = chars[colon_pos + 1..col]
                .iter()
                .skip_while(|&&c| c == ' ')
                .collect::<String>()
                .trim_start()
                .to_ascii_lowercase();
            if directive_name == "meta" {
                return CompletionContext::MetadataKey { prefix };
            }
            // Other directive values — no completion implemented yet.
            return CompletionContext::None;
        }
        // Before the colon: completing the directive name.
        // in_brace is true only after a `{` was scanned — rposition always finds it.
        let open_brace = prefix_chars
            .iter()
            .rposition(|&c| c == '{')
            .expect("invariant: in_brace requires a scanned `{`");
        let prefix: String = chars[open_brace + 1..col].iter().collect();
        let prefix = prefix.trim().to_ascii_lowercase();
        return CompletionContext::DirectiveName { prefix };
    }

    CompletionContext::None
}

// ---------------------------------------------------------------------------
// Static completion data
// ---------------------------------------------------------------------------

/// All canonical ChordPro directive names with optional alias info.
///
/// Each entry is `(canonical_name, alias_note)` where `alias_note` is
/// `Some("alias: soc")` for directives that have short aliases.
const DIRECTIVES: &[(&str, Option<&str>)] = &[
    // Metadata
    ("title", Some("alias: t")),
    ("subtitle", Some("alias: st")),
    ("artist", None),
    ("composer", None),
    ("lyricist", None),
    ("album", None),
    ("year", None),
    ("key", None),
    ("tempo", None),
    ("time", None),
    ("capo", None),
    ("sorttitle", None),
    ("sortartist", None),
    ("arranger", None),
    ("copyright", None),
    ("duration", None),
    ("tag", None),
    // Transpose
    ("transpose", None),
    // Song boundary
    ("new_song", Some("alias: ns")),
    // Formatting / comment
    ("comment", Some("alias: c")),
    ("comment_italic", Some("alias: ci")),
    ("comment_box", Some("alias: cb")),
    // Environments
    ("start_of_chorus", Some("alias: soc")),
    ("end_of_chorus", Some("alias: eoc")),
    ("start_of_verse", Some("alias: sov")),
    ("end_of_verse", Some("alias: eov")),
    ("start_of_bridge", Some("alias: sob")),
    ("end_of_bridge", Some("alias: eob")),
    ("start_of_tab", Some("alias: sot")),
    ("end_of_tab", Some("alias: eot")),
    ("start_of_grid", Some("alias: sog")),
    ("end_of_grid", Some("alias: eog")),
    ("start_of_abc", None),
    ("end_of_abc", None),
    ("start_of_ly", None),
    ("end_of_ly", None),
    ("start_of_svg", None),
    ("end_of_svg", None),
    ("start_of_textblock", None),
    ("end_of_textblock", None),
    // Recall
    ("chorus", None),
    // Page control
    ("new_page", Some("alias: np")),
    ("new_physical_page", Some("alias: npp")),
    ("column_break", Some("alias: colb")),
    ("columns", Some("alias: col")),
    // Font/size/color (inline)
    ("textfont", Some("alias: tf")),
    ("textsize", Some("alias: ts")),
    ("textcolour", Some("alias: tc")),
    ("chordfont", Some("alias: cf")),
    ("chordsize", Some("alias: cs")),
    ("chordcolour", Some("alias: cc")),
    ("tabfont", None),
    ("tabsize", None),
    ("tabcolour", None),
    // Font/size/color (title-level)
    ("titlefont", None),
    ("titlesize", None),
    ("titlecolour", None),
    ("chorusfont", None),
    ("chorussize", None),
    ("choruscolour", None),
    ("footerfont", None),
    ("footersize", None),
    ("footercolour", None),
    ("headerfont", None),
    ("headersize", None),
    ("headercolour", None),
    ("labelfont", None),
    ("labelsize", None),
    ("labelcolour", None),
    ("gridfont", None),
    ("gridsize", None),
    ("gridcolour", None),
    ("tocfont", None),
    ("tocsize", None),
    ("toccolour", None),
    // Chord definitions
    ("define", None),
    ("chord", None),
    ("diagrams", None),
    // Generic metadata
    ("meta", None),
    // Image
    ("image", None),
];

/// Common `{meta: KEY}` keys.
const META_KEYS: &[&str] = &[
    "album",
    "arranger",
    "artist",
    "capo",
    "composer",
    "copyright",
    "duration",
    "instrument",
    "key",
    "lyricist",
    "sorttitle",
    "sortartist",
    "subtitle",
    "tag",
    "tempo",
    "time",
    "title",
    "year",
];

/// Note roots.
const ROOTS: &[&str] = &[
    "A", "A#", "Ab", "B", "Bb", "C", "C#", "D", "D#", "Db", "E", "Eb", "F", "F#", "G", "G#", "Gb",
];

/// Chord quality suffixes.
const SUFFIXES: &[&str] = &[
    "", "m", "7", "m7", "maj7", "M7", "dim", "dim7", "aug", "sus2", "sus4", "add9", "9", "m9",
    "maj9", "11", "13", "6", "m6", "5",
];

// ---------------------------------------------------------------------------
// Item builders
// ---------------------------------------------------------------------------

/// Returns directive name completion items matching `prefix`.
#[must_use]
pub fn directive_items(prefix: &str) -> Vec<CompletionItem> {
    DIRECTIVES
        .iter()
        .filter(|(name, _)| name.starts_with(prefix))
        .map(|(name, alias)| {
            let detail = alias.map(ToString::to_string);
            CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail,
                ..Default::default()
            }
        })
        .collect()
}

/// Returns metadata key completion items matching `prefix`.
#[must_use]
pub fn meta_key_items(prefix: &str) -> Vec<CompletionItem> {
    META_KEYS
        .iter()
        .filter(|k| k.starts_with(prefix))
        .map(|k| CompletionItem {
            label: k.to_string(),
            kind: Some(CompletionItemKind::FIELD),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::PlainText,
                value: format!("Metadata key: {k}"),
            })),
            ..Default::default()
        })
        .collect()
}

/// Returns chord name completion items matching `prefix`.
///
/// Only root+suffix combinations whose label starts with `prefix` are
/// returned, keeping the list small for common cases (e.g. `"C"` matches
/// `C`, `Cm`, `C7`, etc.).
#[must_use]
pub fn chord_items(prefix: &str) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    for root in ROOTS {
        for suffix in SUFFIXES {
            let label = format!("{root}{suffix}");
            if label.starts_with(prefix) {
                items.push(CompletionItem {
                    label,
                    kind: Some(CompletionItemKind::VALUE),
                    ..Default::default()
                });
            }
        }
        // Bass-note slash chords (e.g. C/G): generated only when `prefix`
        // starts with the root string, so they are suppressed for an empty
        // prefix (preventing a combinatorial explosion of root×bass items).
        if !prefix.is_empty() && prefix.starts_with(root) {
            for bass in ROOTS {
                let label = format!("{root}/{bass}");
                if label.starts_with(prefix) {
                    items.push(CompletionItem {
                        label,
                        kind: Some(CompletionItemKind::VALUE),
                        ..Default::default()
                    });
                }
            }
        }
    }
    items
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- context detection ---

    #[test]
    fn context_directive_name_empty_prefix() {
        let ctx = detect_context("{", 1);
        assert_eq!(
            ctx,
            CompletionContext::DirectiveName {
                prefix: String::new()
            }
        );
    }

    #[test]
    fn context_directive_name_with_prefix() {
        let ctx = detect_context("{tit", 4);
        assert_eq!(
            ctx,
            CompletionContext::DirectiveName {
                prefix: "tit".to_string()
            }
        );
    }

    #[test]
    fn context_chord_name_empty() {
        let ctx = detect_context("[", 1);
        assert_eq!(
            ctx,
            CompletionContext::ChordName {
                prefix: String::new()
            }
        );
    }

    #[test]
    fn context_chord_name_with_prefix() {
        let ctx = detect_context("[Am", 3);
        assert_eq!(
            ctx,
            CompletionContext::ChordName {
                prefix: "Am".to_string()
            }
        );
    }

    #[test]
    fn context_metadata_key() {
        let ctx = detect_context("{meta: ", 7);
        assert_eq!(
            ctx,
            CompletionContext::MetadataKey {
                prefix: String::new()
            }
        );
    }

    #[test]
    fn context_metadata_key_with_prefix() {
        let ctx = detect_context("{meta: art", 10);
        assert_eq!(
            ctx,
            CompletionContext::MetadataKey {
                prefix: "art".to_string()
            }
        );
    }

    #[test]
    fn context_outside_any_delimiter_is_none() {
        let ctx = detect_context("Hello world", 5);
        assert_eq!(ctx, CompletionContext::None);
    }

    #[test]
    fn context_after_closed_brace_is_none() {
        let ctx = detect_context("{title: My Song} ", 17);
        assert_eq!(ctx, CompletionContext::None);
    }

    // --- item builders ---

    #[test]
    fn directive_items_filters_by_prefix() {
        let items = directive_items("start_of_");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"start_of_chorus"));
        assert!(labels.contains(&"start_of_verse"));
        assert!(!labels.contains(&"end_of_chorus"));
    }

    #[test]
    fn directive_items_empty_prefix_returns_all() {
        let items = directive_items("");
        assert!(
            items.len() >= 40,
            "expected many directives, got {}",
            items.len()
        );
    }

    #[test]
    fn chord_items_filters_by_root() {
        let items = chord_items("C");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"C"));
        assert!(labels.contains(&"Cm"));
        assert!(labels.contains(&"C7"));
        // C# / Cb items are also included (they start with "C").
        // Unrelated roots (D, G, …) must not appear.
        assert!(
            !labels
                .iter()
                .any(|l| l.starts_with('D') || l.starts_with('G'))
        );
    }

    #[test]
    fn meta_key_items_filters_by_prefix() {
        let items = meta_key_items("art");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "artist");
    }
}
