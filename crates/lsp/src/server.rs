//! LSP [`Backend`] implementation.
//!
//! Implements the [`LanguageServer`] trait from `tower-lsp`. Only the
//! capabilities required for parse-error diagnostics and text completion are
//! declared; all other requests are left to their default (not-implemented)
//! response so that editors degrade gracefully.

use std::collections::HashMap;
use std::sync::Arc;

use chordsketch_core::parse_multi_lenient;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, InitializedParams, PositionEncodingKind,
    ServerCapabilities, TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer};

use crate::completion::{
    CompletionContext, chord_items, detect_context, directive_items, meta_key_items,
};
use crate::convert::parse_error_to_diagnostic;

/// Maximum number of open documents tracked for completion.
///
/// When the limit is reached an arbitrary entry is evicted to stay within the
/// cap. In practice, editors open and close documents regularly so the map
/// stays small.
const MAX_DOCUMENTS: usize = 256;

/// The LSP server backend.
pub struct Backend {
    client: Client,
    /// Open document texts, keyed by URI. Needed for completion.
    documents: Arc<Mutex<HashMap<Url, String>>>,
}

impl Backend {
    /// Creates a new `Backend` with the given `tower-lsp` client.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Re-parses `text` and publishes diagnostics for `uri`.
    async fn publish_diagnostics(&self, uri: Url, text: &str) {
        self.client
            .publish_diagnostics(uri, diagnostics_for(text), None)
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
/// converts each `ParseError` to an LSP `Diagnostic`. Extracted as a free
/// function so it can be unit-tested independently of the LSP transport.
#[must_use]
pub fn diagnostics_for(text: &str) -> Vec<Diagnostic> {
    parse_multi_lenient(text)
        .all_errors()
        .into_iter()
        .map(parse_error_to_diagnostic)
        .collect()
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Declare UTF-8 encoding so clients send byte offsets rather
                // than UTF-16 code-unit offsets, avoiding off-by-one errors
                // for documents that contain non-ASCII characters.
                position_encoding: Some(PositionEncodingKind::UTF8),
                text_document_sync: Some(tower_lsp::lsp_types::TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        "{".to_string(),
                        "[".to_string(),
                        // Space triggers completion inside `{meta: …}` after the colon+space.
                        " ".to_string(),
                    ]),
                    ..Default::default()
                }),
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

        let docs = self.documents.lock().await;
        let Some(text) = docs.get(uri) else {
            return Ok(None);
        };

        // Get the line at the cursor (0-based line index).
        let line = text.lines().nth(pos.line as usize).unwrap_or("");

        // The server declared UTF-8 position encoding, so `pos.character` is a
        // byte offset into the line. Convert to a char count for `detect_context`.
        let byte_col = pos.character as usize;
        let col = line
            .char_indices()
            .take_while(|(byte_idx, _)| *byte_idx < byte_col)
            .count();

        let items = match detect_context(line, col) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::DiagnosticSeverity;

    #[test]
    fn diagnostics_for_valid_document_returns_empty() {
        let text = "[C]Hello [G]world\n{title: My Song}\n";
        let diags = diagnostics_for(text);
        assert!(
            diags.is_empty(),
            "expected no diagnostics for valid ChordPro, got: {diags:?}"
        );
    }

    #[test]
    fn diagnostics_for_unclosed_directive_returns_error() {
        // Missing closing `}` — the parser reports a structural error.
        let text = "{title: Broken\n[C]Hello\n";
        let diags = diagnostics_for(text);
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
        let diags = diagnostics_for(text);
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
        assert!(!diagnostics_for(broken).is_empty());
        assert!(diagnostics_for(fixed).is_empty());
    }

    #[test]
    fn diagnostics_for_multi_song_collects_all_errors() {
        // Two song segments each with a structural error.
        let text = "{title: A\n[C\n{new_song}\n{title: B\n[G\n";
        let diags = diagnostics_for(text);
        assert!(
            diags.len() >= 2,
            "expected errors from both song segments, got {}: {diags:?}",
            diags.len()
        );
    }
}
