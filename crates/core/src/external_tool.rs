//! Utilities for detecting and invoking external tools.
//!
//! The delegate rendering pipeline (ABC, Lilypond) requires external tools
//! to convert notation into SVG/PNG. This module provides functions to check
//! whether these tools are installed and accessible on the current system,
//! and to invoke them on delegate environment content.
//!
//! All detection functions are safe to call even when the tools are not
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

/// Invoke `abc2svg` on ABC notation content and return the rendered SVG fragment.
///
/// Writes `abc_content` to a temporary file, runs `abc2svg tosvg.js <file>`,
/// and extracts the `<body>` content from the HTML output. The returned string
/// is an HTML fragment containing one or more `<svg>` elements suitable for
/// direct embedding.
///
/// # Errors
///
/// Returns an error string if:
/// - The temporary file cannot be written
/// - `abc2svg` is not available or fails to execute
/// - The output does not contain a recognizable `<body>` section
pub fn invoke_abc2svg(abc_content: &str) -> Result<String, String> {
    let tmp_dir = std::env::temp_dir();
    let tmp_path = tmp_dir.join(format!("chordpro_abc_{}.abc", std::process::id()));

    std::fs::write(&tmp_path, abc_content)
        .map_err(|e| format!("failed to write temp file: {e}"))?;

    let output = Command::new("abc2svg")
        .arg("tosvg.js")
        .arg(&tmp_path)
        .output()
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            format!("failed to invoke abc2svg: {e}")
        })?;

    let _ = std::fs::remove_file(&tmp_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("abc2svg exited with error: {stderr}"));
    }

    let html = String::from_utf8_lossy(&output.stdout);
    extract_body_content(&html)
        .ok_or_else(|| "failed to extract SVG from abc2svg output".to_string())
}

/// Extract content after `<body>` from an HTML string.
///
/// Looks for a `</body>` closing tag first; if absent (abc2svg omits it),
/// falls back to `</html>`. Returns the trimmed content between the opening
/// tag and whichever closing marker is found.
fn extract_body_content(html: &str) -> Option<String> {
    let body_open = html.find("<body>")?;
    let content_start = body_open + "<body>".len();
    let content_end = html
        .find("</body>")
        .or_else(|| html.find("</html>"))
        .unwrap_or(html.len());
    if content_start > content_end {
        return None;
    }
    Some(html[content_start..content_end].trim().to_string())
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
    fn extract_body_content_basic() {
        let html = "<html><body>\n<svg>hello</svg>\n</body></html>";
        let result = super::extract_body_content(html);
        assert_eq!(result, Some("<svg>hello</svg>".to_string()));
    }

    #[test]
    fn extract_body_content_missing_body() {
        let html = "<html><div>no body tag</div></html>";
        assert!(super::extract_body_content(html).is_none());
    }

    #[test]
    fn extract_body_content_empty_body() {
        let html = "<html><body></body></html>";
        assert_eq!(super::extract_body_content(html), Some(String::new()));
    }

    #[test]
    #[ignore]
    fn abc2svg_detection() {
        assert!(has_abc2svg(), "abc2svg not found in PATH");
    }

    #[test]
    #[ignore]
    fn invoke_abc2svg_produces_svg() {
        let abc = "X:1\nT:Test\nM:4/4\nK:C\nCDEF|GABc|\n";
        let result = invoke_abc2svg(abc);
        assert!(result.is_ok(), "invoke_abc2svg failed: {:?}", result.err());
        let svg = result.unwrap();
        assert!(svg.contains("<svg"), "output should contain SVG element");
    }

    #[test]
    fn invoke_abc2svg_fails_gracefully_without_tool() {
        if has_abc2svg() {
            // Skip if abc2svg is available — this test is for the missing-tool path.
            return;
        }
        let result = invoke_abc2svg("X:1\n");
        assert!(result.is_err());
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
