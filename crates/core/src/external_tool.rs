//! Utilities for detecting external tool availability.
//!
//! The delegate rendering pipeline (ABC, Lilypond) requires external tools
//! to convert notation into SVG/PNG. This module provides functions to check
//! whether these tools are installed and accessible on the current system.
//!
//! All functions in this module are safe to call even when the tools are not
//! installed — they return `false` rather than panicking.

use std::process::Command;

/// Checks whether a command is available by attempting to run it.
///
/// Returns `true` if the command can be executed (the process starts
/// successfully). Returns `false` if the command is not found or cannot
/// be executed.
///
/// # Examples
///
/// ```no_run
/// use chordpro_core::external_tool::is_available;
///
/// if is_available("lilypond") {
///     println!("Lilypond is installed");
/// }
/// ```
#[must_use]
pub fn is_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// Returns `true` if `abc2svg` is available.
#[must_use]
pub fn has_abc2svg() -> bool {
    is_available("abc2svg")
}

/// Returns `true` if `lilypond` is available.
#[must_use]
pub fn has_lilypond() -> bool {
    is_available("lilypond")
}

/// Returns `true` if the Perl `chordpro` reference implementation is available.
#[must_use]
pub fn has_perl_chordpro() -> bool {
    is_available("chordpro")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonexistent_tool_returns_false() {
        assert!(!is_available("this-command-definitely-does-not-exist-xyz"));
    }

    // The following tests are #[ignore] because they depend on external tools.
    // Run with: cargo test -p chordpro-core -- --ignored

    #[test]
    #[ignore]
    fn abc2svg_detection() {
        assert!(has_abc2svg(), "abc2svg not found in PATH");
    }

    #[test]
    #[ignore]
    fn lilypond_detection() {
        assert!(has_lilypond(), "lilypond not found in PATH");
    }

    #[test]
    #[ignore]
    fn perl_chordpro_detection() {
        assert!(has_perl_chordpro(), "chordpro (Perl) not found in PATH");
    }
}
