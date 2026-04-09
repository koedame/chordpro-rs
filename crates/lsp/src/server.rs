//! LSP [`Backend`] implementation.
//!
//! Implements the [`LanguageServer`] trait from `tower-lsp`. Only the
//! capabilities required for parse-error diagnostics are declared; all other
//! requests are left to their default (not-implemented) response so that
//! editors degrade gracefully.

use chordsketch_core::parse_multi_lenient;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    InitializeParams, InitializeResult, InitializedParams, ServerCapabilities,
    TextDocumentSyncKind,
};
use tower_lsp::{Client, LanguageServer};

use crate::convert::parse_error_to_diagnostic;

/// The LSP server backend.
pub struct Backend {
    client: Client,
}

impl Backend {
    /// Creates a new `Backend` with the given `tower-lsp` client.
    #[must_use]
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Re-parses `text` and publishes diagnostics for `uri`.
    async fn publish_diagnostics(&self, uri: tower_lsp::lsp_types::Url, text: &str) {
        self.client
            .publish_diagnostics(uri, diagnostics_for(text), None)
            .await;
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
                text_document_sync: Some(tower_lsp::lsp_types::TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
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
        self.publish_diagnostics(uri, &change.text).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        // Clear diagnostics when the document is closed.
        self.client
            .publish_diagnostics(params.text_document.uri, vec![], None)
            .await;
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
        // "[C" on line 2 — the closing `]` is missing.
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
