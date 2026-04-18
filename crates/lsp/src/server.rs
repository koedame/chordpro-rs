//! LSP [`Backend`] implementation.
//!
//! Implements the [`LanguageServer`] trait from `tower-lsp`. Capabilities
//! declared:
//! - Diagnostics (parse errors)
//! - Text completion (directive names, chord names, metadata keys)
//! - Hover (chord diagrams, directive documentation)
//! - Document formatting
//!
//! All other requests are left to their default (not-implemented) response so
//! that editors degrade gracefully.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use chordsketch_core::parse_multi_lenient;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, Hover, HoverContents, HoverParams, HoverProviderCapability,
    InitializeParams, InitializeResult, InitializedParams, MarkupContent, MarkupKind, OneOf,
    Position, Range, ServerCapabilities, TextDocumentSyncKind, TextEdit, Url,
};
use tower_lsp::{Client, LanguageServer};

use crate::completion::{
    CompletionContext, chord_items, detect_context, directive_items, meta_key_items,
};
use crate::convert::parse_error_to_diagnostic;
use crate::encoding::{
    PositionEncoding, char_idx_to_lsp_char, line_length, lsp_char_to_char_idx, negotiate_encoding,
};
use crate::hover::{
    HoverContext, chord_hover_markdown, detect_hover_context, directive_hover_markdown,
    hover_token_span,
};

/// Maximum number of open documents tracked for completion.
///
/// When the limit is reached an arbitrary entry is evicted to stay within the
/// cap. In practice, editors open and close documents regularly so the map
/// stays small.
const MAX_DOCUMENTS: usize = 256;

// Sentinel values for `Backend.encoding`. UTF-16 is the spec-mandated
// fallback when the client omits `general.positionEncodings`, so it is
// also the pre-initialize default.
const ENCODING_UTF16: u8 = 0;
const ENCODING_UTF8: u8 = 1;

/// The LSP server backend.
pub struct Backend {
    client: Client,
    /// Open document texts, keyed by URI. Needed for completion.
    documents: Arc<Mutex<HashMap<Url, String>>>,
    /// Position encoding negotiated during `initialize`. Stored as an atomic
    /// because `initialize` is called through `&self` (tower-lsp) and all
    /// subsequent request handlers read it concurrently. Written exactly once.
    encoding: AtomicU8,
}

impl Backend {
    /// Creates a new `Backend` with the given `tower-lsp` client.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(Mutex::new(HashMap::new())),
            encoding: AtomicU8::new(ENCODING_UTF16),
        }
    }

    /// Returns the negotiated position encoding. Before `initialize` has
    /// been processed the value is UTF-16 (the LSP 3.17 default).
    ///
    /// Uses `Acquire` ordering to pair with the `Release` store in
    /// `initialize`, making the write-once publish semantics explicit and
    /// safe if the runtime is ever switched from `current_thread` to a
    /// multi-threaded one.
    fn encoding(&self) -> PositionEncoding {
        match self.encoding.load(Ordering::Acquire) {
            ENCODING_UTF8 => PositionEncoding::Utf8,
            _ => PositionEncoding::Utf16,
        }
    }

    /// Re-parses `text` and publishes diagnostics for `uri`.
    async fn publish_diagnostics(&self, uri: Url, text: &str) {
        let encoding = self.encoding();
        self.client
            .publish_diagnostics(uri, diagnostics_for(text, encoding), None)
            .await;
    }

    /// Stores `text` for `uri`, evicting an arbitrary entry if the cap is exceeded.
    async fn store_document(&self, uri: Url, text: String) {
        let mut docs = self.documents.lock().await;
        if docs.len() >= MAX_DOCUMENTS && !docs.contains_key(&uri) {
            // Evict an arbitrary entry to stay within the cap.
            if let Some(key) = docs.keys().next().cloned() {
                docs.remove(&key);
            }
        }
        docs.insert(uri, text);
    }
}

/// Parses `text` and returns LSP diagnostics for every parse error found.
///
/// This is the core mapping function: it drives `parse_multi_lenient` and
/// converts each `ParseError` to an LSP `Diagnostic` using the negotiated
/// position `encoding`. Extracted as a free function so it can be
/// unit-tested independently of the LSP transport.
#[must_use]
pub fn diagnostics_for(text: &str, encoding: PositionEncoding) -> Vec<Diagnostic> {
    parse_multi_lenient(text)
        .all_errors()
        .into_iter()
        .map(|e| parse_error_to_diagnostic(e, text, encoding))
        .collect()
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        // LSP 3.17 requires the server to pick an encoding from the client's
        // advertised list (or fall back to UTF-16 when absent). Storing the
        // choice lets every request handler produce offsets in matching units.
        let chosen = negotiate_encoding(&params);
        // `Release` pairs with `Acquire` in `Backend::encoding()` to publish
        // the chosen encoding to every subsequent request handler. `initialize`
        // is called exactly once, so this is a write-once publish.
        self.encoding.store(
            match chosen {
                PositionEncoding::Utf8 => ENCODING_UTF8,
                PositionEncoding::Utf16 => ENCODING_UTF16,
            },
            Ordering::Release,
        );

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                position_encoding: Some(chosen.to_kind()),
                text_document_sync: Some(tower_lsp::lsp_types::TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "{".to_string(),
                        "[".to_string(),
                        // Metadata-key completion (`{meta: <key>`) is available via
                        // manual invocation (Ctrl+Space); space is not registered as a
                        // trigger to avoid firing on every space in the document.
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        self.client
            .log_message(
                tower_lsp::lsp_types::MessageType::INFO,
                "chordsketch-lsp initialized",
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        self.store_document(uri.clone(), text.clone()).await;
        self.publish_diagnostics(uri, &text).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        // Full sync: exactly one TextDocumentContentChangeEvent per notification.
        // Use `next()` per spec; log a warning if the client sends an empty list.
        let Some(change) = params.content_changes.into_iter().next() else {
            self.client
                .log_message(
                    tower_lsp::lsp_types::MessageType::WARNING,
                    "didChange received with no content changes",
                )
                .await;
            return;
        };
        self.store_document(uri.clone(), change.text.clone()).await;
        self.publish_diagnostics(uri, &change.text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.lock().await.remove(&uri);
        // Clear diagnostics when the document is closed.
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let pos = &params.text_document_position.position;
        let encoding = self.encoding();

        // Extract the relevant line under a short critical section, then drop
        // the lock before calling detect_context and the item builders so that
        // concurrent did_open/did_change/did_close are not blocked.
        let line_owned: String = {
            let docs = self.documents.lock().await;
            let Some(text) = docs.get(uri) else {
                return Ok(None);
            };
            text.lines().nth(pos.line as usize).unwrap_or("").to_owned()
        };

        // `pos.character` is in the negotiated encoding's units; convert it
        // to a 0-based character index for `detect_context`.
        let col = lsp_char_to_char_idx(&line_owned, pos.character, encoding);

        let items = match detect_context(&line_owned, col) {
            CompletionContext::DirectiveName { prefix } => directive_items(&prefix),
            CompletionContext::MetadataKey { prefix } => meta_key_items(&prefix),
            CompletionContext::ChordName { prefix } => chord_items(&prefix),
            CompletionContext::None => return Ok(None),
        };

        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = &params.text_document_position_params.position;
        let encoding = self.encoding();

        let line_owned: String = {
            let docs = self.documents.lock().await;
            let Some(text) = docs.get(uri) else {
                return Ok(None);
            };
            text.lines().nth(pos.line as usize).unwrap_or("").to_owned()
        };

        // `pos.character` is in the negotiated encoding's units; convert to a
        // 0-based character index for the hover-context helpers.
        let col = lsp_char_to_char_idx(&line_owned, pos.character, encoding);

        let markdown = match detect_hover_context(&line_owned, col) {
            HoverContext::ChordName { name } => chord_hover_markdown(&name),
            HoverContext::DirectiveName { name } => directive_hover_markdown(&name),
            HoverContext::NoContext => None,
        };

        let range = hover_token_span(&line_owned, col).map(|(start_char, end_char)| {
            // Convert char indices back to `Position.character` in the
            // negotiated encoding.
            let start = char_idx_to_lsp_char(&line_owned, start_char, encoding);
            let end = char_idx_to_lsp_char(&line_owned, end_char, encoding);
            Range {
                start: Position {
                    line: pos.line,
                    character: start,
                },
                end: Position {
                    line: pos.line,
                    character: end,
                },
            }
        });

        Ok(markdown.map(|md| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: md,
            }),
            range,
        }))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let encoding = self.encoding();

        // Extract the document text under a short lock scope, then drop the
        // lock before the (potentially expensive) formatting pass.
        let text = {
            let docs = self.documents.lock().await;
            match docs.get(uri) {
                Some(t) => t.clone(),
                None => return Ok(None),
            }
        };

        let formatted = chordsketch_core::formatter::format(
            &text,
            &chordsketch_core::formatter::FormatOptions::default(),
        );

        // No-op if already formatted.
        if formatted == text {
            return Ok(Some(vec![]));
        }

        // Replace the entire document with a single TextEdit. The `character`
        // field of the end position must be expressed in the negotiated
        // encoding's units.
        let end = document_end_position(&text, encoding);
        let edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end,
            },
            new_text: formatted,
        };
        Ok(Some(vec![edit]))
    }
}

/// Compute the end [`Position`] of `text` in the negotiated `encoding`.
///
/// Used to build a full-document `TextEdit` range. Supports LF (`\n`),
/// CRLF (`\r\n`), and CR-only (`\r`) line endings. Each line break advances
/// the line counter; the character offset is the length of the final line
/// expressed in the negotiated encoding's units.
fn document_end_position(text: &str, encoding: PositionEncoding) -> Position {
    let mut line: u32 = 0;
    let mut last_newline_byte: usize = 0;
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        match bytes[i] {
            b'\n' => {
                line += 1;
                last_newline_byte = i + 1;
            }
            // A bare \r (CR-only) is a line break. A \r\n pair is handled
            // by the \n branch on the next iteration, so skip the \r here.
            b'\r' if i + 1 >= len || bytes[i + 1] != b'\n' => {
                line += 1;
                last_newline_byte = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let last_line = &text[last_newline_byte..];
    let character = line_length(last_line, encoding);
    Position { line, character }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::DiagnosticSeverity;

    #[test]
    fn diagnostics_for_valid_document_returns_empty() {
        let text = "[C]Hello [G]world\n{title: My Song}\n";
        let diags = diagnostics_for(text, PositionEncoding::Utf16);
        assert!(
            diags.is_empty(),
            "expected no diagnostics for valid ChordPro, got: {diags:?}"
        );
    }

    #[test]
    fn diagnostics_for_unclosed_directive_returns_error() {
        // Missing closing `}` — the parser reports a structural error.
        let text = "{title: Broken\n[C]Hello\n";
        let diags = diagnostics_for(text, PositionEncoding::Utf16);
        assert!(
            !diags.is_empty(),
            "expected at least one diagnostic for unclosed directive"
        );
        assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diags[0].source.as_deref(), Some("chordsketch"));
    }

    #[test]
    fn diagnostics_for_unclosed_chord_returns_error_at_correct_line() {
        // Line 2 contains `[C Hello world` — the chord bracket is opened with
        // `[` but never closed with `]`, producing a structural parse error.
        let text = "{title: Test}\n[C Hello world\n";
        let diags = diagnostics_for(text, PositionEncoding::Utf16);
        assert!(
            !diags.is_empty(),
            "expected at least one diagnostic for unclosed chord bracket"
        );
        // Parser positions are 1-based; LSP Range is 0-based.
        // The chord starts on line 2 (1-based) → line 1 (0-based).
        assert_eq!(diags[0].range.start.line, 1);
    }

    #[test]
    fn diagnostics_for_clears_on_fix() {
        // Start with an error, then verify the fixed version has no errors.
        let broken = "{title: Broken\n";
        let fixed = "{title: Fixed}\n";
        assert!(!diagnostics_for(broken, PositionEncoding::Utf16).is_empty());
        assert!(diagnostics_for(fixed, PositionEncoding::Utf16).is_empty());
    }

    #[test]
    fn diagnostics_for_multi_song_collects_all_errors() {
        // Two song segments each with a structural error.
        let text = "{title: A\n[C\n{new_song}\n{title: B\n[G\n";
        let diags = diagnostics_for(text, PositionEncoding::Utf16);
        assert!(
            diags.len() >= 2,
            "expected errors from both song segments, got {}: {diags:?}",
            diags.len()
        );
    }

    // --- document_end_position ---

    #[test]
    fn end_pos_lf_only_utf8() {
        // Standard LF line endings, ASCII-only.
        let pos = document_end_position("line1\nline2\nline3", PositionEncoding::Utf8);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 5); // len("line3") bytes
    }

    #[test]
    fn end_pos_lf_only_utf16() {
        // Same ASCII text — UTF-16 code units equal char count (all BMP).
        let pos = document_end_position("line1\nline2\nline3", PositionEncoding::Utf16);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn end_pos_crlf_utf8() {
        // CRLF line endings — \r is skipped, \n advances the counter.
        let pos = document_end_position("line1\r\nline2\r\nline3", PositionEncoding::Utf8);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn end_pos_cr_only_utf8() {
        // CR-only line endings (old Mac OS 9 style).
        let pos = document_end_position("line1\rline2\rline3", PositionEncoding::Utf8);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn end_pos_empty_utf8() {
        let pos = document_end_position("", PositionEncoding::Utf8);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn end_pos_trailing_newline_utf8() {
        // Trailing \n means the final line is empty (character = 0).
        let pos = document_end_position("line1\nline2\n", PositionEncoding::Utf8);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn end_pos_trailing_crlf_utf8() {
        // Trailing \r\n also yields character = 0 on the final line.
        let pos = document_end_position("line1\r\nline2\r\n", PositionEncoding::Utf8);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 0);
    }

    #[test]
    fn end_pos_non_ascii_last_line_utf8_bytes() {
        // "Ré" is 3 UTF-8 bytes.
        let pos = document_end_position("line1\nRé", PositionEncoding::Utf8);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn end_pos_non_ascii_last_line_utf16_code_units() {
        // "Ré" is 2 UTF-16 code units (both BMP).
        let pos = document_end_position("line1\nRé", PositionEncoding::Utf16);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 2);
    }

    #[test]
    fn end_pos_astral_last_line_utf16_surrogate_pair() {
        // U+1F3B8 GUITAR is a surrogate pair in UTF-16 (2 units).
        let pos = document_end_position("line1\n\u{1F3B8}", PositionEncoding::Utf16);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 2);
    }
}
