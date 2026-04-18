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
    /// Must fit in `i8` (`-128..=127`). Values outside that range cause
    /// the call to reject with `Status::InvalidArg`, matching the CLI,
    /// UniFFI, and WASM bindings (issue #1826 — previously this binding
    /// silently clamped, which produced different output for the same
    /// input depending on which binding was used).
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
#[must_use = "callers must handle render errors"]
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
#[must_use = "callers must handle render errors"]
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
#[must_use = "callers must handle render errors"]
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

/// Coerce a JS-supplied transposition value to `i8`, rejecting
/// out-of-range integers.
///
/// Every other binding (CLI via clap, UniFFI at the boundary, WASM via
/// `serde_wasm_bindgen`) rejects integers that do not fit in `i8`.
/// napi-rs has no built-in `i8` unmarshaling — the wire type is `i32` —
/// so the check has to run here at the first opportunity. Returning an
/// `InvalidArg` error gives the same failure shape across all four
/// bindings for the same input (issue #1826).
///
/// The previous implementation clamped instead of rejecting, which
/// silently produced a different musical result for inputs outside
/// `-128..=127` compared with the other three bindings. See #1065 for
/// the earlier iteration of this divergence.
fn parse_transpose(raw: i32) -> Result<i8> {
    try_parse_transpose(raw).map_err(|msg| Error::new(Status::InvalidArg, msg))
}

/// Pure-Rust cousin of [`parse_transpose`] that returns an owned error
/// message instead of a `napi::Error`.
///
/// This separation keeps the validation logic unit-testable: `napi::Error`
/// has a `Drop` impl that references Node-API symbols, so constructing
/// one inside a plain `cargo test` binary would fail to link. Tests call
/// this helper directly; production callers go through `parse_transpose`
/// and get a proper `napi::Error`.
fn try_parse_transpose(raw: i32) -> std::result::Result<i8, String> {
    i8::try_from(raw).map_err(|_| {
        format!(
            "transpose value {raw} is out of range; expected an integer \
             in -128..=127 (matches CLI / UniFFI / WASM binding semantics)"
        )
    })
}

/// Render ChordPro input as plain text with options.
#[must_use = "callers must handle render errors"]
#[napi]
pub fn render_text_with_options(input: String, options: RenderOptions) -> Result<String> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0))?;
    do_render_string(
        &input,
        &config,
        transpose,
        chordsketch_render_text::render_songs_with_warnings,
    )
}

/// Render ChordPro input as an HTML document with options.
#[must_use = "callers must handle render errors"]
#[napi]
pub fn render_html_with_options(input: String, options: RenderOptions) -> Result<String> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0))?;
    do_render_string(
        &input,
        &config,
        transpose,
        chordsketch_render_html::render_songs_with_warnings,
    )
}

/// Render ChordPro input as a PDF document with options (returned as a Buffer).
#[must_use = "callers must handle render errors"]
#[napi]
pub fn render_pdf_with_options(input: String, options: RenderOptions) -> Result<Buffer> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0))?;
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
#[must_use]
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
#[must_use]
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
    fn test_try_parse_transpose_in_range_passes_through() {
        // After #1826, parse_transpose rejects out-of-range i32 values
        // so the NAPI binding matches the CLI / UniFFI / WASM "reject"
        // contract rather than silently clamping. Tests use the pure-
        // Rust helper `try_parse_transpose` (see its doc comment) so
        // that `cargo test --lib` does not have to link Node-API.
        use super::try_parse_transpose;
        assert_eq!(try_parse_transpose(0).unwrap(), 0);
        assert_eq!(try_parse_transpose(7).unwrap(), 7);
        assert_eq!(try_parse_transpose(-7).unwrap(), -7);
        assert_eq!(try_parse_transpose(12).unwrap(), 12);
        assert_eq!(try_parse_transpose(-12).unwrap(), -12);
        // Boundary values pass through.
        assert_eq!(try_parse_transpose(127).unwrap(), 127);
        assert_eq!(try_parse_transpose(-128).unwrap(), -128);
    }

    #[test]
    fn test_try_parse_transpose_out_of_range_rejected() {
        // Every value one step past the i8 boundary must error with a
        // message that carries the offending value.
        use super::try_parse_transpose;
        for bad in [128_i32, 129, 200, 1_000_000, i32::MAX] {
            let msg = try_parse_transpose(bad).unwrap_err();
            assert!(
                msg.contains("out of range") && msg.contains(&bad.to_string()),
                "positive out-of-range {bad} must report its value; got {msg:?}"
            );
        }
        for bad in [-129_i32, -200, -1_000_000, i32::MIN] {
            let msg = try_parse_transpose(bad).unwrap_err();
            assert!(
                msg.contains("out of range") && msg.contains(&bad.to_string()),
                "negative out-of-range {bad} must report its value; got {msg:?}"
            );
        }
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
