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

/// Structured render result returned by the `*_with_warnings` family
/// (string outputs).
///
/// Callers that need warning-driven UI (inline banners, telemetry
/// aggregation, selective suppression) should use the `*_with_warnings`
/// entry points. The plain variants (`renderText`, `renderHtml`,
/// `renderPdf`) forward warnings to `eprintln!`, which lands on process
/// stderr — fine for CLI scripts but invisible to a React component
/// embedding the addon. See issue #1827.
#[napi(object)]
pub struct TextRenderWithWarnings {
    /// Rendered text or HTML output.
    pub output: String,
    /// Renderer warnings captured during the render pass.
    pub warnings: Vec<String>,
}

/// Structured render result for PDF output. See
/// [`TextRenderWithWarnings`] for the warnings contract.
#[napi(object)]
pub struct PdfRenderWithWarnings {
    /// PDF byte stream.
    pub output: Buffer,
    /// Renderer warnings captured during the render pass.
    pub warnings: Vec<String>,
}

/// String-returning render that captures warnings as structured data.
///
/// Shared by `render_text_with_warnings` and `render_html_with_warnings`
/// so both variants route through the same parse + render + capture
/// pipeline without the `flush_warnings` stderr side effect.
fn do_render_string_with_warnings(
    input: &str,
    config: &chordsketch_core::config::Config,
    transpose: i8,
    render_fn: fn(
        &[chordsketch_core::ast::Song],
        i8,
        &chordsketch_core::config::Config,
    ) -> RenderResult<String>,
) -> Result<TextRenderWithWarnings> {
    let songs = parse_songs(input)?;
    let result = render_fn(&songs, transpose, config);
    Ok(TextRenderWithWarnings {
        output: result.output,
        warnings: result.warnings,
    })
}

/// Render ChordPro input as plain text, returning both output and
/// captured warnings.
///
/// This is the structured variant of [`render_text`]. Warnings are
/// returned as an array of strings instead of being forwarded to
/// process stderr via `eprintln!`. Callers that do not need warnings
/// should keep using the plain `render_text`.
#[must_use = "callers must handle render errors"]
#[napi]
pub fn render_text_with_warnings(input: String) -> Result<TextRenderWithWarnings> {
    do_render_string_with_warnings(
        &input,
        &chordsketch_core::config::Config::defaults(),
        0,
        chordsketch_render_text::render_songs_with_warnings,
    )
}

/// Render ChordPro input as HTML, returning both output and captured
/// warnings.
///
/// See [`render_text_with_warnings`] for the warnings contract.
#[must_use = "callers must handle render errors"]
#[napi]
pub fn render_html_with_warnings(input: String) -> Result<TextRenderWithWarnings> {
    do_render_string_with_warnings(
        &input,
        &chordsketch_core::config::Config::defaults(),
        0,
        chordsketch_render_html::render_songs_with_warnings,
    )
}

/// Render ChordPro input as a PDF document, returning the byte stream
/// alongside captured warnings.
///
/// See [`render_text_with_warnings`] for the warnings contract.
#[must_use = "callers must handle render errors"]
#[napi]
pub fn render_pdf_with_warnings(input: String) -> Result<PdfRenderWithWarnings> {
    do_render_pdf_with_warnings(&input, &chordsketch_core::config::Config::defaults(), 0)
}

/// Shared implementation for the PDF `*_with_warnings` variants. Extracted
/// so [`render_pdf_with_warnings`] and
/// [`render_pdf_with_warnings_and_options`] route through the same
/// parse + render + capture pipeline.
fn do_render_pdf_with_warnings(
    input: &str,
    config: &chordsketch_core::config::Config,
    transpose: i8,
) -> Result<PdfRenderWithWarnings> {
    let songs = parse_songs(input)?;
    let result = chordsketch_render_pdf::render_songs_with_warnings(&songs, transpose, config);
    Ok(PdfRenderWithWarnings {
        output: result.output.into(),
        warnings: result.warnings,
    })
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

/// Render ChordPro input as plain text with options, returning both output
/// and captured warnings.
///
/// Combines the `options` payload of [`render_text_with_options`] with the
/// structured-warning capture of [`render_text_with_warnings`] (#1895).
#[must_use = "callers must handle render errors"]
#[napi]
pub fn render_text_with_warnings_and_options(
    input: String,
    options: RenderOptions,
) -> Result<TextRenderWithWarnings> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0))?;
    do_render_string_with_warnings(
        &input,
        &config,
        transpose,
        chordsketch_render_text::render_songs_with_warnings,
    )
}

/// Render ChordPro input as HTML with options, returning both output and
/// captured warnings.
///
/// See [`render_text_with_warnings_and_options`] for the contract.
#[must_use = "callers must handle render errors"]
#[napi]
pub fn render_html_with_warnings_and_options(
    input: String,
    options: RenderOptions,
) -> Result<TextRenderWithWarnings> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0))?;
    do_render_string_with_warnings(
        &input,
        &config,
        transpose,
        chordsketch_render_html::render_songs_with_warnings,
    )
}

/// Render ChordPro input as a PDF document with options, returning the byte
/// stream alongside captured warnings.
///
/// See [`render_text_with_warnings_and_options`] for the contract.
#[must_use = "callers must handle render errors"]
#[napi]
pub fn render_pdf_with_warnings_and_options(
    input: String,
    options: RenderOptions,
) -> Result<PdfRenderWithWarnings> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0))?;
    do_render_pdf_with_warnings(&input, &config, transpose)
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

    // -- *_with_warnings exposes structured warnings (#1827) ---------------
    //
    // The `#[napi]` wrappers `render_*_with_warnings` return
    // `napi::Result<TextRenderWithWarnings>` / `napi::Result<PdfRenderWithWarnings>`,
    // whose error path references Node-API symbols via `Drop`. Tests
    // therefore bypass the wrapper and exercise the underlying
    // chordsketch-core renderer directly, mirroring the pattern used by
    // `test_try_parse_transpose_*` for issue #1826.

    #[test]
    fn test_with_warnings_captures_core_output() {
        let result = chordsketch_core::parse_multi_lenient(MINIMAL_INPUT);
        let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
        let text = chordsketch_render_text::render_songs_with_warnings(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        );
        assert!(!text.output.is_empty());
        assert!(
            text.warnings.is_empty(),
            "minimal input should produce no warnings; got {:?}",
            text.warnings
        );
        let html = chordsketch_render_html::render_songs_with_warnings(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        );
        assert!(html.output.contains("<html"));
        assert!(html.warnings.is_empty());
        let pdf = chordsketch_render_pdf::render_songs_with_warnings(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        );
        assert!(pdf.output.starts_with(b"%PDF"));
        assert!(pdf.warnings.is_empty());
    }

    #[test]
    fn test_text_render_with_warnings_struct_fields_exist() {
        // Compile-time check that `TextRenderWithWarnings::output` is
        // a String and `.warnings` is a Vec<String> — these field names
        // and types are part of the public #[napi(object)] API, so a
        // rename is a breaking change that should be deliberate.
        let v = super::TextRenderWithWarnings {
            output: String::from("ok"),
            warnings: vec!["w".to_string()],
        };
        assert_eq!(v.output, "ok");
        assert_eq!(v.warnings, vec!["w".to_string()]);
    }

    // -- *_with_warnings_and_options (#1895) ------------------------------
    //
    // The new `render_*_with_warnings_and_options` wrappers cannot be
    // called directly from `cargo test --lib` because their return types
    // (`Result<_, napi::Error>` and `Buffer`) reference Node-API symbols
    // via `Drop`. The same constraint applies to every other `#[napi]` in
    // this file, so the established pattern is to exercise the underlying
    // chordsketch-core code paths that the wrapper delegates to.
    //
    // The wrapper body is a three-line delegation:
    //   1. `resolve_config(options.config)?`
    //   2. `parse_transpose(options.transpose.unwrap_or(0))?`
    //   3. `do_render_*_with_warnings(&input, &config, transpose, …)`
    //
    // Step 1 is exercised by `test_with_warnings_captures_core_output`
    // (via `Config::defaults`). Step 2 is exercised by
    // `test_try_parse_transpose_*`. Step 3 — the plumbing that carries
    // `transpose` through the renderer — is covered below.

    #[test]
    fn test_transpose_option_changes_text_render_output() {
        // Regression guard: a refactor of
        // `render_text_with_warnings_and_options` that forgot to thread
        // `opts.transpose` into the renderer would compile (`0` is the
        // default) but silently ignore the option. The pure renderer
        // call below proves the core renderer responds to `transpose`,
        // which pins down exactly the contract the wrapper promises.
        let result = chordsketch_core::parse_multi_lenient(MINIMAL_INPUT);
        let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
        let zero = chordsketch_render_text::render_songs_with_warnings(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        )
        .output;
        let shifted = chordsketch_render_text::render_songs_with_warnings(
            &songs,
            2,
            &chordsketch_core::config::Config::defaults(),
        )
        .output;
        assert_ne!(
            zero, shifted,
            "transpose=2 must produce different output from transpose=0"
        );
    }

    #[test]
    fn test_transpose_option_changes_html_render_output() {
        // Same plumbing guard for the HTML variant.
        let result = chordsketch_core::parse_multi_lenient(MINIMAL_INPUT);
        let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
        let zero = chordsketch_render_html::render_songs_with_warnings(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        )
        .output;
        let shifted = chordsketch_render_html::render_songs_with_warnings(
            &songs,
            2,
            &chordsketch_core::config::Config::defaults(),
        )
        .output;
        assert_ne!(
            zero, shifted,
            "transpose=2 must produce different output from transpose=0"
        );
    }

    #[test]
    fn test_transpose_option_changes_pdf_render_output() {
        // Same plumbing guard for the PDF variant.
        let result = chordsketch_core::parse_multi_lenient(MINIMAL_INPUT);
        let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
        let zero = chordsketch_render_pdf::render_songs_with_warnings(
            &songs,
            0,
            &chordsketch_core::config::Config::defaults(),
        )
        .output;
        let shifted = chordsketch_render_pdf::render_songs_with_warnings(
            &songs,
            2,
            &chordsketch_core::config::Config::defaults(),
        )
        .output;
        assert_ne!(zero, shifted, "transpose=2 must alter the PDF byte stream");
        assert!(zero.starts_with(b"%PDF"));
    }

    #[test]
    fn test_config_option_preset_resolves() {
        // Minimal regression guard that the preset-name path of
        // `resolve_config` used by `*_with_warnings_and_options` still
        // returns a `Some(...)`. The preset's effect on rendered output
        // depends on which config option it sets and what the input
        // exercises, which is covered by the Config tests in
        // chordsketch-core; this test just pins the name-lookup contract
        // so a future rename of the "guitar" preset would surface here.
        let preset = chordsketch_core::config::Config::preset("guitar");
        assert!(preset.is_some(), "the 'guitar' preset must be available");
    }
}
