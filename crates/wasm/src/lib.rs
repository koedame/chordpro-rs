//! WebAssembly bindings for ChordSketch.
//!
//! Exposes ChordPro parsing and rendering (HTML, plain text, PDF) to
//! JavaScript/TypeScript via `wasm-bindgen`.

use serde::Deserialize;
use wasm_bindgen::prelude::*;

/// Render options passed from JavaScript.
#[derive(Deserialize, Default)]
struct RenderOptions {
    /// Semitone transposition offset (-12 to +12).
    #[serde(default)]
    transpose: i8,
    /// Configuration preset name (e.g., "guitar", "ukulele") or inline
    /// RRJSON configuration string.
    #[serde(default)]
    config: Option<String>,
}

/// Resolve a [`chordsketch_core::config::Config`] from the options.
fn resolve_config(opts: &RenderOptions) -> chordsketch_core::config::Config {
    match &opts.config {
        Some(name) => {
            // Try as a preset first, then as inline RRJSON.
            if let Some(preset) = chordsketch_core::config::Config::preset(name) {
                preset
            } else if let Ok(parsed) = chordsketch_core::config::Config::parse(name) {
                parsed
            } else {
                chordsketch_core::config::Config::defaults()
            }
        }
        None => chordsketch_core::config::Config::defaults(),
    }
}

/// Parse input and render songs, returning a `String` result or an error.
fn do_render_string(
    input: &str,
    config: &chordsketch_core::config::Config,
    transpose: i8,
    render_fn: fn(&[chordsketch_core::ast::Song], i8, &chordsketch_core::config::Config) -> String,
) -> Result<String, JsValue> {
    let result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
    if songs.is_empty() {
        return Err(JsValue::from_str("no songs found in input"));
    }
    Ok(render_fn(&songs, transpose, config))
}

/// Render ChordPro input as HTML.
///
/// Returns the rendered HTML string.
///
/// # Errors
///
/// Returns a `JsValue` error string if the input contains no songs.
#[wasm_bindgen]
pub fn render_html(input: &str) -> Result<String, JsValue> {
    do_render_string(
        input,
        &chordsketch_core::config::Config::defaults(),
        0,
        chordsketch_render_html::render_songs_with_transpose,
    )
}

/// Render ChordPro input as plain text.
///
/// Returns the rendered text string.
///
/// # Errors
///
/// Returns a `JsValue` error string if the input contains no songs.
#[wasm_bindgen]
pub fn render_text(input: &str) -> Result<String, JsValue> {
    do_render_string(
        input,
        &chordsketch_core::config::Config::defaults(),
        0,
        chordsketch_render_text::render_songs_with_transpose,
    )
}

/// Render ChordPro input as a PDF document.
///
/// Returns the PDF as a `Uint8Array`.
///
/// # Errors
///
/// Returns a `JsValue` error string if the input contains no songs.
#[wasm_bindgen]
pub fn render_pdf(input: &str) -> Result<Vec<u8>, JsValue> {
    let config = chordsketch_core::config::Config::defaults();
    let result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
    if songs.is_empty() {
        return Err(JsValue::from_str("no songs found in input"));
    }
    Ok(chordsketch_render_pdf::render_songs_with_transpose(
        &songs, 0, &config,
    ))
}

/// Render ChordPro input as HTML with options.
///
/// `options` is a JavaScript object with optional fields:
/// - `transpose`: semitone offset (integer, default 0)
/// - `config`: preset name ("guitar", "ukulele") or inline RRJSON string
///
/// # Errors
///
/// Returns a `JsValue` error string on parse failure or invalid options.
#[wasm_bindgen]
pub fn render_html_with_options(input: &str, options: JsValue) -> Result<String, JsValue> {
    let opts: RenderOptions =
        serde_wasm_bindgen::from_value(options).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let config = resolve_config(&opts);
    do_render_string(
        input,
        &config,
        opts.transpose,
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
    let opts: RenderOptions =
        serde_wasm_bindgen::from_value(options).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let config = resolve_config(&opts);
    do_render_string(
        input,
        &config,
        opts.transpose,
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
    let opts: RenderOptions =
        serde_wasm_bindgen::from_value(options).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let config = resolve_config(&opts);
    let result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = result.results.into_iter().map(|r| r.song).collect();
    if songs.is_empty() {
        return Err(JsValue::from_str("no songs found in input"));
    }
    Ok(chordsketch_render_pdf::render_songs_with_transpose(
        &songs,
        opts.transpose,
        &config,
    ))
}

/// Returns the ChordSketch library version.
#[wasm_bindgen]
pub fn version() -> String {
    chordsketch_core::version().to_string()
}
