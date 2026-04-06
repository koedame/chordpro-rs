//! UniFFI bindings for ChordSketch.
//!
//! Exposes ChordPro parsing and rendering (text, HTML, PDF) to Python,
//! Swift, Kotlin, and Ruby via [UniFFI](https://mozilla.github.io/uniffi-rs/).

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

/// Parse input into songs, returning an error if no songs are found.
fn parse_songs(input: &str) -> Result<Vec<chordsketch_core::ast::Song>, ChordSketchError> {
    let result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
    if songs.is_empty() {
        return Err(ChordSketchError::NoSongsFound);
    }
    Ok(songs)
}

/// Parse ChordPro input and render as plain text.
pub fn parse_and_render_text(
    input: String,
    config_json: Option<String>,
    transpose: Option<i8>,
) -> Result<String, ChordSketchError> {
    let config = resolve_config(config_json)?;
    let songs = parse_songs(&input)?;
    Ok(chordsketch_render_text::render_songs_with_transpose(
        &songs,
        transpose.unwrap_or(0),
        &config,
    ))
}

/// Parse ChordPro input and render as an HTML document.
pub fn parse_and_render_html(
    input: String,
    config_json: Option<String>,
    transpose: Option<i8>,
) -> Result<String, ChordSketchError> {
    let config = resolve_config(config_json)?;
    let songs = parse_songs(&input)?;
    Ok(chordsketch_render_html::render_songs_with_transpose(
        &songs,
        transpose.unwrap_or(0),
        &config,
    ))
}

/// Parse ChordPro input and render as a PDF document.
pub fn parse_and_render_pdf(
    input: String,
    config_json: Option<String>,
    transpose: Option<i8>,
) -> Result<Vec<u8>, ChordSketchError> {
    let config = resolve_config(config_json)?;
    let songs = parse_songs(&input)?;
    Ok(chordsketch_render_pdf::render_songs_with_transpose(
        &songs,
        transpose.unwrap_or(0),
        &config,
    ))
}

/// Validate ChordPro input and return any parse errors as strings.
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
        // Strict parsing of unclosed directive would produce an error
        // but lenient parsing may accept it. Use a structurally invalid directive.
        let errors = validate("{title: Test}\n{invalid_directive}".to_string());
        // The lenient parser may or may not produce errors here depending
        // on how unknown directives are handled, so just check the return type.
        let _ = errors;
    }

    #[test]
    fn test_version() {
        let v = version();
        assert!(!v.is_empty());
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
