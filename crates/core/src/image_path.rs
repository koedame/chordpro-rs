//! Shared image-path validation utilities.
//!
//! These helpers are used by multiple renderers to reject unsafe image
//! paths (directory traversal, absolute paths, Windows-style paths).
//! Centralising them in `chordpro-core` avoids duplication and ensures
//! consistent security behaviour across all renderers.

/// Check whether a path string looks like a Windows absolute path.
///
/// Detects drive-letter paths (`C:\…`, `C:/…`) and UNC paths (`\\…`)
/// using string-level checks so the result is consistent across platforms.
///
/// # Examples
///
/// ```
/// use chordpro_core::image_path::is_windows_absolute;
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
/// use chordpro_core::image_path::has_traversal;
///
/// assert!(has_traversal("../photo.jpg"));
/// assert!(has_traversal(r"images\..\..\etc\passwd"));
/// assert!(!has_traversal("images/photo.jpg"));
/// ```
#[must_use]
pub fn has_traversal(path: &str) -> bool {
    path.split(['/', '\\']).any(|seg| seg == "..")
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
}
