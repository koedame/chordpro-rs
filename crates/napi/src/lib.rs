//! Native Node.js addon for ChordSketch via [napi-rs](https://napi.rs/).
//!
//! Provides the same API as `@chordsketch/wasm` but as a prebuilt native
//! addon, offering better performance and no WASM overhead.

use napi::bindgen_prelude::*;
use napi_derive::napi;

/// Render options matching the WASM package API.
#[napi(object)]
pub struct RenderOptions {
    /// Semitone transposition offset (-12 to +12). Defaults to 0.
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

/// Parse and validate a transpose value, returning an error if out of range.
fn parse_transpose(raw: i32) -> Result<i8> {
    if !(-12..=12).contains(&raw) {
        return Err(Error::new(
            Status::InvalidArg,
            format!("transpose must be between -12 and 12, got {raw}"),
        ));
    }
    Ok(raw as i8)
}

/// Render ChordPro input as plain text with options.
#[napi]
pub fn render_text_with_options(input: String, options: RenderOptions) -> Result<String> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0))?;
    let songs = parse_songs(&input)?;
    Ok(chordsketch_render_text::render_songs_with_transpose(
        &songs, transpose, &config,
    ))
}

/// Render ChordPro input as an HTML document with options.
#[napi]
pub fn render_html_with_options(input: String, options: RenderOptions) -> Result<String> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0))?;
    let songs = parse_songs(&input)?;
    Ok(chordsketch_render_html::render_songs_with_transpose(
        &songs, transpose, &config,
    ))
}

/// Render ChordPro input as a PDF document with options (returned as a Buffer).
#[napi]
pub fn render_pdf_with_options(input: String, options: RenderOptions) -> Result<Buffer> {
    let config = resolve_config(options.config)?;
    let transpose = parse_transpose(options.transpose.unwrap_or(0))?;
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
