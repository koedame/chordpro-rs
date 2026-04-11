// @chordsketch/node — TypeScript declarations.
//
// Mirrors the public API defined by `crates/napi/src/lib.rs` #[napi] items.
// Keep in sync with the Rust source: every exported function and object
// over there must have a matching declaration here.

/**
 * Rendering options accepted by the `*WithOptions` functions.
 *
 * Matches the `RenderOptions` struct in `crates/napi/src/lib.rs`.
 */
export interface RenderOptions {
  /**
   * Semitone transposition offset. Defaults to 0.
   *
   * Any integer is accepted, but values outside the i8 range (-128..=127)
   * are clamped before the underlying renderer reduces modulo 12.
   */
  transpose?: number;

  /**
   * Configuration preset name (e.g. "guitar", "ukulele") or an inline
   * RRJSON configuration string.
   */
  config?: string;
}

/**
 * A single validation issue reported by `validate`.
 */
export interface ValidationError {
  /** One-based line number of the issue within the input. */
  line: number;
  /** Column offset (byte) within the line, one-based. */
  column: number;
  /** Human-readable description of the issue. */
  message: string;
}

/** Returns the version string baked into the compiled Rust crate. */
export function version(): string;

/** Renders the ChordPro source to plain text. */
export function renderText(source: string): string;

/**
 * Renders the ChordPro source to plain text with rendering options applied.
 */
export function renderTextWithOptions(source: string, options: RenderOptions): string;

/** Renders the ChordPro source to HTML. */
export function renderHtml(source: string): string;

/**
 * Renders the ChordPro source to HTML with rendering options applied.
 */
export function renderHtmlWithOptions(source: string, options: RenderOptions): string;

/** Renders the ChordPro source to a PDF document, returned as a Buffer. */
export function renderPdf(source: string): Buffer;

/**
 * Renders the ChordPro source to a PDF document with rendering options
 * applied.
 */
export function renderPdfWithOptions(source: string, options: RenderOptions): Buffer;

/**
 * Validates a ChordPro source document and returns a list of issues. An
 * empty array indicates the document parses cleanly.
 */
export function validate(source: string): ValidationError[];
