//! Shared image-path validation utilities.
//!
//! These helpers are used by multiple renderers to reject unsafe image
//! paths (directory traversal, absolute paths, Windows-style paths).
//! Centralising them in `chordsketch-core` avoids duplication and ensures
//! consistent security behaviour across all renderers.

/// Check whether a path string looks like a Windows absolute path.
///
/// Detects drive-letter paths (`C:\…`, `C:/…`) and UNC paths (`\\…`)
/// using string-level checks so the result is consistent across platforms.
///
/// # Examples
///
/// ```
/// use chordsketch_core::image_path::is_windows_absolute;
///
/// assert!(is_windows_absolute(r"C:\photo.jpg"));
/// assert!(is_windows_absolute(r"\\server\share"));
/// assert!(!is_windows_absolute("images/photo.jpg"));
/// ```
#[must_use]
pub fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    // Drive letter: e.g. `C:\` or `C:/`
    if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        return true;
    }
    // UNC path: `\\server\share`
    if bytes.len() >= 2 && bytes[0] == b'\\' && bytes[1] == b'\\' {
        return true;
    }
    false
}

/// Check whether a path contains `..` directory-traversal components.
///
/// Splits on both `/` and `\` so the check works for both Unix and
/// Windows-style separators.
///
/// # Examples
///
/// ```
/// use chordsketch_core::image_path::has_traversal;
///
/// assert!(has_traversal("../photo.jpg"));
/// assert!(has_traversal(r"images\..\..\etc\passwd"));
/// assert!(!has_traversal("images/photo.jpg"));
/// ```
#[must_use]
pub fn has_traversal(path: &str) -> bool {
    path.split(['/', '\\']).any(|seg| seg == "..")
}

/// Check whether an image `src` value is safe to expose in rendered output
/// across every renderer (text, HTML, PDF).
///
/// Uses an allowlist approach: only `http:`, `https:`, or scheme-less
/// *relative* paths are permitted. Absolute filesystem paths (Unix
/// `/…`, Windows `C:\…` / UNC `\\…`) and every other URI scheme
/// (`javascript:`, `data:`, `file:`, `blob:`, `vbscript:`, `mhtml:`, …)
/// are rejected. Paths containing NUL bytes or `..` components are also
/// rejected.
///
/// Centralising this check in `chordsketch-core` keeps the three
/// renderers aligned per `.claude/rules/renderer-parity.md` §Validation
/// Parity — a single `.cho` file must not produce different text / HTML /
/// PDF depending on how permissive each renderer happens to be.
///
/// # Examples
///
/// ```
/// use chordsketch_core::image_path::is_safe_image_src;
///
/// assert!(is_safe_image_src("photo.jpg"));
/// assert!(is_safe_image_src("https://example.com/photo.jpg"));
/// assert!(!is_safe_image_src("javascript:alert(1)"));
/// assert!(!is_safe_image_src("file:///etc/passwd"));
/// assert!(!is_safe_image_src("/absolute/path.jpg"));
/// ```
#[must_use]
pub fn is_safe_image_src(src: &str) -> bool {
    if src.is_empty() {
        return false;
    }

    // Reject null bytes (defense-in-depth — can truncate C string APIs
    // downstream even in pure-Rust code paths via FFI).
    if src.contains('\0') {
        return false;
    }

    let trimmed = src.trim_start();
    let normalised = trimmed.to_ascii_lowercase();

    // Reject Unix absolute paths.
    if normalised.starts_with('/') {
        return false;
    }

    // Reject Windows absolute paths (drive letter or UNC) on all platforms.
    if is_windows_absolute(trimmed) {
        return false;
    }

    // Reject directory traversal.
    if has_traversal(src) {
        return false;
    }

    // If the src contains a colon before any slash it has a URI scheme;
    // allow only http: and https:. A colon that appears after a slash is
    // part of a path segment (e.g. `path/to:file`) and is permitted.
    if let Some(colon_pos) = normalised.find(':') {
        let before_colon = &normalised[..colon_pos];
        if !before_colon.contains('/') {
            return before_colon == "http" || before_colon == "https";
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- is_windows_absolute ------------------------------------------------

    #[test]
    fn windows_drive_letter_backslash() {
        assert!(is_windows_absolute(r"C:\photo.jpg"));
        assert!(is_windows_absolute(r"D:\Users\photo.jpg"));
    }

    #[test]
    fn windows_drive_letter_forward_slash() {
        assert!(is_windows_absolute("C:/photo.jpg"));
    }

    #[test]
    fn windows_unc_path() {
        assert!(is_windows_absolute(r"\\server\share\photo.jpg"));
    }

    #[test]
    fn relative_path_not_windows_absolute() {
        assert!(!is_windows_absolute("images/photo.jpg"));
        assert!(!is_windows_absolute("photo.jpg"));
    }

    #[test]
    fn unix_absolute_not_windows_absolute() {
        assert!(!is_windows_absolute("/etc/passwd"));
    }

    // -- has_traversal ------------------------------------------------------

    #[test]
    fn forward_slash_traversal() {
        assert!(has_traversal("../photo.jpg"));
        assert!(has_traversal("images/../../etc/passwd"));
    }

    #[test]
    fn backslash_traversal() {
        assert!(has_traversal(r"..\photo.jpg"));
        assert!(has_traversal(r"images\..\..\etc\passwd"));
    }

    #[test]
    fn no_traversal() {
        assert!(!has_traversal("images/photo.jpg"));
        assert!(!has_traversal("photo.jpg"));
        assert!(!has_traversal(r"images\photo.jpg"));
    }

    #[test]
    fn double_dot_in_filename_not_traversal() {
        // "file..name" contains ".." but not as a path component.
        assert!(!has_traversal("file..name.jpg"));
    }

    // -- is_safe_image_src -------------------------------------------------

    #[test]
    fn safe_src_relative_paths() {
        assert!(is_safe_image_src("photo.jpg"));
        assert!(is_safe_image_src("images/photo.jpg"));
        assert!(is_safe_image_src("path/to:file.jpg")); // colon after slash is not a scheme
    }

    #[test]
    fn safe_src_http_and_https() {
        assert!(is_safe_image_src("http://example.com/photo.jpg"));
        assert!(is_safe_image_src("https://example.com/photo.jpg"));
    }

    #[test]
    fn safe_src_rejects_dangerous_schemes() {
        assert!(!is_safe_image_src("javascript:alert(1)"));
        assert!(!is_safe_image_src("data:image/png;base64,AAAA"));
        assert!(!is_safe_image_src("file:///etc/passwd"));
        assert!(!is_safe_image_src("blob:https://example.com/uuid"));
        assert!(!is_safe_image_src("vbscript:msgbox"));
    }

    #[test]
    fn safe_src_rejects_absolute_paths() {
        assert!(!is_safe_image_src("/etc/passwd"));
        assert!(!is_safe_image_src(r"C:\Users\secret"));
        assert!(!is_safe_image_src(r"\\server\share\file"));
    }

    #[test]
    fn safe_src_rejects_empty_and_null() {
        assert!(!is_safe_image_src(""));
        assert!(!is_safe_image_src("photo\0.jpg"));
    }

    #[test]
    fn safe_src_rejects_traversal() {
        assert!(!is_safe_image_src("../photo.jpg"));
        assert!(!is_safe_image_src("images/../../etc/passwd"));
    }

    #[test]
    fn safe_src_case_insensitive_scheme() {
        assert!(!is_safe_image_src("JavaScript:alert(1)"));
        assert!(is_safe_image_src("HTTPS://example.com/photo.jpg"));
    }

    #[test]
    fn safe_src_rejects_scheme_with_whitespace_prefix() {
        assert!(!is_safe_image_src(" javascript:alert(1)"));
        assert!(!is_safe_image_src("\tfile:///etc/passwd"));
    }
}
