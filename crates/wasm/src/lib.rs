//! WebAssembly bindings for ChordSketch.
//!
//! Exposes ChordPro parsing and rendering (HTML, plain text, PDF) to
//! JavaScript/TypeScript via `wasm-bindgen`.

use chordsketch_core::render_result::RenderResult;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

/// Set up the panic hook on module instantiation so any unexpected panic
/// in the renderer surfaces in the JavaScript console with a Rust
/// backtrace instead of an opaque `unreachable executed`. See #1052.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// JS `console.warn` binding, used to surface render warnings (transpose
/// saturation, chorus recall limits, deny-listed metadata overrides) to
/// developers in the browser console.
///
/// On wasm32 this resolves to the global `console.warn`. On native test
/// targets it's a no-op so the unit tests in this file (which run with
/// `cargo test`, not `wasm-pack test`) still compile and run.
///
/// See #1051 — the previous code path called `render_songs_with_transpose`,
/// which forwards warnings to `eprintln!`. In a browser WASM context
/// stderr is dropped; in Node.js it goes to a console nobody reads.
/// Routing warnings through `console.warn` makes them observable in dev
/// tools without changing the public API surface.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(s: &str);
}

#[cfg(not(target_arch = "wasm32"))]
fn console_warn(_s: &str) {}

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

/// Forward each warning in a [`RenderResult`] to `console.warn` and
/// unwrap the output. Single source of truth for the warning side
/// effect, called by both [`do_render_string`] and [`do_render_bytes`].
/// See #1109.
fn flush_warnings<T>(result: RenderResult<T>) -> T {
    for w in &result.warnings {
        console_warn(&format!("chordsketch: {w}"));
    }
    result.output
}

/// Parse input and render songs.
///
/// Calls the renderer's `*_with_warnings` variant and forwards each
/// captured warning to `console.warn` (#1051) before returning the
/// rendered string.
///
/// `parse_multi_lenient` always returns at least one `ParseResult`
/// (`split_at_new_song` unconditionally pushes the trailing segment, even
/// for empty input — see `chordsketch_core::parser`), so the resulting
/// `Vec<Song>` is never empty and the previous `is_empty()` guard was
/// dead code (#1083). The return type stays `Result` because
/// `render_html_with_options` and friends use the same `JsValue` error
/// channel for their config-parse failures.
fn do_render_string(
    input: &str,
    config: &chordsketch_core::config::Config,
    transpose: i8,
    render_fn: fn(
        &[chordsketch_core::ast::Song],
        i8,
        &chordsketch_core::config::Config,
    ) -> RenderResult<String>,
) -> Result<String, JsValue> {
    let parse_result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = parse_result.results.into_iter().map(|r| r.song).collect();
    Ok(flush_warnings(render_fn(&songs, transpose, config)))
}

/// Parse input and render songs, returning a `Vec<u8>` result.
///
/// See [`do_render_string`] for the note on console-routed warnings and
/// the lenient parser always producing at least one song.
fn do_render_bytes(
    input: &str,
    config: &chordsketch_core::config::Config,
    transpose: i8,
    render_fn: fn(
        &[chordsketch_core::ast::Song],
        i8,
        &chordsketch_core::config::Config,
    ) -> RenderResult<Vec<u8>>,
) -> Result<Vec<u8>, JsValue> {
    let parse_result = chordsketch_core::parse_multi_lenient(input);
    let songs: Vec<_> = parse_result.results.into_iter().map(|r| r.song).collect();
    Ok(flush_warnings(render_fn(&songs, transpose, config)))
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
    render_fn: fn(
        &[chordsketch_core::ast::Song],
        i8,
        &chordsketch_core::config::Config,
    ) -> RenderResult<String>,
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
    render_fn: fn(
        &[chordsketch_core::ast::Song],
        i8,
        &chordsketch_core::config::Config,
    ) -> RenderResult<Vec<u8>>,
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
        chordsketch_render_html::render_songs_with_warnings,
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
        chordsketch_render_text::render_songs_with_warnings,
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
        chordsketch_render_pdf::render_songs_with_warnings,
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
        chordsketch_render_html::render_songs_with_warnings,
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
        chordsketch_render_text::render_songs_with_warnings,
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
        chordsketch_render_pdf::render_songs_with_warnings,
    )
}

/// Returns the ChordSketch library version.
#[must_use]
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
    // for that, see the `wasm_tests` module below (gated to wasm32 and
    // run via `wasm-pack test --node` in CI).

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

/// Integration tests that exercise the actual `#[wasm_bindgen]` ->
/// `JsValue` boundary. These cannot run under native `cargo test`
/// because `JsValue` and `console_warn` are wasm-bindgen imports;
/// `wasm_bindgen_test` arranges a JS host (Node.js or a headless
/// browser) to back them.
///
/// Run via `wasm-pack test --node crates/wasm`. CI runs this in
/// `.github/workflows/wasm.yml`. See #1055, #1108.
#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use js_sys::{Array, Reflect};
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::wasm_bindgen_test;

    const MINIMAL_INPUT: &str = "{title: Test}\n[C]Hello";

    /// `render_html_with_options` accepts a real JS object and produces
    /// HTML containing the rendered title. Exercises the
    /// `serde_wasm_bindgen::from_value` deserialization path that
    /// native tests bypass.
    #[wasm_bindgen_test]
    fn render_html_with_options_object() {
        let opts = js_sys::Object::new();
        Reflect::set(&opts, &"transpose".into(), &JsValue::from(2)).unwrap();
        Reflect::set(&opts, &"config".into(), &JsValue::from_str("guitar")).unwrap();
        let result = render_html_with_options(MINIMAL_INPUT, opts.into()).unwrap();
        assert!(result.contains("Test"));
    }

    /// `render_*_with_options(undefined)` is the spelling the no-options
    /// entry points use to delegate. The deserializer treats `undefined`
    /// as the default `RenderOptions`.
    #[wasm_bindgen_test]
    fn render_html_with_options_undefined() {
        let result = render_html_with_options(MINIMAL_INPUT, JsValue::UNDEFINED).unwrap();
        assert!(result.contains("Test"));
    }

    #[wasm_bindgen_test]
    fn render_html_with_options_null() {
        let result = render_html_with_options(MINIMAL_INPUT, JsValue::NULL).unwrap();
        assert!(result.contains("Test"));
    }

    /// A non-numeric `transpose` (string) fails deserialization with a JS
    /// error, matching the `RenderOptions::transpose: i8` declaration.
    /// `serde_wasm_bindgen` rejects the type mismatch before the value
    /// ever reaches `parse_transpose`.
    #[wasm_bindgen_test]
    fn render_html_with_options_invalid_transpose_type() {
        let opts = js_sys::Object::new();
        Reflect::set(
            &opts,
            &"transpose".into(),
            &JsValue::from_str("not a number"),
        )
        .unwrap();
        let result = render_html_with_options(MINIMAL_INPUT, opts.into());
        assert!(
            result.is_err(),
            "string transpose should fail to deserialize"
        );
    }

    /// Invalid `config` strings (neither a known preset nor valid RRJSON)
    /// produce a `JsValue` error string from `resolve_config`.
    #[wasm_bindgen_test]
    fn render_html_with_options_invalid_config() {
        let opts = js_sys::Object::new();
        Reflect::set(
            &opts,
            &"config".into(),
            &JsValue::from_str("{ not valid rrjson"),
        )
        .unwrap();
        let result = render_html_with_options(MINIMAL_INPUT, opts.into());
        assert!(result.is_err(), "invalid config should fail to resolve");
        let err = result.unwrap_err();
        let msg = err.as_string().unwrap_or_default();
        assert!(
            msg.contains("invalid config"),
            "error should mention invalid config, got: {msg}"
        );
    }

    /// `render_pdf_with_options` returns a `Uint8Array` (mapped from
    /// `Vec<u8>` by wasm-bindgen). Verify the magic header.
    #[wasm_bindgen_test]
    fn render_pdf_with_options_undefined_returns_pdf() {
        let result = render_pdf_with_options(MINIMAL_INPUT, JsValue::UNDEFINED).unwrap();
        assert!(result.len() > 4);
        assert_eq!(&result[0..4], b"%PDF");
    }

    /// `version()` returns a non-empty string through the `JsValue`
    /// boundary.
    #[wasm_bindgen_test]
    fn version_returns_nonempty_string() {
        let v = version();
        assert!(!v.is_empty());
    }

    /// Smoke test that the `start` panic hook function exists and is
    /// callable through the wasm-bindgen boundary. We don't trigger an
    /// actual panic (it would abort the test runner), but calling
    /// `set_once` a second time is safe and exercises the symbol.
    #[wasm_bindgen_test]
    fn start_panic_hook_callable() {
        start();
        // Calling twice exercises the `Once` semantics in
        // `console_error_panic_hook` and confirms the symbol resolves.
        start();
    }

    /// Build a sentinel input that GUARANTEES the renderer emits at
    /// least one warning, render it, and assert that `console.warn`
    /// was called with the expected prefix at least once. This is the
    /// regression test for #1051 (warnings going to `eprintln!` and
    /// silently disappearing in WASM contexts).
    ///
    /// Sentinel: a `{transpose: 100}` directive in the source combined
    /// with a CLI `transpose: 100` exceeds the `i8` range (200 > 127),
    /// so `chordsketch_core::transpose::combine_transpose` saturates
    /// and the renderer pushes a `transpose offset ... clamped to ...`
    /// warning. See `crates/render-text/src/lib.rs` line 124-131.
    /// (The HTML and PDF renderers have identical saturation paths;
    /// we exercise text here because it has the smallest output.)
    ///
    /// Implementation note: we monkey-patch `console.warn` for the
    /// duration of the test, capture each call into a JS array, then
    /// restore the original. We assert BOTH `captured.length() >= 1`
    /// (so a future regression that drops warnings on the floor would
    /// fail loudly — see #1111) AND that every captured entry starts
    /// with the `chordsketch:` prefix.
    #[wasm_bindgen_test]
    fn render_forwards_warnings_to_console_warn() {
        // Save the original `console.warn`.
        let console = js_sys::Reflect::get(&js_sys::global(), &"console".into()).unwrap();
        let original_warn = js_sys::Reflect::get(&console, &"warn".into()).unwrap();

        // Install a capturing replacement: a JS function that pushes
        // its first argument into a known JS array.
        let captured = Array::new();
        let captured_clone = captured.clone();
        let capture_fn = wasm_bindgen::closure::Closure::wrap(Box::new(move |msg: JsValue| {
            captured_clone.push(&msg);
        })
            as Box<dyn FnMut(JsValue)>);
        Reflect::set(
            &console,
            &"warn".into(),
            capture_fn.as_ref().unchecked_ref(),
        )
        .unwrap();

        // Sentinel: in-file {transpose: 100} + CLI transpose: 100 = 200,
        // which exceeds i8 range and produces a saturation warning.
        let opts = js_sys::Object::new();
        Reflect::set(&opts, &"transpose".into(), &JsValue::from(100)).unwrap();
        let _ = render_text_with_options("{title: T}\n{transpose: 100}\n[C]Hello", opts.into())
            .unwrap();

        // Restore the original `console.warn`.
        Reflect::set(&console, &"warn".into(), &original_warn).unwrap();
        // Drop the closure to free the wasm-bindgen reference.
        drop(capture_fn);

        assert!(
            captured.length() >= 1,
            "expected at least one console.warn call from the saturation-triggering input, got {}",
            captured.length()
        );
        for i in 0..captured.length() {
            let entry = captured.get(i).as_string().unwrap_or_default();
            assert!(
                entry.starts_with("chordsketch:"),
                "console.warn entry should start with 'chordsketch:', got: {entry}"
            );
        }
    }
}
