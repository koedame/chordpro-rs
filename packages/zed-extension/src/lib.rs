use zed_extension_api::{self as zed, Result};

struct ChordProExtension;

impl zed::Extension for ChordProExtension {
    fn new() -> Self {
        ChordProExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        // Look for chordsketch-lsp in the PATH or worktree
        let path = worktree
            .which("chordsketch-lsp")
            .ok_or_else(|| "chordsketch-lsp not found in PATH. Install it with: cargo install chordsketch-lsp".to_string())?;

        Ok(zed::Command {
            command: path,
            args: vec!["--stdio".to_string()],
            env: Default::default(),
        })
    }
}

zed::register_extension!(ChordProExtension);
