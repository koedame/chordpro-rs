//! UniFFI bindings for ChordSketch.
//!
//! Exposes ChordPro parsing and rendering (text, HTML, PDF) to Python,
//! Swift, Kotlin, and Ruby via [UniFFI](https://mozilla.github.io/uniffi-rs/).

use chordsketch_core::render_result::RenderResult;

uniffi::include_scaffolding!("chordsketch");

/// Errors returned by the FFI layer.
#[derive(Debug, thiserror::Error)]
pub enum ChordSketchError {
    /// No songs found in the input.
    #[error("no songs found in input")]
    NoSongsFound,
    /// Invalid configuration string.
    #[error("invalid config: {reason}")]
    InvalidConfig {
        /// Human-readable description of why the config is invalid.
        reason: String,
    },
}

/// Resolve a [`chordsketch_core::config::Config`] from an optional JSON/preset string.
fn resolve_config(
    config_json: Option<String>,
) -> Result<chordsketch_core::config::Config, ChordSketchError> {
    match config_json {
        Some(name) => {
            if let Some(preset) = chordsketch_core::config::Config::preset(&name) {
                Ok(preset)
            } else {
                chordsketch_core::config::Config::parse(&name).map_err(|e| {
                    ChordSketchError::InvalidConfig {
                        reason: format!("not a known preset and not valid RRJSON: {e}"),
                    }
                })
            }
        }
        None => Ok(chordsketch_core::config::Config::defaults()),
    }
}

/// Parse input into songs.
///
/// `parse_multi_lenient` always returns at least one `ParseResult`
/// (`split_at_new_song` unconditionally pushes the trailing segment, even
/// for empty input — see `chordsketch_core::parser`), so the resulting
/// `Vec<Song>` is never empty. The previous `is_empty()` guard was dead
/// code. See #1083.
///
/// The function still returns `Result` (and the [`ChordSketchError::NoSongsFound`]
/// variant is still part of the FFI surface) for binding ABI stability —
/// removing the variant would be a breaking change for Python / Swift /
/// Kotlin / Ruby consumers. The variant is retained as defensive
/// future-proofing in case the lenient parser ever changes its
/// always-returns-one-segment behavior.
fn parse_songs(input: &str) -> Result<Vec<chordsketch_core::ast::Song>, ChordSketchError> {
    let result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
    Ok(songs)
}

/// Forward each warning in a [`RenderResult`] to `eprintln!` and unwrap the output.
///
/// UniFFI-based consumers (Python, Swift, Kotlin, Ruby) receive output via their
/// language binding. Render warnings — such as transpose saturation or chorus
/// recall limits — are forwarded to the platform's standard error stream
/// (`sys.stderr` in Python, `NSLog`/`stderr` in Swift, `System.err` in Kotlin,
/// `$stderr` in Ruby). This matches the NAPI binding's pattern. See #1541.
fn flush_warnings<T>(result: RenderResult<T>) -> T {
    for w in &result.warnings {
        eprintln!("chordsketch: {w}");
    }
    result.output
}

/// Parse ChordPro input and render as plain text.
///
/// Parse warnings are silently discarded — the lenient parser produces a
/// best-effort result even when the input contains errors. To retrieve
/// diagnostics, call [`validate()`] before or after rendering.
///
/// Render warnings (e.g. transpose saturation, chorus recall limits) are
/// forwarded to stderr via `flush_warnings`.
///
/// # Parameters
///
/// * `transpose` — semitone transposition offset applied on top of any
///   in-file `{transpose}` directives. Accepts the full `i8` range
///   (`-128..=127`); musically meaningful values are typically `-24..=24`
///   (two octaves). If the combined offset (API value + in-file directive)
///   saturates the `i8` range, it is clamped and a warning is emitted.
pub fn parse_and_render_text(
    input: String,
    config_json: Option<String>,
    transpose: Option<i8>,
) -> Result<String, ChordSketchError> {
    let config = resolve_config(config_json)?;
    let songs = parse_songs(&input)?;
    Ok(flush_warnings(
        chordsketch_render_text::render_songs_with_warnings(
            &songs,
            transpose.unwrap_or(0),
            &config,
        ),
    ))
}

/// Parse ChordPro input and render as an HTML document.
///
/// Parse warnings are silently discarded — the lenient parser produces a
/// best-effort result even when the input contains errors. To retrieve
/// diagnostics, call [`validate()`] before or after rendering.
///
/// Render warnings (e.g. transpose saturation, chorus recall limits) are
/// forwarded to stderr via `flush_warnings`.
///
/// See [`parse_and_render_text`] for `transpose` parameter documentation.
pub fn parse_and_render_html(
    input: String,
    config_json: Option<String>,
    transpose: Option<i8>,
) -> Result<String, ChordSketchError> {
    let config = resolve_config(config_json)?;
    let songs = parse_songs(&input)?;
    Ok(flush_warnings(
        chordsketch_render_html::render_songs_with_warnings(
            &songs,
            transpose.unwrap_or(0),
            &config,
        ),
    ))
}

/// Parse ChordPro input and render as a PDF document.
///
/// Parse warnings are silently discarded — the lenient parser produces a
/// best-effort result even when the input contains errors. To retrieve
/// diagnostics, call [`validate()`] before or after rendering.
///
/// Render warnings (e.g. transpose saturation, chorus recall limits) are
/// forwarded to stderr via `flush_warnings`.
///
/// See [`parse_and_render_text`] for `transpose` parameter documentation.
pub fn parse_and_render_pdf(
    input: String,
    config_json: Option<String>,
    transpose: Option<i8>,
) -> Result<Vec<u8>, ChordSketchError> {
    let config = resolve_config(config_json)?;
    let songs = parse_songs(&input)?;
    Ok(flush_warnings(
        chordsketch_render_pdf::render_songs_with_warnings(&songs, transpose.unwrap_or(0), &config),
    ))
}

/// Validate ChordPro input and return any parse errors as strings.
#[must_use]
pub fn validate(input: String) -> Vec<String> {
    let result = chordsketch_core::parse_multi_lenient(&input);
    result
        .results
        .into_iter()
        .flat_map(|r| r.errors.into_iter().map(|e| e.to_string()))
        .collect()
}

/// Return the ChordSketch library version.
pub fn version() -> String {
    chordsketch_core::version().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_INPUT: &str = "{title: Test}\n[C]Hello";

    #[test]
    fn test_render_text() {
        let result = parse_and_render_text(MINIMAL_INPUT.to_string(), None, None);
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("Test"));
        assert!(text.contains("Hello"));
    }

    #[test]
    fn test_render_html() {
        let result = parse_and_render_html(MINIMAL_INPUT.to_string(), None, None);
        assert!(result.is_ok());
        let html = result.unwrap();
        assert!(html.contains("Test"));
    }

    #[test]
    fn test_render_pdf() {
        let result = parse_and_render_pdf(MINIMAL_INPUT.to_string(), None, None);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_render_with_transpose() {
        let result = parse_and_render_text(MINIMAL_INPUT.to_string(), None, Some(2));
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_with_preset_config() {
        let result =
            parse_and_render_text(MINIMAL_INPUT.to_string(), Some("guitar".to_string()), None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_with_rrjson_config() {
        let result = parse_and_render_text(
            MINIMAL_INPUT.to_string(),
            Some(r#"{ "settings": { "transpose": 2 } }"#.to_string()),
            None,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_config() {
        let result = parse_and_render_text(
            MINIMAL_INPUT.to_string(),
            Some("{ invalid rrjson !!!".to_string()),
            None,
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            ChordSketchError::InvalidConfig { reason } => {
                assert!(!reason.is_empty());
            }
            other => panic!("expected InvalidConfig, got {other:?}"),
        }
    }

    #[test]
    fn test_validate_valid_input() {
        let errors = validate(MINIMAL_INPUT.to_string());
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_returns_errors_for_bad_input() {
        // An unclosed chord bracket is always a parse error, even in lenient mode.
        let errors = validate("{title: Test}\n[G".to_string());
        assert!(
            !errors.is_empty(),
            "unclosed chord should produce a parse error"
        );
        assert!(
            errors[0].contains("unclosed"),
            "error message should mention 'unclosed', got: {}",
            errors[0]
        );
    }

    #[test]
    fn test_version() {
        let v = version();
        assert!(!v.is_empty());
    }

    #[test]
    fn test_render_succeeds_despite_parse_warnings() {
        // Input with an unclosed chord produces parse warnings, but the
        // lenient parser still yields a song that can be rendered.
        // Callers who want diagnostics should call validate() separately.
        let input = "{title: Warn}\n[C]Hello [G\nWorld".to_string();
        let result = parse_and_render_text(input.clone(), None, None);
        assert!(result.is_ok(), "render should succeed despite parse errors");
        let text = result.unwrap();
        assert!(text.contains("Warn"), "title should be rendered");

        // validate() surfaces the warnings that render silently discards.
        let warnings = validate(input);
        assert!(
            !warnings.is_empty(),
            "validate should report the unclosed chord"
        );
    }

    #[test]
    fn test_roundtrip_text() {
        let input = "{title: Roundtrip Test}\n{subtitle: FFI}\n\n[Am]First [G]line\n[C]Second line";
        let result = parse_and_render_text(input.to_string(), None, None);
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(text.contains("Roundtrip Test"));
        assert!(text.contains("First"));
        assert!(text.contains("Second"));
    }

    #[test]
    fn test_roundtrip_html() {
        let input = "{title: Roundtrip Test}\n[Am]Hello [G]World";
        let result = parse_and_render_html(input.to_string(), None, None);
        assert!(result.is_ok());
        let html = result.unwrap();
        assert!(html.contains("Roundtrip Test"));
        assert!(html.contains("Hello"));
    }

    #[test]
    fn test_roundtrip_pdf() {
        let input = "{title: PDF Roundtrip}\n[C]Test";
        let result = parse_and_render_pdf(input.to_string(), None, None);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"%PDF"));
    }
}
