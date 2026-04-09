//! `chordsketch-lsp` — Language Server Protocol server for ChordPro files.
//!
//! # Usage
//!
//! ```text
//! chordsketch-lsp [--stdio] [--version] [--help]
//! ```
//!
//! The server communicates over standard input/output (stdio transport).
//! The `--stdio` flag is accepted for compatibility with editor launcher
//! conventions but is otherwise a no-op — stdio is always the transport.
//! `--version` / `-V` prints the binary version. `--help` / `-h` prints
//! usage information.
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

mod completion;
mod convert;
mod server;

use server::Backend;
use tower_lsp::{LspService, Server};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    // argv[0] is the binary name; iterate over the rest.
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--stdio" => {
                // Expected: stdio is always the transport; accepted as a no-op.
                // Flags are processed independently — order does not matter.
            }
            "--version" | "-V" => {
                println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                return;
            }
            "--help" | "-h" => {
                // Help text goes to stdout per POSIX convention.
                println!("Usage: chordsketch-lsp [--stdio] [--version] [--help]");
                println!();
                println!("Language Server Protocol server for ChordPro files.");
                println!("Communicates over stdio (--stdio is accepted but is the default).");
                println!();
                println!("Options:");
                println!("  --stdio       Use stdio transport (default, accepted as a no-op)");
                println!("  --version, -V Print version and exit");
                println!("  --help, -h    Print this help and exit");
                println!();
                println!("Set RUST_LOG=debug for verbose logging (written to stderr).");
                return;
            }
            unknown => {
                eprintln!("error: unrecognized argument '{unknown}'");
                eprintln!("Usage: chordsketch-lsp [--stdio] [--version] [--help]");
                std::process::exit(1);
            }
        }
    }

    // Write structured logs to stderr so they don't interfere with the
    // LSP JSON-RPC stream on stdout. RUST_LOG controls the log level
    // (defaults to "info" if not set).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
