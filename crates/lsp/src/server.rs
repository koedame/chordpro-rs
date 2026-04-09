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
        let result = parse_multi_lenient(text);
        let diagnostics: Vec<Diagnostic> = result
            .all_errors()
            .into_iter()
            .map(parse_error_to_diagnostic)
            .collect();
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }
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
