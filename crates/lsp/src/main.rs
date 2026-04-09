//! `chordsketch-lsp` — Language Server Protocol server for ChordPro files.
//!
//! # Usage
//!
//! ```text
//! chordsketch-lsp [--stdio]
//! ```
//!
//! The server communicates over standard input/output (stdio transport).
//! The `--stdio` flag is accepted for compatibility with editor launcher
//! conventions but is otherwise a no-op — stdio is always the transport.
//!
//! # Editor configuration
//!
//! In VS Code (manual setup, before the extension ships):
//!
//! ```json
//! {
//!   "languageServer": {
//!     "command": "chordsketch-lsp",
//!     "args": ["--stdio"]
//!   }
//! }
//! ```

mod convert;
mod server;

use server::Backend;
use tower_lsp::{LspService, Server};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Accept --stdio for editor compatibility; stdio is always the transport.
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("Usage: chordsketch-lsp [--stdio]");
        eprintln!();
        eprintln!("Language Server Protocol server for ChordPro files.");
        eprintln!("Communicates over stdio (--stdio is accepted but is the default).");
        return;
    }

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
