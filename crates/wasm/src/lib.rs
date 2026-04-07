//! WebAssembly bindings for ChordSketch.
//!
//! Exposes ChordPro parsing and rendering (HTML, plain text, PDF) to
//! JavaScript/TypeScript via `wasm-bindgen`.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

/// Set up the panic hook on module instantiation so any unexpected panic
/// in the renderer surfaces in the JavaScript console with a Rust
/// backtrace instead of an opaque `unreachable executed`. See #1052.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Render options passed from JavaScript.
#[derive(Deserialize, Default)]
struct RenderOptions {
    /// Semitone transposition offset. Any `i8` value is accepted; the
    /// renderer reduces modulo 12 internally. (Aligning with the CLI,
    /// UniFFI bindings, and napi-rs binding behavior — see #1053, #1065.)
    #[serde(default)]
    transpose: i8,
    /// Configuration preset name (e.g., "guitar", "ukulele") or inline
    /// RRJSON configuration string.
    #[serde(default)]
    config: Option<String>,
}

/// Resolve a [`chordsketch_core::config::Config`] from the options.
///
/// # Errors
///
/// Returns an error string if the config value is not a known preset
/// and cannot be parsed as RRJSON.
fn resolve_config(opts: &RenderOptions) -> Result<chordsketch_core::config::Config, JsValue> {
    match &opts.config {
        Some(name) => {
            // Try as a preset first, then as inline RRJSON.
            if let Some(preset) = chordsketch_core::config::Config::preset(name) {
                Ok(preset)
            } else {
                chordsketch_core::config::Config::parse(name).map_err(|e| {
                    JsValue::from_str(&format!(
                        "invalid config (not a known preset and not valid RRJSON): {e}"
                    ))
                })
            }
        }
        None => Ok(chordsketch_core::config::Config::defaults()),
    }
}

/// Parse input and render songs.
///
/// `parse_multi_lenient` always returns at least one `ParseResult`
/// (`split_at_new_song` unconditionally pushes the trailing segment, even
/// for empty input — see `chordsketch_core::parser`), so the resulting
/// `Vec<Song>` is never empty and the previous `is_empty()` guard was
/// dead code. See #1083. The return type stays `Result` because
/// `render_html_with_options` and friends use the same `JsValue` error
/// channel for their config-parse failures.
fn do_render_string(
    input: &str,
    config: &chordsketch_core::config::Config,
    transpose: i8,
    render_fn: fn(&[chordsketch_core::ast::Song], i8, &chordsketch_core::config::Config) -> String,
) -> Result<String, JsValue> {
    let result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
    Ok(render_fn(&songs, transpose, config))
}

/// Parse input and render songs, returning a `Vec<u8>` result.
///
/// See [`do_render_string`] for the note on the lenient parser always
/// producing at least one song.
fn do_render_bytes(
    input: &str,
    config: &chordsketch_core::config::Config,
    transpose: i8,
    render_fn: fn(&[chordsketch_core::ast::Song], i8, &chordsketch_core::config::Config) -> Vec<u8>,
) -> Result<Vec<u8>, JsValue> {
    let result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
    Ok(render_fn(&songs, transpose, config))
}

/// Decode a JS-supplied `options` value into a `RenderOptions` struct.
///
/// Treats `undefined` and `null` as the default options object so the
/// `*_with_options` entry points can be called without an explicit
/// options argument from JS callers.
fn deserialize_options(options: JsValue) -> Result<RenderOptions, JsValue> {
    if options.is_undefined() || options.is_null() {
        Ok(RenderOptions::default())
    } else {
        serde_wasm_bindgen::from_value(options).map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

/// Resolve config and dispatch a string-returning render call.
///
/// Single source of truth shared by `render_html` / `render_text` and
/// their `*_with_options` counterparts. Avoids the boilerplate duplication
/// flagged in #1059. Takes `RenderOptions` directly (not `JsValue`) so
/// the no-options entry points can call this on native test targets
/// without touching wasm-bindgen-imported types.
fn render_string_inner(
    input: &str,
    opts: RenderOptions,
    render_fn: fn(&[chordsketch_core::ast::Song], i8, &chordsketch_core::config::Config) -> String,
) -> Result<String, JsValue> {
    let config = resolve_config(&opts)?;
    do_render_string(input, &config, opts.transpose, render_fn)
}

/// Resolve config and dispatch a bytes-returning render call.
///
/// See [`render_string_inner`].
fn render_bytes_inner(
    input: &str,
    opts: RenderOptions,
    render_fn: fn(&[chordsketch_core::ast::Song], i8, &chordsketch_core::config::Config) -> Vec<u8>,
) -> Result<Vec<u8>, JsValue> {
    let config = resolve_config(&opts)?;
    do_render_bytes(input, &config, opts.transpose, render_fn)
}

/// Render ChordPro input as HTML using default configuration.
///
/// Returns the rendered HTML string. Use [`render_html_with_options`]
/// to pass a config preset or transposition.
///
/// The return type is `Result<String, JsValue>` for consistency with
/// the `*_with_options` variant — this function itself never errors
/// because the lenient parser always produces at least one song and
/// the default config never fails to resolve.
#[wasm_bindgen]
pub fn render_html(input: &str) -> Result<String, JsValue> {
    render_string_inner(
        input,
        RenderOptions::default(),
        chordsketch_render_html::render_songs_with_transpose,
    )
}

/// Render ChordPro input as plain text using default configuration.
///
/// Returns the rendered text string. Use [`render_text_with_options`]
/// to pass a config preset or transposition.
///
/// The return type is `Result<String, JsValue>` for consistency with
/// the `*_with_options` variant — this function itself never errors
/// because the lenient parser always produces at least one song and
/// the default config never fails to resolve.
#[wasm_bindgen]
pub fn render_text(input: &str) -> Result<String, JsValue> {
    render_string_inner(
        input,
        RenderOptions::default(),
        chordsketch_render_text::render_songs_with_transpose,
    )
}

/// Render ChordPro input as a PDF document using default configuration.
///
/// Returns the PDF as a `Uint8Array`. Use [`render_pdf_with_options`]
/// to pass a config preset or transposition.
///
/// The return type is `Result<Vec<u8>, JsValue>` for consistency with
/// the `*_with_options` variant — this function itself never errors
/// because the lenient parser always produces at least one song and
/// the default config never fails to resolve.
#[wasm_bindgen]
pub fn render_pdf(input: &str) -> Result<Vec<u8>, JsValue> {
    render_bytes_inner(
        input,
        RenderOptions::default(),
        chordsketch_render_pdf::render_songs_with_transpose,
    )
}

/// Render ChordPro input as HTML with options.
///
/// `options` is a JavaScript object with optional fields:
/// - `transpose`: semitone offset (integer in `i8` range; the renderer
///   reduces modulo 12 internally)
/// - `config`: preset name ("guitar", "ukulele") or inline RRJSON string
///
/// `undefined` or `null` is accepted and treated as the default options
/// object.
///
/// # Errors
///
/// Returns a `JsValue` error string on parse failure or invalid options.
#[wasm_bindgen]
pub fn render_html_with_options(input: &str, options: JsValue) -> Result<String, JsValue> {
    render_string_inner(
        input,
        deserialize_options(options)?,
        chordsketch_render_html::render_songs_with_transpose,
    )
}

/// Render ChordPro input as plain text with options.
///
/// See [`render_html_with_options`] for the `options` format.
///
/// # Errors
///
/// Returns a `JsValue` error string on parse failure or invalid options.
#[wasm_bindgen]
pub fn render_text_with_options(input: &str, options: JsValue) -> Result<String, JsValue> {
    render_string_inner(
        input,
        deserialize_options(options)?,
        chordsketch_render_text::render_songs_with_transpose,
    )
}

/// Render ChordPro input as a PDF document with options.
///
/// See [`render_html_with_options`] for the `options` format.
///
/// Returns the PDF as a `Uint8Array`.
///
/// # Errors
///
/// Returns a `JsValue` error string on parse failure or invalid options.
#[wasm_bindgen]
pub fn render_pdf_with_options(input: &str, options: JsValue) -> Result<Vec<u8>, JsValue> {
    render_bytes_inner(
        input,
        deserialize_options(options)?,
        chordsketch_render_pdf::render_songs_with_transpose,
    )
}

/// Returns the ChordSketch library version.
#[wasm_bindgen]
pub fn version() -> String {
    chordsketch_core::version().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_INPUT: &str = "{title: Test}\n[C]Hello";

    #[test]
    fn test_render_html_returns_content() {
        let result = render_html(MINIMAL_INPUT);
        assert!(result.is_ok());
        let html = result.unwrap();
        assert!(!html.is_empty());
        assert!(html.contains("Test"));
    }

    #[test]
    fn test_render_text_returns_content() {
        let result = render_text(MINIMAL_INPUT);
        assert!(result.is_ok());
        let text = result.unwrap();
        assert!(!text.is_empty());
        assert!(text.contains("Test"));
    }

    #[test]
    fn test_render_pdf_returns_bytes() {
        let result = render_pdf(MINIMAL_INPUT);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
        // PDF files start with %PDF
        assert!(bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn test_empty_input_returns_ok() {
        // Lenient parser produces an empty song even for blank input.
        let result = render_html("");
        assert!(result.is_ok());
    }

    #[test]
    fn test_version_returns_string() {
        let v = version();
        assert!(!v.is_empty());
    }

    // The following tests cover the chordsketch-core APIs that
    // `resolve_config` delegates to (Config::preset and Config::parse).
    // They do NOT exercise resolve_config itself or the JsValue boundary —
    // those need a wasm-bindgen-test integration test (tracked in #1055).

    #[test]
    fn config_parse_invalid_rrjson_returns_err() {
        let result = chordsketch_core::config::Config::parse("{ invalid rrjson !!!");
        assert!(result.is_err(), "invalid RRJSON should fail to parse");
    }

    #[test]
    fn config_preset_known_and_unknown_names() {
        assert!(
            chordsketch_core::config::Config::preset("guitar").is_some(),
            "guitar preset should exist"
        );
        assert!(
            chordsketch_core::config::Config::preset("nonexistent").is_none(),
            "unknown preset should return None"
        );
    }

    #[test]
    fn config_parse_valid_rrjson_returns_ok() {
        let result =
            chordsketch_core::config::Config::parse(r#"{ "settings": { "transpose": 2 } }"#);
        assert!(result.is_ok(), "valid RRJSON should parse successfully");
    }
}
