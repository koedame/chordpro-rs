//! Utilities for detecting and invoking external tools.
//!
//! The delegate rendering pipeline (ABC, Lilypond) requires external tools
//! to convert notation into SVG/PNG. This module provides functions to check
//! whether these tools are installed and accessible on the current system,
//! and to invoke them on delegate environment content.
//!
//! All detection functions are safe to call even when the tools are not
//! installed — they return `false` rather than panicking.

use std::fs::OpenOptions;
use std::io::Write;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// Checks whether a command is available by attempting to run it.
///
/// Returns `true` if the command can be executed (the process starts
/// successfully). Returns `false` if the command is not found or cannot
/// be executed.
///
/// # Security
///
/// The `command` parameter is passed directly to [`std::process::Command::new`].
/// Callers **must not** pass user-controlled or untrusted input as the command
/// name, as this could lead to arbitrary command execution. This function is
/// intended only for checking hardcoded tool names (e.g., `"lilypond"`,
/// `"abc2svg"`).
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

/// Global counter for generating unique temp file names within a process.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique temporary file path with the given prefix and extension.
///
/// Uses PID + monotonic counter to avoid collisions across threads
/// and sequential invocations within the same process.
fn unique_temp_path(prefix: &str, ext: &str) -> std::path::PathBuf {
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("{prefix}_{pid}_{counter}.{ext}"))
}

/// Write content to a new temporary file using `O_EXCL` semantics.
///
/// The file is created with `create_new(true)` which maps to `O_CREAT | O_EXCL`
/// on Unix, preventing symlink attacks and ensuring the file did not already exist.
fn write_temp_file_exclusive(path: &std::path::Path, content: &str) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("failed to create temp file: {e}"))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("failed to write temp file: {e}"))?;
    Ok(())
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
    let sanitized = sanitize_abc_content(abc_content);

    let tmp_path = unique_temp_path("chordpro_abc", "abc");

    write_temp_file_exclusive(&tmp_path, &sanitized)?;

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

/// Strip dangerous JavaScript directives from ABC notation content.
///
/// abc2svg supports embedded JavaScript via `%%beginjs`/`%%endjs` blocks and
/// `%%javascript` directives. When processing untrusted `.cho` files, these
/// directives could allow arbitrary code execution in the Node.js runtime.
/// This function removes such directives to prevent code injection.
fn sanitize_abc_content(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_js_block = false;

    for line in input.lines() {
        let trimmed = line.trim();

        if in_js_block {
            if trimmed.eq_ignore_ascii_case("%%endjs") {
                in_js_block = false;
            }
            continue;
        }

        if trimmed.eq_ignore_ascii_case("%%beginjs") {
            in_js_block = true;
            continue;
        }

        // %%javascript <code> is a single-line JS directive (case-insensitive).
        if trimmed.len() >= 12 && trimmed[..12].eq_ignore_ascii_case("%%javascript") {
            let after = &trimmed[12..];
            if after.is_empty() || after.starts_with(' ') || after.starts_with('\t') {
                continue;
            }
        }

        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(line);
    }

    output
}

/// Dangerous Scheme function names that should be stripped from Lilypond content.
///
/// These functions can escape the `-dsafe` sandbox or access the filesystem
/// if a future Lilypond version has a `-dsafe` bypass.
const DANGEROUS_SCHEME_FUNCTIONS: &[&str] = &[
    "system",
    "getenv",
    "open-input-file",
    "open-output-file",
    "open-file",
    "primitive-load",
    "primitive-load-path",
    "eval-string",
    "ly:gulp-file",
    "ly:system",
];

/// Strip dangerous Scheme directives from Lilypond content as defense-in-depth.
///
/// Lilypond's `-dsafe` flag sandboxes the embedded Scheme interpreter, but as a
/// secondary layer of protection, this function removes lines containing known
/// dangerous Scheme function calls (e.g., `#(system ...)`, `#(getenv ...)`).
fn sanitize_lilypond_content(input: &str) -> String {
    let mut output = String::with_capacity(input.len());

    for line in input.lines() {
        if line_contains_dangerous_scheme(line) {
            continue;
        }
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(line);
    }

    output
}

/// Check if a line contains a dangerous Scheme function call.
///
/// Matches patterns like `#(system ...)`, `#(ly:system ...)`, `$(system ...)`,
/// or `$(ly:system ...)` anywhere in the line, case-insensitively. In Lilypond
/// 2.18+, `$` is an alternative to `#` for Scheme evaluation.
fn line_contains_dangerous_scheme(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    for sigil in &["#(", "$("] {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(sigil) {
            let abs_pos = search_from + pos;
            let after = &lower[abs_pos + sigil.len()..];
            let trimmed = after.trim_start();
            for &func in DANGEROUS_SCHEME_FUNCTIONS {
                if trimmed.starts_with(func) {
                    return true;
                }
            }
            search_from = abs_pos + sigil.len();
        }
    }
    false
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

/// Invoke `lilypond` on Lilypond notation content and return the rendered SVG.
///
/// Creates a temporary directory, writes `ly_content` to a `.ly` file, runs
/// `lilypond --svg -o <prefix> <file>`, and reads the resulting SVG file.
/// The returned string is a complete SVG document suitable for inline embedding.
///
/// # Errors
///
/// Returns an error string if:
/// - The temporary directory or file cannot be created
/// - `lilypond` is not available or fails to execute
/// - The output SVG file cannot be read
pub fn invoke_lilypond(ly_content: &str) -> Result<String, String> {
    let sanitized = sanitize_lilypond_content(ly_content);

    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let tmp_dir = std::env::temp_dir().join(format!("chordpro_ly_{pid}_{counter}"));

    // Use create_dir (not create_dir_all) to fail if the directory already exists,
    // preventing symlink-based attacks on predictable paths.
    std::fs::create_dir(&tmp_dir).map_err(|e| format!("failed to create temp directory: {e}"))?;

    let ly_path = tmp_dir.join("input.ly");
    let output_prefix = tmp_dir.join("output");

    std::fs::write(&ly_path, &sanitized).map_err(|e| {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        format!("failed to write temp file: {e}")
    })?;

    // Use -dsafe to sandbox Lilypond's embedded Scheme interpreter,
    // preventing arbitrary code execution from untrusted .cho files.
    let result = Command::new("lilypond")
        .arg("-dsafe")
        .arg("--svg")
        .arg(format!("-o{}", output_prefix.display()))
        .arg(&ly_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            format!("failed to invoke lilypond: {e}")
        })?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(format!("lilypond exited with error: {stderr}"));
    }

    let svg_path = tmp_dir.join("output.svg");
    let svg = std::fs::read_to_string(&svg_path).map_err(|e| {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        format!("failed to read lilypond SVG output: {e}")
    })?;

    let _ = std::fs::remove_dir_all(&tmp_dir);
    Ok(svg)
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

    #[test]
    fn unique_temp_paths_are_distinct() {
        let paths: Vec<_> = (0..100)
            .map(|_| super::unique_temp_path("test", "tmp"))
            .collect();
        // All paths should be unique thanks to the atomic counter.
        let unique: std::collections::HashSet<_> = paths.iter().collect();
        assert_eq!(unique.len(), paths.len());
    }

    #[test]
    fn write_temp_file_exclusive_prevents_overwrite() {
        let path = super::unique_temp_path("test_excl", "tmp");
        // First write succeeds.
        super::write_temp_file_exclusive(&path, "hello").unwrap();
        // Second write to same path must fail (O_EXCL semantics).
        let result = super::write_temp_file_exclusive(&path, "world");
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
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
    fn invoke_lilypond_produces_svg() {
        let ly = "\\relative c' { c4 d e f | g2 g | }\n";
        let result = invoke_lilypond(ly);
        assert!(result.is_ok(), "invoke_lilypond failed: {:?}", result.err());
        let svg = result.unwrap();
        assert!(svg.contains("<svg"), "output should contain SVG element");
    }

    #[test]
    fn invoke_lilypond_fails_gracefully_without_tool() {
        if has_lilypond() {
            return;
        }
        let result = invoke_lilypond("{ c4 }\n");
        assert!(result.is_err());
    }

    #[test]
    #[ignore]
    fn invoke_lilypond_blocks_scheme_code() {
        if !has_lilypond() {
            return;
        }
        // Embedded Scheme that tries to execute a system command.
        // With -dsafe this should fail (lilypond exits with error).
        let ly = r#"#(system "echo pwned")
\relative c' { c4 d e f | }
"#;
        let result = invoke_lilypond(ly);
        assert!(
            result.is_err(),
            "Lilypond should reject Scheme system calls with -dsafe"
        );
    }

    #[test]
    fn sanitize_abc_strips_beginjs_endjs_block() {
        let input = "X:1\nK:C\n%%beginjs\nprocess.exit(1);\n%%endjs\nCDEF|\n";
        let result = super::sanitize_abc_content(input);
        assert_eq!(result, "X:1\nK:C\nCDEF|");
        assert!(!result.contains("beginjs"));
        assert!(!result.contains("process"));
    }

    #[test]
    fn sanitize_abc_strips_javascript_directive() {
        let input = "X:1\nK:C\n%%javascript require('child_process').exec('id')\nCDEF|\n";
        let result = super::sanitize_abc_content(input);
        assert_eq!(result, "X:1\nK:C\nCDEF|");
        assert!(!result.contains("javascript"));
        assert!(!result.contains("child_process"));
    }

    #[test]
    fn sanitize_abc_preserves_normal_content() {
        let input = "X:1\nT:Test\nM:4/4\nK:C\n%%MIDI program 1\nCDEF|GABc|\n";
        let result = super::sanitize_abc_content(input);
        assert_eq!(
            result,
            "X:1\nT:Test\nM:4/4\nK:C\n%%MIDI program 1\nCDEF|GABc|"
        );
    }

    #[test]
    fn sanitize_abc_case_insensitive_beginjs() {
        let input = "X:1\n%%BeginJS\nalert(1);\n%%EndJS\nK:C\n";
        let result = super::sanitize_abc_content(input);
        assert_eq!(result, "X:1\nK:C");
    }

    #[test]
    fn sanitize_abc_case_insensitive_javascript() {
        // All-caps
        let input = "X:1\n%%JAVASCRIPT alert(1)\nK:C\n";
        let result = super::sanitize_abc_content(input);
        assert_eq!(result, "X:1\nK:C");

        // Mixed case
        let input = "X:1\n%%Javascript require('fs')\nK:C\n";
        let result = super::sanitize_abc_content(input);
        assert_eq!(result, "X:1\nK:C");

        // Another mixed case variant
        let input = "X:1\n%%jAvAsCrIpT evil()\nK:C\n";
        let result = super::sanitize_abc_content(input);
        assert_eq!(result, "X:1\nK:C");

        // Bare directive without code
        let input = "X:1\n%%JAVASCRIPT\nK:C\n";
        let result = super::sanitize_abc_content(input);
        assert_eq!(result, "X:1\nK:C");
    }

    #[test]
    fn sanitize_abc_does_not_strip_javascript_prefix_in_other_words() {
        // %%javascriptfoo is NOT a %%javascript directive (no space/tab separator).
        let input = "X:1\n%%javascriptfoo bar\nK:C\n";
        let result = super::sanitize_abc_content(input);
        assert!(result.contains("%%javascriptfoo"));
    }

    #[test]
    fn sanitize_lilypond_strips_system_call() {
        let input = "\\relative c' { c4 d e f }\n#(system \"echo pwned\")\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("system"));
        assert!(result.contains("\\relative"));
    }

    #[test]
    fn sanitize_lilypond_strips_getenv() {
        let input = "\\relative c' { c4 }\n#(getenv \"HOME\")\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("getenv"));
    }

    #[test]
    fn sanitize_lilypond_strips_file_access() {
        let input = "#(open-input-file \"/etc/passwd\")\n\\relative c' { c4 }\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("open-input-file"));
        assert!(result.contains("\\relative"));
    }

    #[test]
    fn sanitize_lilypond_strips_ly_system() {
        let input = "\\relative c' { c4 }\n#(ly:system \"rm -rf /\")\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("ly:system"));
    }

    #[test]
    fn sanitize_lilypond_case_insensitive() {
        let input = "#(SYSTEM \"echo pwned\")\n\\relative c' { c4 }\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("SYSTEM"));
    }

    #[test]
    fn sanitize_lilypond_preserves_normal_content() {
        let input = "\\version \"2.24.0\"\n\\relative c' {\n  c4 d e f |\n  g2 g |\n}\n";
        let result = super::sanitize_lilypond_content(input);
        assert_eq!(
            result,
            "\\version \"2.24.0\"\n\\relative c' {\n  c4 d e f |\n  g2 g |\n}"
        );
    }

    #[test]
    fn sanitize_lilypond_strips_eval_string() {
        let input = "#(eval-string \"(system \\\"id\\\")\")\n\\relative c' { c4 }\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("eval-string"));
    }

    #[test]
    fn sanitize_lilypond_strips_gulp_file() {
        let input = "#(ly:gulp-file \"/etc/passwd\")\n\\relative c' { c4 }\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("ly:gulp-file"));
    }

    #[test]
    fn sanitize_lilypond_strips_dollar_sign_system() {
        let input = "$(system \"echo pwned\")\n\\relative c' { c4 }\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("system"));
        assert!(result.contains("\\relative"));
    }

    #[test]
    fn sanitize_lilypond_strips_dollar_sign_with_space() {
        let input = "$( system \"echo pwned\")\n\\relative c' { c4 }\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("system"));
    }

    #[test]
    fn sanitize_lilypond_strips_dollar_sign_getenv() {
        let input = "$(getenv \"HOME\")\n\\relative c' { c4 }\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("getenv"));
    }

    #[test]
    fn sanitize_lilypond_strips_dollar_sign_ly_system() {
        let input = "$(ly:system \"rm -rf /\")\n\\relative c' { c4 }\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("ly:system"));
    }

    #[test]
    fn sanitize_lilypond_dollar_sign_case_insensitive() {
        let input = "$(SYSTEM \"echo pwned\")\n\\relative c' { c4 }\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("SYSTEM"));
    }

    #[test]
    fn sanitize_lilypond_strips_multi_space_scheme() {
        // Multiple spaces between #( or $( and the function name.
        let input = "#(  system \"rm -rf /\")\n\\relative c' { c4 }\n";
        let result = super::sanitize_lilypond_content(input);
        assert!(!result.contains("system"));

        let input2 = "$(   getenv \"SECRET\")\n\\relative c' { c4 }\n";
        let result2 = super::sanitize_lilypond_content(input2);
        assert!(!result2.contains("getenv"));
    }

    #[test]
    fn sanitize_lilypond_preserves_normal_dollar_sign() {
        // Dollar sign not followed by ( should be preserved.
        let input = "\\relative c' { c4$\\markup{test} }\n";
        let result = super::sanitize_lilypond_content(input);
        assert_eq!(result, "\\relative c' { c4$\\markup{test} }");
    }

    #[test]
    #[ignore]
    fn perl_chordpro_detection() {
        assert!(has_perl_chordpro(), "chordpro (Perl) not found in PATH");
    }
}
