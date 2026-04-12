//! Native Node.js addon for ChordSketch via [napi-rs](https://napi.rs/).
//!
//! Provides the same API as `@chordsketch/wasm` but as a prebuilt native
//! addon, offering better performance and no WASM overhead.

use chordsketch_core::render_result::RenderResult;
use napi::bindgen_prelude::*;
use napi_derive::napi;

/// Render options matching the WASM package API.
#[napi(object)]
pub struct RenderOptions {
    /// Semitone transposition offset. Defaults to 0.
    ///
    /// Any integer is accepted, but values outside `i8` range
    /// (`-128..=127`) are clamped to that range *before* the renderer
    /// reduces modulo 12. So `transpose: 200` becomes
    /// `clamp(200, -128, 127) = 127`, and the effective offset is
    /// `127 % 12 = 7` — not `200 % 12 = 8`. This only matters for
    /// transpositions greater than ~10 octaves (see #1080).
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

/// Parse input into songs.
///
/// `parse_multi_lenient` always returns at least one `ParseResult`
/// (`split_at_new_song` unconditionally pushes the trailing segment, even
/// for empty input — see `chordsketch_core::parser`), so the resulting
/// `Vec<Song>` is never empty. The previous `is_empty()` guard was dead
/// code. See #1083. The function still returns `Result` because the
/// `*_with_options` callers use the same `napi::Result` channel for
/// their `resolve_config` failures.
fn parse_songs(input: &str) -> Result<Vec<chordsketch_core::ast::Song>> {
    let result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
    Ok(songs)
}

/// Forward each warning in a [`RenderResult`] to `eprintln!` and unwrap the
/// output.
///
/// In a Node.js addon, `eprintln!` writes to process stderr, which is visible
/// to callers (unlike WASM in a browser context where stderr is silently
/// dropped). This matches the WASM binding's `flush_warnings` pattern: both
/// bindings now explicitly capture warnings via `render_songs_with_warnings`
/// rather than relying on the renderer's internal eprintln paths. See #1541.
fn flush_warnings<T>(result: RenderResult<T>) -> T {
    for w in &result.warnings {
        eprintln!("chordsketch: {w}");
    }
    result.output
}

/// Parse and render songs as a string, forwarding any render warnings to stderr.
///
/// Single source of truth for all string-output render calls. Accepts a
/// `render_fn` so it can be shared by text and HTML renderers.
fn do_render_string(
    input: &str,
    config: &chordsketch_core::config::Config,
    transpose: i8,
    render_fn: fn(
        &[chordsketch_core::ast::Song],
        i8,
        &chordsketch_core::config::Config,
    ) -> RenderResult<String>,
) -> Result<String> {
    let songs = parse_songs(input)?;
    Ok(flush_warnings(render_fn(&songs, transpose, config)))
}

/// Parse and render songs as bytes, forwarding any render warnings to stderr.
///
/// See [`do_render_string`] — same pattern for PDF output.
fn do_render_bytes(
    input: &str,
    config: &chordsketch_core::config::Config,
    transpose: i8,
    render_fn: fn(
        &[chordsketch_core::ast::Song],
        i8,
        &chordsketch_core::config::Config,
    ) -> RenderResult<Vec<u8>>,
) -> Result<Vec<u8>> {
    let songs = parse_songs(input)?;
    Ok(flush_warnings(render_fn(&songs, transpose, config)))
}

/// Render ChordPro input as plain text using default configuration.
///
/// Use [`render_text_with_options`] to pass a config preset, inline RRJSON,
/// or transposition.
#[napi]
pub fn render_text(input: String) -> Result<String> {
    do_render_string(
        &input,
        &chordsketch_core::config::Config::defaults(),
        0,
        chordsketch_render_text::render_songs_with_warnings,
    )
}

/// Render ChordPro input as an HTML document using default configuration.
///
/// Use [`render_html_with_options`] to pass a config preset, inline RRJSON,
/// or transposition.
#[napi]
pub fn render_html(input: String) -> Result<String> {
    do_render_string(
        &input,
        &chordsketch_core::config::Config::defaults(),
        0,
        chordsketch_render_html::render_songs_with_warnings,
    )
}

/// Render ChordPro input as a PDF document using default configuration
/// (returned as a Buffer).
///
/// Use [`render_pdf_with_options`] to pass a config preset, inline RRJSON,
/// or transposition.
#[napi]
pub fn render_pdf(input: String) -> Result<Buffer> {
    let bytes = do_render_bytes(
        &input,
        &chordsketch_core::config::Config::defaults(),
        0,
        chordsketch_render_pdf::render_songs_with_warnings,
    )?;
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
    do_render_string(
        &input,
        &config,
        transpose,
        chordsketch_render_text::render_songs_with_warnings,
    )
}

/// Render ChordPro input as an HTML document with options.
#[napi]
pub fn render_html_with_options(input: String, options: RenderOptions) -> Result<String> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0));
    do_render_string(
        &input,
        &config,
        transpose,
        chordsketch_render_html::render_songs_with_warnings,
    )
}

/// Render ChordPro input as a PDF document with options (returned as a Buffer).
#[napi]
pub fn render_pdf_with_options(input: String, options: RenderOptions) -> Result<Buffer> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0));
    let bytes = do_render_bytes(
        &input,
        &config,
        transpose,
        chordsketch_render_pdf::render_songs_with_warnings,
    )?;
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
        // Use render_songs_with_warnings to match the code path used by the NAPI
        // binding's do_render_string (via flush_warnings).
        let text = chordsketch_render_text::render_songs_with_warnings(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        )
        .output;
        assert!(!text.is_empty());
        assert!(text.contains("Test"));
    }

    #[test]
    fn test_render_html_returns_content() {
        let result = chordsketch_core::parse_multi_lenient(MINIMAL_INPUT);
        let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
        // Use render_songs_with_warnings to match the code path used by the NAPI
        // binding's do_render_string (via flush_warnings).
        let html = chordsketch_render_html::render_songs_with_warnings(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        )
        .output;
        assert!(!html.is_empty());
        assert!(html.contains("Test"));
    }

    #[test]
    fn test_render_pdf_returns_bytes() {
        let result = chordsketch_core::parse_multi_lenient(MINIMAL_INPUT);
        let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
        // Use render_songs_with_warnings to match the code path used by the NAPI
        // binding's do_render_bytes (via flush_warnings).
        let bytes = chordsketch_render_pdf::render_songs_with_warnings(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        )
        .output;
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

    /// Verifies that `render_songs_with_warnings` captures warnings rather than
    /// silently discarding them. The saturation input ({transpose: 100} in
    /// source + 100 CLI transpose = 200 > 127) reliably triggers a "transpose
    /// clamped" warning from the renderer. This is the regression test for
    /// #1541: previously `render_songs_with_transpose` was called instead,
    /// which routed warnings to internal eprintln and made them invisible
    /// to the napi binding.
    #[test]
    fn test_render_songs_with_warnings_captures_saturation_warning() {
        let input = "{title: T}\n{transpose: 100}\n[C]Hello";
        let result = chordsketch_core::parse_multi_lenient(input);
        let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
        let render_result = chordsketch_render_text::render_songs_with_warnings(
            &songs,
            100,
            &chordsketch_core::config::Config::defaults(),
        );
        assert!(
            !render_result.warnings.is_empty(),
            "expected at least one transpose saturation warning, got none"
        );
        assert!(
            !render_result.output.is_empty(),
            "render output must not be empty"
        );
    }
}
