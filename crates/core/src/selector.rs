//! Selector resolution for conditional directives.
//!
//! ChordPro directives can have a selector suffix (e.g., `{textfont-piano: Courier}`)
//! that targets a specific instrument or user. The selector resolution logic
//! determines whether a directive should be applied based on the current
//! rendering context.
//!
//! # Examples
//!
//! ```
//! use chordpro_core::selector::SelectorContext;
//!
//! let ctx = SelectorContext::new(Some("guitar"), None);
//! assert!(ctx.matches(None));           // No selector = always matches
//! assert!(ctx.matches(Some("guitar"))); // Matches active instrument
//! assert!(!ctx.matches(Some("piano"))); // Different instrument
//! ```

/// Context for resolving directive selectors.
///
/// Holds the active instrument type and user name from configuration.
/// Directives with selectors are tested against this context to decide
/// whether they should be applied.
#[derive(Debug, Clone, Default)]
pub struct SelectorContext {
    /// The active instrument type (e.g., `"guitar"`, `"ukulele"`).
    pub instrument: Option<String>,
    /// The active user name (e.g., `"default"`, `"john"`).
    pub user: Option<String>,
}

impl SelectorContext {
    /// Create a new selector context.
    #[must_use]
    pub fn new(instrument: Option<&str>, user: Option<&str>) -> Self {
        Self {
            instrument: instrument.map(|s| s.to_ascii_lowercase()),
            user: user.map(|s| s.to_ascii_lowercase()),
        }
    }

    /// Create a context from a [`Config`](crate::config::Config).
    ///
    /// Reads `instrument.type` and `user.name` from the configuration.
    #[must_use]
    pub fn from_config(config: &crate::config::Config) -> Self {
        let instrument = config
            .get_path("instrument.type")
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.to_ascii_lowercase());
        let user = config
            .get_path("user.name")
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.to_ascii_lowercase());
        Self { instrument, user }
    }

    /// Test whether a directive selector matches this context.
    ///
    /// - `None` (no selector) → always matches
    /// - `Some(selector)` → matches if it equals the active instrument or user
    ///   (case-insensitive comparison)
    ///
    /// Unrecognized selectors (that match neither instrument nor user) are
    /// silently ignored (return `false`).
    #[must_use]
    pub fn matches(&self, selector: Option<&str>) -> bool {
        let Some(sel) = selector else {
            return true; // No selector = unconditional
        };

        // Context values are normalized to lowercase at construction, but
        // selectors may bypass normalization via direct Directive struct
        // construction. `eq_ignore_ascii_case` is used defensively to handle
        // both paths without allocating.
        if let Some(ref instrument) = self.instrument {
            if instrument.eq_ignore_ascii_case(sel) {
                return true;
            }
        }

        if let Some(ref user) = self.user {
            if user.eq_ignore_ascii_case(sel) {
                return true;
            }
        }

        false
    }

    /// Test whether a [`Directive`](crate::ast::Directive)'s selector matches.
    #[must_use]
    pub fn matches_directive(&self, directive: &crate::ast::Directive) -> bool {
        self.matches(directive.selector.as_deref())
    }

    /// Filter a song's lines based on the active selector context.
    ///
    /// - Directives without selectors are always kept.
    /// - Directives with matching selectors are kept.
    /// - Directives with non-matching selectors are removed.
    /// - When a non-matching section-start directive is removed, all lines
    ///   until (and including) its corresponding section-end are also removed.
    ///
    /// After filtering, metadata is re-derived: the base metadata (from
    /// unselectored directives) is augmented with metadata from any
    /// selector-bearing directives that survived filtering.
    #[must_use]
    pub fn filter_song(&self, song: &crate::ast::Song) -> crate::ast::Song {
        let mut filtered_lines = Vec::new();
        // Depth counter for non-matching sections. When > 0, all lines are suppressed.
        let mut suppress_depth: usize = 0;

        for line in &song.lines {
            match line {
                crate::ast::Line::Directive(d) => {
                    if suppress_depth > 0 {
                        // Inside a suppressed section — track nested section boundaries.
                        if d.kind.is_section_start() {
                            suppress_depth += 1;
                        } else if d.kind.is_section_end() {
                            suppress_depth -= 1;
                        }
                        // All lines inside the suppressed section are dropped.
                        continue;
                    }

                    if !self.matches_directive(d) {
                        // Non-matching directive.
                        if d.kind.is_section_start() {
                            // Begin suppressing all content until the matching end.
                            suppress_depth = 1;
                        }
                        // Either way, this directive is removed.
                        continue;
                    }

                    filtered_lines.push(line.clone());
                }
                _ => {
                    if suppress_depth == 0 {
                        filtered_lines.push(line.clone());
                    }
                }
            }
        }

        // Re-derive metadata: start from the base metadata (unselectored directives
        // were already populated during parsing) and add metadata from any
        // selector-bearing directives that survived filtering.
        let mut metadata = song.metadata.clone();
        for line in &filtered_lines {
            if let crate::ast::Line::Directive(d) = line {
                if d.selector.is_some() {
                    crate::parser::Parser::populate_metadata(&mut metadata, d);
                }
            }
        }

        crate::ast::Song {
            metadata,
            lines: filtered_lines,
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_selector_always_matches() {
        let ctx = SelectorContext::default();
        assert!(ctx.matches(None));
    }

    #[test]
    fn test_instrument_match() {
        let ctx = SelectorContext::new(Some("guitar"), None);
        assert!(ctx.matches(Some("guitar")));
    }

    #[test]
    fn test_instrument_mismatch() {
        let ctx = SelectorContext::new(Some("guitar"), None);
        assert!(!ctx.matches(Some("piano")));
    }

    #[test]
    fn test_instrument_case_insensitive() {
        let ctx = SelectorContext::new(Some("Guitar"), None);
        assert!(ctx.matches(Some("GUITAR")));
        assert!(ctx.matches(Some("guitar")));
    }

    #[test]
    fn test_user_match() {
        let ctx = SelectorContext::new(None, Some("john"));
        assert!(ctx.matches(Some("john")));
    }

    #[test]
    fn test_user_mismatch() {
        let ctx = SelectorContext::new(None, Some("john"));
        assert!(!ctx.matches(Some("alice")));
    }

    #[test]
    fn test_both_instrument_and_user() {
        let ctx = SelectorContext::new(Some("guitar"), Some("john"));
        assert!(ctx.matches(Some("guitar")));
        assert!(ctx.matches(Some("john")));
        assert!(!ctx.matches(Some("piano")));
    }

    #[test]
    fn test_empty_context_rejects_selector() {
        let ctx = SelectorContext::default();
        assert!(!ctx.matches(Some("anything")));
    }

    #[test]
    fn test_from_config() {
        let config = crate::config::Config::parse(
            r#"{"instrument": {"type": "ukulele"}, "user": {"name": "Alice"}}"#,
        )
        .unwrap();
        let ctx = SelectorContext::from_config(&config);
        assert!(ctx.matches(Some("ukulele")));
        assert!(ctx.matches(Some("alice"))); // case-insensitive
        assert!(!ctx.matches(Some("guitar")));
    }

    #[test]
    fn test_from_config_missing_fields() {
        let config = crate::config::Config::empty();
        let ctx = SelectorContext::from_config(&config);
        assert!(ctx.matches(None));
        assert!(!ctx.matches(Some("guitar")));
    }

    #[test]
    fn test_from_config_empty_instrument_treated_as_none() {
        let config = crate::config::Config::parse(r#"{"instrument": {"type": ""}}"#).unwrap();
        let ctx = SelectorContext::from_config(&config);
        assert!(ctx.instrument.is_none(), "empty instrument should be None");
    }

    #[test]
    fn test_from_config_whitespace_instrument_treated_as_none() {
        let config = crate::config::Config::parse(r#"{"instrument": {"type": "  "}}"#).unwrap();
        let ctx = SelectorContext::from_config(&config);
        assert!(
            ctx.instrument.is_none(),
            "whitespace-only instrument should be None"
        );
    }

    #[test]
    fn test_matches_directive() {
        let ctx = SelectorContext::new(Some("guitar"), None);
        let directive = crate::ast::Directive {
            name: "textfont".to_string(),
            value: Some("Courier".to_string()),
            kind: crate::ast::DirectiveKind::TextFont,
            selector: Some("guitar".to_string()),
        };
        assert!(ctx.matches_directive(&directive));
    }

    #[test]
    fn test_matches_directive_no_selector() {
        let ctx = SelectorContext::new(Some("guitar"), None);
        let directive = crate::ast::Directive {
            name: "textfont".to_string(),
            value: Some("Courier".to_string()),
            kind: crate::ast::DirectiveKind::TextFont,
            selector: None,
        };
        assert!(ctx.matches_directive(&directive));
    }

    #[test]
    fn test_matches_directive_mismatch() {
        let ctx = SelectorContext::new(Some("guitar"), None);
        let directive = crate::ast::Directive {
            name: "textfont".to_string(),
            value: Some("Courier".to_string()),
            kind: crate::ast::DirectiveKind::TextFont,
            selector: Some("piano".to_string()),
        };
        assert!(!ctx.matches_directive(&directive));
    }

    // -- edge case tests (#322) ------------------------------------------------

    #[test]
    fn test_empty_string_selector_does_not_match() {
        let ctx = SelectorContext::new(Some("guitar"), Some("john"));
        assert!(!ctx.matches(Some("")), "empty selector should not match");
    }

    #[test]
    fn test_trailing_hyphen_directive_no_selector() {
        // "title-" has an empty suffix after the hyphen — should resolve
        // without a selector since the suffix is empty.
        let (kind, sel) = crate::ast::DirectiveKind::resolve_with_selector("title-");
        // Empty suffix is rejected by the `!suffix.is_empty()` check,
        // so it falls back to Unknown("title-") with no selector.
        assert_eq!(sel, None);
        assert!(matches!(kind, crate::ast::DirectiveKind::Unknown(_)));
    }

    #[test]
    fn test_with_selector_normalizes_to_lowercase() {
        let d = crate::ast::Directive::with_selector("title", Some("Test".into()), "PIANO");
        assert_eq!(d.selector.as_deref(), Some("piano"));
    }

    // -- filter_song tests ----------------------------------------------------

    #[test]
    fn test_filter_song_keeps_matching_directives() {
        let song = crate::parse("{textfont-guitar: Courier}\nLyrics").unwrap();
        let ctx = SelectorContext::new(Some("guitar"), None);
        let filtered = ctx.filter_song(&song);
        // The directive should be kept
        let has_directive = filtered
            .lines
            .iter()
            .any(|l| matches!(l, crate::ast::Line::Directive(_)));
        assert!(has_directive);
    }

    #[test]
    fn test_filter_song_removes_non_matching_directives() {
        let song = crate::parse("{textfont-piano: Courier}\nLyrics").unwrap();
        let ctx = SelectorContext::new(Some("guitar"), None);
        let filtered = ctx.filter_song(&song);
        // The piano directive should be removed
        let has_directive = filtered
            .lines
            .iter()
            .any(|l| matches!(l, crate::ast::Line::Directive(_)));
        assert!(!has_directive);
    }

    #[test]
    fn test_filter_song_keeps_unselectored_directives() {
        let song = crate::parse("{textfont: Courier}\nLyrics").unwrap();
        let ctx = SelectorContext::new(Some("guitar"), None);
        let filtered = ctx.filter_song(&song);
        let has_directive = filtered
            .lines
            .iter()
            .any(|l| matches!(l, crate::ast::Line::Directive(_)));
        assert!(has_directive);
    }

    #[test]
    fn test_filter_song_keeps_lyrics_and_comments() {
        let song = crate::parse("{textfont-piano: Courier}\nLyrics\n{comment: Note}").unwrap();
        let ctx = SelectorContext::new(Some("guitar"), None);
        let filtered = ctx.filter_song(&song);
        let has_lyrics = filtered
            .lines
            .iter()
            .any(|l| matches!(l, crate::ast::Line::Lyrics(_)));
        assert!(has_lyrics);
    }

    #[test]
    fn test_filter_song_mixed_selectors() {
        let input = "{textfont-guitar: Courier}\n{textfont-piano: Times}\n[Am]Hello";
        let song = crate::parse(input).unwrap();
        let ctx = SelectorContext::new(Some("guitar"), None);
        let filtered = ctx.filter_song(&song);
        // Should have guitar directive but not piano
        let directive_count = filtered
            .lines
            .iter()
            .filter(|l| matches!(l, crate::ast::Line::Directive(_)))
            .count();
        assert_eq!(directive_count, 1);
    }

    // -- Section content filtering (#319) ------------------------------------

    #[test]
    fn test_filter_song_removes_section_contents() {
        let input =
            "{start_of_chorus-piano}\n[C]La la la\n{end_of_chorus-piano}\n[Am]Regular lyrics";
        let song = crate::parse(input).unwrap();
        let ctx = SelectorContext::new(Some("guitar"), None);
        let filtered = ctx.filter_song(&song);
        // The piano chorus and its lyrics should be removed.
        let texts: Vec<_> = filtered
            .lines
            .iter()
            .filter_map(|l| match l {
                crate::ast::Line::Lyrics(ly) => Some(ly.text()),
                _ => None,
            })
            .collect();
        assert!(
            !texts.iter().any(|t| t.contains("La la")),
            "piano chorus lyrics should be removed"
        );
        assert!(
            texts.iter().any(|t| t.contains("Regular")),
            "unselectored lyrics should remain"
        );
    }

    #[test]
    fn test_filter_song_keeps_matching_section_contents() {
        let input = "{start_of_chorus-guitar}\n[C]Guitar chorus\n{end_of_chorus-guitar}";
        let song = crate::parse(input).unwrap();
        let ctx = SelectorContext::new(Some("guitar"), None);
        let filtered = ctx.filter_song(&song);
        let has_chorus_lyrics = filtered.lines.iter().any(|l| match l {
            crate::ast::Line::Lyrics(ly) => ly.text().contains("Guitar chorus"),
            _ => false,
        });
        assert!(
            has_chorus_lyrics,
            "matching section contents should be kept"
        );
    }

    // -- Metadata re-derivation (#318) ---------------------------------------

    #[test]
    fn test_metadata_skips_selector_directives_during_parse() {
        let input = "{title: Main Title}\n{title-band: Band Title}";
        let song = crate::parse(input).unwrap();
        // During parsing, only the unselectored title should populate metadata.
        assert_eq!(
            song.metadata.title.as_deref(),
            Some("Main Title"),
            "selector-bearing title should not overwrite metadata during parsing"
        );
    }

    #[test]
    fn test_filter_song_rederives_metadata_from_matching_selector() {
        let input = "{title: Main Title}\n{title-band: Band Title}";
        let song = crate::parse(input).unwrap();
        // When filtering for "band", the band title should be applied.
        let ctx = SelectorContext::new(None, Some("band"));
        let filtered = ctx.filter_song(&song);
        assert_eq!(
            filtered.metadata.title.as_deref(),
            Some("Band Title"),
            "matching selector title should override metadata after filtering"
        );
    }

    #[test]
    fn test_filter_song_keeps_base_metadata_when_no_selector_match() {
        let input = "{title: Main Title}\n{title-band: Band Title}";
        let song = crate::parse(input).unwrap();
        // When filtering for "guitar" (no match for "band"), only the base title remains.
        let ctx = SelectorContext::new(Some("guitar"), None);
        let filtered = ctx.filter_song(&song);
        assert_eq!(
            filtered.metadata.title.as_deref(),
            Some("Main Title"),
            "base metadata should remain when selector doesn't match"
        );
    }
}
