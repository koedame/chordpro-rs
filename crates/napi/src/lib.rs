//! Native Node.js addon for ChordSketch via [napi-rs](https://napi.rs/).
//!
//! Provides the same API as `@chordsketch/wasm` but as a prebuilt native
//! addon, offering better performance and no WASM overhead.

use napi::bindgen_prelude::*;
use napi_derive::napi;

/// Render options matching the WASM package API.
#[napi(object)]
pub struct RenderOptions {
    /// Semitone transposition offset. Any integer is accepted; the renderer
    /// reduces modulo 12. Defaults to 0.
    pub transpose: Option<i32>,
    /// Configuration preset name (e.g., "guitar", "ukulele") or inline
    /// RRJSON configuration string.
    pub config: Option<String>,
}

/// Resolve a config from an optional preset name or RRJSON string.
fn resolve_config(config: Option<String>) -> Result<chordsketch_core::config::Config> {
    match config {
        Some(name) => {
            if let Some(preset) = chordsketch_core::config::Config::preset(&name) {
                Ok(preset)
            } else {
                chordsketch_core::config::Config::parse(&name).map_err(|e| {
                    Error::new(
                        Status::InvalidArg,
                        format!("invalid config (not a known preset and not valid RRJSON): {e}"),
                    )
                })
            }
        }
        None => Ok(chordsketch_core::config::Config::defaults()),
    }
}

/// Parse input into songs, returning an error if none found.
fn parse_songs(input: &str) -> Result<Vec<chordsketch_core::ast::Song>> {
    let result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
    if songs.is_empty() {
        return Err(Error::new(Status::InvalidArg, "no songs found in input"));
    }
    Ok(songs)
}

/// Render ChordPro input as plain text.
#[napi]
pub fn render_text(input: String) -> Result<String> {
    let songs = parse_songs(&input)?;
    Ok(chordsketch_render_text::render_songs_with_transpose(
        &songs,
        0,
        &chordsketch_core::config::Config::defaults(),
    ))
}

/// Render ChordPro input as an HTML document.
#[napi]
pub fn render_html(input: String) -> Result<String> {
    let songs = parse_songs(&input)?;
    Ok(chordsketch_render_html::render_songs_with_transpose(
        &songs,
        0,
        &chordsketch_core::config::Config::defaults(),
    ))
}

/// Render ChordPro input as a PDF document (returned as a Buffer).
#[napi]
pub fn render_pdf(input: String) -> Result<Buffer> {
    let songs = parse_songs(&input)?;
    let bytes = chordsketch_render_pdf::render_songs_with_transpose(
        &songs,
        0,
        &chordsketch_core::config::Config::defaults(),
    );
    Ok(bytes.into())
}

/// Coerce a JS-supplied transposition value to `i8`.
///
/// Matches the CLI / UniFFI / WASM behavior: any integer is accepted, and the
/// underlying renderer reduces modulo 12 internally (`shift_semitone` uses
/// `i16 + rem_euclid(12)` so the arithmetic is overflow-safe). Out-of-range
/// `i32` values are clamped to `i8::MIN..=i8::MAX` rather than rejected,
/// because rejecting them would make this binding behave differently from
/// every other binding for the same input. See #1065 for the cross-binding
/// inconsistency that motivated this change.
fn parse_transpose(raw: i32) -> i8 {
    raw.clamp(i8::MIN as i32, i8::MAX as i32) as i8
}

/// Render ChordPro input as plain text with options.
#[napi]
pub fn render_text_with_options(input: String, options: RenderOptions) -> Result<String> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0));
    let songs = parse_songs(&input)?;
    Ok(chordsketch_render_text::render_songs_with_transpose(
        &songs, transpose, &config,
    ))
}

/// Render ChordPro input as an HTML document with options.
#[napi]
pub fn render_html_with_options(input: String, options: RenderOptions) -> Result<String> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0));
    let songs = parse_songs(&input)?;
    Ok(chordsketch_render_html::render_songs_with_transpose(
        &songs, transpose, &config,
    ))
}

/// Render ChordPro input as a PDF document with options (returned as a Buffer).
#[napi]
pub fn render_pdf_with_options(input: String, options: RenderOptions) -> Result<Buffer> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0));
    let songs = parse_songs(&input)?;
    let bytes = chordsketch_render_pdf::render_songs_with_transpose(&songs, transpose, &config);
    Ok(bytes.into())
}

/// Validate ChordPro input and return any parse errors as strings.
/// Returns an empty array if the input is valid.
#[napi]
pub fn validate(input: String) -> Vec<String> {
    let result = chordsketch_core::parse_multi_lenient(&input);
    result
        .results
        .into_iter()
        .flat_map(|r| r.errors.into_iter().map(|e| e.to_string()))
        .collect()
}

/// Return the ChordSketch library version.
#[napi]
pub fn version() -> String {
    chordsketch_core::version().to_string()
}

// Unit tests exercise the underlying rendering and parsing logic directly
// via chordsketch_core and renderer crates. The napi wrapper functions
// cannot be tested natively because they depend on the Node.js runtime for
// linking (Buffer, napi::Error, etc.).
#[cfg(test)]
mod tests {
    const MINIMAL_INPUT: &str = "{title: Test}\n[C]Hello";

    #[test]
    fn test_render_text_returns_content() {
        let result = chordsketch_core::parse_multi_lenient(MINIMAL_INPUT);
        let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
        assert!(!songs.is_empty());
        let text = chordsketch_render_text::render_songs_with_transpose(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        );
        assert!(!text.is_empty());
        assert!(text.contains("Test"));
    }

    #[test]
    fn test_render_html_returns_content() {
        let result = chordsketch_core::parse_multi_lenient(MINIMAL_INPUT);
        let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
        let html = chordsketch_render_html::render_songs_with_transpose(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        );
        assert!(!html.is_empty());
        assert!(html.contains("Test"));
    }

    #[test]
    fn test_render_pdf_returns_bytes() {
        let result = chordsketch_core::parse_multi_lenient(MINIMAL_INPUT);
        let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
        let bytes = chordsketch_render_pdf::render_songs_with_transpose(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        );
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_version_returns_nonempty_string() {
        let v = chordsketch_core::version();
        assert!(!v.is_empty());
    }

    #[test]
    fn test_transpose_clamps_to_i8_range() {
        // After #1065, parse_transpose clamps any i32 to i8 range and never
        // returns an error, matching the CLI / UniFFI / WASM behavior. The
        // renderer then reduces modulo 12 internally.
        use super::parse_transpose;
        // In-range values pass through unchanged.
        assert_eq!(parse_transpose(0), 0);
        assert_eq!(parse_transpose(7), 7);
        assert_eq!(parse_transpose(-7), -7);
        assert_eq!(parse_transpose(12), 12);
        assert_eq!(parse_transpose(-12), -12);
        // Beyond i8 are clamped, not rejected.
        assert_eq!(parse_transpose(127), 127);
        assert_eq!(parse_transpose(128), 127);
        assert_eq!(parse_transpose(1_000_000), 127);
        assert_eq!(parse_transpose(-128), -128);
        assert_eq!(parse_transpose(-129), -128);
        assert_eq!(parse_transpose(-1_000_000), -128);
    }

    #[test]
    fn test_validate_returns_empty_for_valid_input() {
        let result = chordsketch_core::parse_multi_lenient(MINIMAL_INPUT);
        let errors: Vec<_> = result.results.into_iter().flat_map(|r| r.errors).collect();
        assert!(errors.is_empty());
    }

    #[test]
    fn test_validate_returns_errors_for_bad_input() {
        let result = chordsketch_core::parse_multi_lenient("{title: Test}\n[G");
        let errors: Vec<_> = result.results.into_iter().flat_map(|r| r.errors).collect();
        assert!(!errors.is_empty());
    }

    #[test]
    fn test_preset_config_resolves() {
        assert!(chordsketch_core::config::Config::preset("guitar").is_some());
        assert!(chordsketch_core::config::Config::preset("nonexistent").is_none());
    }

    #[test]
    fn test_invalid_config_fails() {
        assert!(chordsketch_core::config::Config::parse("{ invalid rrjson !!!").is_err());
    }

    #[test]
    fn test_valid_rrjson_config_parses() {
        assert!(
            chordsketch_core::config::Config::parse(r#"{ "settings": { "transpose": 2 } }"#)
                .is_ok()
        );
    }
}
