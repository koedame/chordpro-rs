//! Hierarchical configuration loading for ChordPro.
//!
//! Configuration is loaded from multiple sources in precedence order (later
//! sources override earlier ones):
//!
//! 1. **Built-in defaults** — hardcoded baseline configuration
//! 2. **System config** — `/etc/chordpro.json` (Linux), platform equivalent
//! 3. **User config** — `~/.config/chordpro/chordpro.json`
//! 4. **Project config** — `chordpro.json` in the song file directory
//! 5. **Song-specific config** — referenced via CLI flag or directive
//!
//! Map values are deep-merged; array values are replaced (not concatenated).
//!
//! # Examples
//!
//! ```
//! use chordpro_core::config::Config;
//!
//! let config = Config::defaults();
//! assert!(!config.get("pdf").is_null());
//! ```

use crate::rrjson::{self, NULL, Value};

use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// An error encountered when loading or resolving a configuration file.
#[derive(Debug)]
pub enum ConfigError {
    /// An I/O error occurred while reading a config file.
    Io {
        /// The path that failed to read.
        path: String,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// A parse error occurred in the config file content.
    Parse {
        /// The path of the file that failed to parse.
        path: String,
        /// The underlying parse error.
        source: rrjson::ParseError,
    },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{path}: {source}"),
            Self::Parse { path, source } => write!(f, "{path}: {source}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

// ---------------------------------------------------------------------------
// Deep merge
// ---------------------------------------------------------------------------

/// Deep-merge `overlay` into `base`, returning the merged result.
///
/// - Objects are recursively merged (keys in `overlay` override `base`).
/// - Arrays are replaced entirely (not concatenated).
/// - Scalar values in `overlay` replace those in `base`.
#[must_use]
pub(crate) fn deep_merge(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_entries), Value::Object(overlay_entries)) => {
            for (key, overlay_val) in overlay_entries {
                let existing = base_entries.iter().position(|(k, _)| k == &key);
                if let Some(idx) = existing {
                    let (_, base_val) = base_entries.remove(idx);
                    base_entries.insert(idx, (key, deep_merge(base_val, overlay_val)));
                } else {
                    base_entries.push((key, overlay_val));
                }
            }
            Value::Object(base_entries)
        }
        // Arrays and scalars: overlay wins entirely
        (_, overlay) => overlay,
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Result of loading configuration, including any non-fatal warnings.
#[derive(Debug)]
pub struct ConfigLoadResult {
    /// The loaded configuration.
    pub config: Config,
    /// Non-fatal warnings encountered during loading (parse errors in
    /// optional config files, unsupported RRJSON directives, I/O issues).
    pub warnings: Vec<String>,
}

/// A ChordPro configuration loaded from one or more sources.
///
/// Wraps a [`Value::Object`] and provides convenience accessors.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    root: Value,
}

impl Config {
    /// Create a configuration from the built-in defaults.
    #[must_use]
    pub fn defaults() -> Self {
        let root =
            rrjson::parse_rrjson(DEFAULT_CONFIG).expect("built-in default config is valid RRJSON");
        Self { root }
    }

    /// Create an empty configuration.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            root: Value::Object(Vec::new()),
        }
    }

    /// Parse a configuration from a RRJSON string.
    ///
    /// # Errors
    ///
    /// Returns a [`rrjson::ParseError`] if the input is malformed.
    pub fn parse(input: &str) -> Result<Self, rrjson::ParseError> {
        let root = rrjson::parse_rrjson(input)?;
        Ok(Self { root })
    }

    /// Merge another configuration on top of this one.
    ///
    /// Values in `overlay` take precedence. Objects are deep-merged.
    #[must_use]
    pub fn merge(self, overlay: Config) -> Config {
        Config {
            root: deep_merge(self.root, overlay.root),
        }
    }

    /// Look up a top-level key. Returns `Value::Null` if not found.
    #[must_use]
    pub fn get(&self, key: &str) -> &Value {
        self.root.get(key)
    }

    /// Look up a dot-separated key path (e.g., `"pdf.chorus.indent"`).
    ///
    /// Returns `Value::Null` if any segment is missing.
    #[must_use]
    pub fn get_path(&self, path: &str) -> &Value {
        let mut current = &self.root;
        for segment in path.split('.') {
            current = current.get(segment);
            if current.is_null() {
                return &NULL;
            }
        }
        current
    }

    /// Returns a reference to the underlying [`Value`].
    #[must_use]
    pub fn as_value(&self) -> &Value {
        &self.root
    }

    /// Load a built-in preset configuration by short name.
    ///
    /// Returns `None` if the name does not match a built-in preset.
    ///
    /// Currently supported presets: `"guitar"`, `"ukulele"`.
    #[must_use]
    pub fn preset(name: &str) -> Option<Self> {
        let rrjson = match name.to_ascii_lowercase().as_str() {
            "guitar" => PRESET_GUITAR,
            "ukulele" => PRESET_UKULELE,
            _ => return None,
        };
        Some(Self {
            root: rrjson::parse_rrjson(rrjson).expect("built-in preset is valid RRJSON"),
        })
    }

    /// Resolve a config name: try as a preset first, then as a file path.
    ///
    /// Returns `Ok(Config)` on success, or a [`ConfigError`] on failure.
    ///
    /// # Trust boundary
    ///
    /// When `name` is not a preset, it is used directly as a file path.
    /// No path validation or restriction is applied — the caller must
    /// ensure the path comes from trusted input. This is safe for a local
    /// CLI tool where the user already has filesystem access, but callers
    /// invoking this programmatically with partially untrusted input should
    /// validate the path first.
    ///
    /// Symlinks and files exceeding [`MAX_CONFIG_FILE_SIZE`] are rejected,
    /// matching the behavior of hierarchical config loading.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Io`] if the file cannot be read, or
    /// [`ConfigError::Parse`] if the content is malformed.
    pub fn resolve(name: &str) -> Result<Self, ConfigError> {
        // Try preset first
        if let Some(preset) = Self::preset(name) {
            return Ok(preset);
        }
        // Try as a file path — apply the same security checks as hierarchical config
        let text = read_config_file(Path::new(name)).map_err(|e| ConfigError::Io {
            path: name.to_string(),
            source: e,
        })?;
        Self::parse(&text).map_err(|e| ConfigError::Parse {
            path: name.to_string(),
            source: e,
        })
    }

    /// Apply a single `key=value` define override.
    ///
    /// The key may be dot-separated (e.g., `pdf.chorus.indent=20`).
    /// The value is parsed as RRJSON (so `20` becomes a number, `"hello"`
    /// becomes a string, etc.). If the value cannot be parsed, it is
    /// treated as a plain string.
    #[must_use]
    pub fn with_define(self, define: &str) -> Self {
        let Some(eq_pos) = define.find('=') else {
            return self;
        };
        let key = define[..eq_pos].trim();
        let raw_value = define[eq_pos + 1..].trim();
        if key.is_empty() {
            return self;
        }

        // Try to parse the value as a JSON value; fall back to string.
        let value = rrjson::parse_rrjson(&format!("{{\"_\": {raw_value}}}"))
            .ok()
            .and_then(|v| match v {
                Value::Object(entries) => entries.into_iter().next().map(|(_, v)| v),
                _ => None,
            })
            .unwrap_or_else(|| Value::String(raw_value.to_string()));

        // Build a nested object from the dot-separated key.
        // If the key exceeds the nesting depth limit, ignore the define.
        let Some(overlay) = build_nested_value(key, value) else {
            return self;
        };
        Config {
            root: deep_merge(self.root, overlay),
        }
    }

    /// Build a configuration by loading and merging from all sources.
    ///
    /// Loads: defaults → system → user → project → song-specific.
    /// Missing files at any level are silently skipped.
    ///
    /// `project_dir` is the directory containing the song file (for
    /// project-level config). `song_config` is an optional path to a
    /// song-specific config file.
    #[must_use]
    pub fn load(project_dir: Option<&str>, song_config: Option<&str>) -> ConfigLoadResult {
        let mut config = Self::defaults();
        let mut warnings = Vec::new();

        // System config
        let system_path = PathBuf::from("/etc/chordpro.json");
        if let Some(text) = read_file_if_exists(&system_path, &mut warnings) {
            match Self::parse_collecting_warnings(&text, &mut warnings) {
                Ok(sys) => config = config.merge(sys),
                Err(e) => warnings.push(format!(
                    "failed to parse config file {}: {e}",
                    system_path.display()
                )),
            }
        }

        // User config: respect XDG_CONFIG_HOME, fall back to $HOME/.config
        if let Some(config_dir) = config_dir() {
            let user_path = config_dir.join("chordpro").join("chordpro.json");
            if let Some(text) = read_file_if_exists(&user_path, &mut warnings) {
                match Self::parse_collecting_warnings(&text, &mut warnings) {
                    Ok(user) => config = config.merge(user),
                    Err(e) => warnings.push(format!(
                        "failed to parse config file {}: {e}",
                        user_path.display()
                    )),
                }
            }
        }

        // Snapshot delegate settings from trusted sources (system + user config).
        // Project-level and song-specific configs must not silently enable
        // delegate execution — only CLI flags, auto-detection, or explicit
        // user config (~/.config/chordpro/) may enable delegates.
        let trusted_abc2svg = config
            .get_path("delegates.abc2svg")
            .as_bool()
            .unwrap_or(false);
        let trusted_lilypond = config
            .get_path("delegates.lilypond")
            .as_bool()
            .unwrap_or(false);

        // Project config
        if let Some(dir) = project_dir {
            let project_path = PathBuf::from(dir).join("chordpro.json");
            if let Some(text) = read_file_if_exists(&project_path, &mut warnings) {
                match Self::parse_collecting_warnings(&text, &mut warnings) {
                    Ok(proj) => config = config.merge(proj),
                    Err(e) => warnings.push(format!(
                        "failed to parse config file {}: {e}",
                        project_path.display()
                    )),
                }
            }
        }

        // Song-specific config
        if let Some(path) = song_config {
            if let Some(text) = read_file_if_exists(Path::new(path), &mut warnings) {
                match Self::parse_collecting_warnings(&text, &mut warnings) {
                    Ok(song) => config = config.merge(song),
                    Err(e) => warnings.push(format!("failed to parse config file {path}: {e}")),
                }
            }
        }

        // Restore delegate settings to trusted values. If a project or song
        // config attempted to enable a delegate, override it back and warn.
        let project_abc2svg = config
            .get_path("delegates.abc2svg")
            .as_bool()
            .unwrap_or(false);
        let project_lilypond = config
            .get_path("delegates.lilypond")
            .as_bool()
            .unwrap_or(false);

        if project_abc2svg && !trusted_abc2svg {
            config = config.with_define("delegates.abc2svg=false");
            warnings.push(
                "delegates.abc2svg was enabled by a project-level config file and has been \
                 disabled for security; use --define delegates.abc2svg=true to enable"
                    .to_string(),
            );
        }
        if project_lilypond && !trusted_lilypond {
            config = config.with_define("delegates.lilypond=false");
            warnings.push(
                "delegates.lilypond was enabled by a project-level config file and has been \
                 disabled for security; use --define delegates.lilypond=true to enable"
                    .to_string(),
            );
        }

        ConfigLoadResult { config, warnings }
    }

    /// Parse a configuration from a RRJSON string, collecting warnings into
    /// the provided vector.
    fn parse_collecting_warnings(
        input: &str,
        warnings: &mut Vec<String>,
    ) -> Result<Self, rrjson::ParseError> {
        let result = rrjson::parse_rrjson_with_warnings(input)?;
        warnings.extend(result.warnings);
        Ok(Self { root: result.value })
    }
}

/// Maximum nesting depth for dotted keys in `--define` flags.
///
/// Matches the limit used by the RRJSON parser for structural nesting.
const MAX_DEFINE_DEPTH: usize = 64;

/// Build a nested `Value::Object` from a dot-separated key and a leaf value.
///
/// For example, `build_nested_value("a.b.c", Number(42))` produces:
/// `{"a": {"b": {"c": 42}}}`
///
/// Returns `None` if the key has more than [`MAX_DEFINE_DEPTH`] segments.
fn build_nested_value(key: &str, value: Value) -> Option<Value> {
    let segments: Vec<&str> = key.split('.').collect();
    if segments.len() > MAX_DEFINE_DEPTH {
        return None;
    }
    let mut result = value;
    for segment in segments.into_iter().rev() {
        result = Value::Object(vec![(segment.to_string(), result)]);
    }
    Some(result)
}

/// Maximum config file size (10 MB). Files larger than this are rejected
/// to prevent accidental OOM from device files or very large inputs.
const MAX_CONFIG_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Open a file without following symlinks.
///
/// On Unix, uses `O_NOFOLLOW` via `OpenOptionsExt::custom_flags` to
/// atomically reject symlinks at the kernel level. On non-Unix platforms,
/// falls back to a plain `File::open` (the caller's pre-open symlink check
/// is the only defense).
fn open_no_follow(path: &Path) -> Result<File, std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NOFOLLOW value from POSIX / Linux headers. The constant is
        // stable across Linux (0o400000) and macOS (0x0100).
        // Note: chordpro-core has a zero-dependency policy, so we cannot
        // use the `libc` crate for portable O_NOFOLLOW constants.
        #[cfg(target_os = "linux")]
        const O_NOFOLLOW: i32 = 0o400000;
        #[cfg(target_os = "macos")]
        const O_NOFOLLOW: i32 = 0x0100;
        // On other Unix platforms (FreeBSD, OpenBSD, etc.) the O_NOFOLLOW
        // value differs and we fall back to 0, which disables kernel-level
        // symlink protection. The pre-open symlink_metadata() check in
        // read_config_file() still provides TOCTOU-window-limited defense.
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        const O_NOFOLLOW: i32 = 0;

        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(O_NOFOLLOW)
            .open(path)
    }
    #[cfg(not(unix))]
    {
        File::open(path)
    }
}

/// Read a config file with symlink and size checks.
///
/// 1. Pre-open `symlink_metadata` check for a user-friendly error message.
/// 2. On Unix: opens with `O_NOFOLLOW` to atomically reject symlinks at
///    the kernel level, eliminating the TOCTOU window between the metadata
///    check and file open.
/// 3. On non-Unix: retains the pre-open metadata check only.
/// 4. Checks metadata on the file descriptor to enforce the size limit.
fn read_config_file(path: &Path) -> Result<String, std::io::Error> {
    // Pre-open symlink check — provides a clear error message on all platforms.
    let link_meta = std::fs::symlink_metadata(path)?;
    if link_meta.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("config file is a symlink: {}", path.display()),
        ));
    }

    // Open the file. On Unix, O_NOFOLLOW atomically rejects symlinks at
    // the kernel level, closing the TOCTOU window between the metadata
    // check above and the actual open.
    let mut file = open_no_follow(path)?;
    let fd_meta = file.metadata()?;

    if fd_meta.len() > MAX_CONFIG_FILE_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "config file size {} exceeds {} byte limit: {}",
                fd_meta.len(),
                MAX_CONFIG_FILE_SIZE,
                path.display()
            ),
        ));
    }

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    Ok(contents)
}

/// Read a file to a String, returning None if it doesn't exist or can't be read.
///
/// Rejects files that are symlinks or exceed [`MAX_CONFIG_FILE_SIZE`], emitting
/// a warning to stderr in either case.
fn read_file_if_exists(path: &Path, warnings: &mut Vec<String>) -> Option<String> {
    match read_config_file(path) {
        Ok(contents) => Some(contents),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            warnings.push(format!("skipping config file {}: {e}", path.display()));
            None
        }
    }
}

/// Get the user's home directory as a `PathBuf`.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Get the XDG config directory, respecting `XDG_CONFIG_HOME` and falling
/// back to `$HOME/.config`.
fn config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        let path = PathBuf::from(xdg);
        if path.is_absolute() {
            return Some(path);
        }
    }
    home_dir().map(|h| h.join(".config"))
}

// ---------------------------------------------------------------------------
// Built-in default configuration
// ---------------------------------------------------------------------------

/// The built-in default configuration as RRJSON.
///
/// This provides sensible defaults for all configurable aspects of ChordPro
/// rendering. These values can be overridden at any level of the config
/// hierarchy.
const DEFAULT_CONFIG: &str = r#"{
    // General settings
    settings: {
        columns: 1,
        suppress_empty_chords: true,
        lyrics_only: false,
        transpose: 0
    },

    // PDF rendering
    pdf: {
        papersize: "a4",
        theme: {
            foreground: "black",
            background: "white"
        },
        fonts: {
            title: { name: "Helvetica-Bold", size: 18 },
            subtitle: { name: "Helvetica", size: 13 },
            text: { name: "Helvetica", size: 11 },
            chord: { name: "Helvetica-Bold", size: 9 },
            comment: { name: "Helvetica-Oblique", size: 9 },
            tab: { name: "Courier", size: 9 }
        },
        spacing: {
            title: 6,
            subtitle: 4,
            lyrics: 4,
            chords: 2,
            grid: 4,
            tab: 2,
            empty: 8
        },
        chorus: {
            indent: 20,
            bar: { offset: 8, width: 1, color: "black" },
            recall: { type: "comment" }
        },
        margins: {
            top: 56,
            bottom: 56,
            left: 56,
            right: 56
        },
        columns: {
            gap: 20
        }
    },

    // HTML rendering
    html: {
        styles: {
            body: "font-family: sans-serif;",
            chord: "color: red; font-weight: bold;",
            comment: "color: gray; font-style: italic;"
        }
    },

    // Chord display
    chords: {
        show: "all",
        capo: { show: true }
    },

    // Metadata
    metadata: {
        separator: "; "
    },

    // Delegate environments (external tool integration)
    delegates: {
        abc2svg: false,
        lilypond: false
    }
}"#;

// ---------------------------------------------------------------------------
// Built-in preset configurations
// ---------------------------------------------------------------------------

/// Guitar preset configuration (standard tuning, 6 strings).
static PRESET_GUITAR: &str = r#"{
    instrument: {
        type: "guitar",
        description: "Guitar, standard tuning"
    },
    tuning: ["E2", "A2", "D3", "G3", "B3", "E4"],
    diagrams: {
        strings: 6,
        frets: 5
    }
}"#;

/// Ukulele preset configuration (standard tuning, 4 strings).
static PRESET_UKULELE: &str = r#"{
    instrument: {
        type: "ukulele",
        description: "Ukulele, standard tuning"
    },
    tuning: ["G4", "C4", "E4", "A4"],
    diagrams: {
        strings: 4,
        frets: 5
    }
}"#;

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_defaults_load() {
        let config = Config::defaults();
        assert!(!config.get("pdf").is_null());
        assert!(!config.get("settings").is_null());
    }

    #[test]
    fn test_get_path() {
        let config = Config::defaults();
        assert_eq!(
            config.get_path("pdf.fonts.title.size"),
            &Value::Number(18.0)
        );
    }

    #[test]
    fn test_get_path_missing() {
        let config = Config::defaults();
        assert!(config.get_path("nonexistent.path").is_null());
    }

    #[test]
    fn test_deep_merge_scalars() {
        let base = Value::Object(vec![("a".to_string(), Value::Number(1.0))]);
        let overlay = Value::Object(vec![("a".to_string(), Value::Number(2.0))]);
        let merged = deep_merge(base, overlay);
        assert_eq!(merged["a"], Value::Number(2.0));
    }

    #[test]
    fn test_deep_merge_adds_keys() {
        let base = Value::Object(vec![("a".to_string(), Value::Number(1.0))]);
        let overlay = Value::Object(vec![("b".to_string(), Value::Number(2.0))]);
        let merged = deep_merge(base, overlay);
        assert_eq!(merged["a"], Value::Number(1.0));
        assert_eq!(merged["b"], Value::Number(2.0));
    }

    #[test]
    fn test_deep_merge_nested_objects() {
        let base =
            rrjson::parse_rrjson(r#"{"pdf": {"fonts": {"size": 11}, "margin": 20}}"#).unwrap();
        let overlay = rrjson::parse_rrjson(r#"{"pdf": {"fonts": {"size": 14}}}"#).unwrap();
        let merged = deep_merge(base, overlay);
        assert_eq!(merged["pdf"]["fonts"]["size"], Value::Number(14.0));
        assert_eq!(merged["pdf"]["margin"], Value::Number(20.0));
    }

    #[test]
    fn test_deep_merge_arrays_replaced() {
        let base = rrjson::parse_rrjson(r#"{"items": [1, 2, 3]}"#).unwrap();
        let overlay = rrjson::parse_rrjson(r#"{"items": [4, 5]}"#).unwrap();
        let merged = deep_merge(base, overlay);
        assert_eq!(
            merged["items"],
            Value::Array(vec![Value::Number(4.0), Value::Number(5.0)])
        );
    }

    #[test]
    fn test_config_merge() {
        let base = Config::parse(r#"{"a": 1, "b": {"c": 2}}"#).unwrap();
        let overlay = Config::parse(r#"{"a": 10, "b": {"d": 3}}"#).unwrap();
        let merged = base.merge(overlay);
        assert_eq!(merged.get_path("a"), &Value::Number(10.0));
        assert_eq!(merged.get_path("b.c"), &Value::Number(2.0));
        assert_eq!(merged.get_path("b.d"), &Value::Number(3.0));
    }

    #[test]
    fn test_config_from_str() {
        let config = Config::parse(r#"{"key": "value"}"#).unwrap();
        assert_eq!(config.get("key"), &Value::String("value".to_string()));
    }

    #[test]
    fn test_config_empty() {
        let config = Config::empty();
        assert!(config.get("anything").is_null());
    }

    #[test]
    fn test_load_with_no_files() {
        // load() should succeed even when no config files exist.
        // We use a non-existent project dir to ensure nothing loads.
        let result = Config::load(Some("/nonexistent/path"), None);
        // Should still have defaults
        assert!(!result.config.get("pdf").is_null());
    }

    #[test]
    fn test_defaults_pdf_margins() {
        let config = Config::defaults();
        assert_eq!(config.get_path("pdf.margins.top"), &Value::Number(56.0));
        assert_eq!(config.get_path("pdf.margins.left"), &Value::Number(56.0));
    }

    #[test]
    fn test_defaults_settings() {
        let config = Config::defaults();
        assert_eq!(config.get_path("settings.columns"), &Value::Number(1.0));
        assert_eq!(config.get_path("settings.transpose"), &Value::Number(0.0));
    }

    #[test]
    fn test_merge_precedence_chain() {
        let defaults = Config::parse(r#"{"a": 1, "b": 2, "c": 3}"#).unwrap();
        let system = Config::parse(r#"{"a": 10}"#).unwrap();
        let user = Config::parse(r#"{"b": 20}"#).unwrap();
        let project = Config::parse(r#"{"c": 30}"#).unwrap();
        let song = Config::parse(r#"{"a": 100}"#).unwrap();

        let config = defaults
            .merge(system)
            .merge(user)
            .merge(project)
            .merge(song);
        assert_eq!(config.get("a"), &Value::Number(100.0));
        assert_eq!(config.get("b"), &Value::Number(20.0));
        assert_eq!(config.get("c"), &Value::Number(30.0));
    }

    // -- with_define tests ----------------------------------------------------

    #[test]
    fn test_define_simple_number() {
        let config = Config::empty().with_define("key=42");
        assert_eq!(config.get("key"), &Value::Number(42.0));
    }

    #[test]
    fn test_define_string() {
        let config = Config::empty().with_define(r#"key="hello""#);
        assert_eq!(config.get("key"), &Value::String("hello".to_string()));
    }

    #[test]
    fn test_define_dotted_key() {
        let config = Config::empty().with_define("pdf.chorus.indent=20");
        assert_eq!(config.get_path("pdf.chorus.indent"), &Value::Number(20.0));
    }

    #[test]
    fn test_define_overrides_existing() {
        let config = Config::defaults().with_define("pdf.margins.top=100");
        assert_eq!(config.get_path("pdf.margins.top"), &Value::Number(100.0));
        // Other margins should be unchanged
        assert_eq!(config.get_path("pdf.margins.left"), &Value::Number(56.0));
    }

    #[test]
    fn test_define_bool() {
        let config = Config::empty().with_define("flag=true");
        assert_eq!(config.get("flag"), &Value::Bool(true));
    }

    #[test]
    fn test_define_unquoted_string_fallback() {
        // Values that aren't valid JSON fall back to string
        let config = Config::empty().with_define("key=hello world");
        assert_eq!(config.get("key"), &Value::String("hello world".to_string()));
    }

    #[test]
    fn test_define_no_equals_ignored() {
        let config = Config::empty().with_define("noequalssign");
        assert!(config.get("noequalssign").is_null());
    }

    #[test]
    fn test_multiple_defines() {
        let config = Config::empty()
            .with_define("a=1")
            .with_define("b=2")
            .with_define("a=3");
        assert_eq!(config.get("a"), &Value::Number(3.0));
        assert_eq!(config.get("b"), &Value::Number(2.0));
    }

    #[test]
    fn test_define_excessive_depth_rejected() {
        // A dotted key with more than MAX_DEFINE_DEPTH segments should be ignored.
        let segments: Vec<String> = (0..=MAX_DEFINE_DEPTH).map(|i| format!("k{i}")).collect();
        let deep_key = segments.join(".");
        let config = Config::empty().with_define(&format!("{deep_key}=1"));
        // The define should have been silently ignored
        assert!(config.get("k0").is_null());
    }

    #[test]
    fn test_define_at_max_depth_accepted() {
        // Exactly MAX_DEFINE_DEPTH segments should be accepted.
        let segments: Vec<String> = (0..MAX_DEFINE_DEPTH).map(|i| format!("k{i}")).collect();
        let key = segments.join(".");
        let config = Config::empty().with_define(&format!("{key}=42"));
        assert!(!config.get("k0").is_null());
    }

    // -- Preset tests ---------------------------------------------------------

    #[test]
    fn test_preset_guitar() {
        let config = Config::preset("guitar").expect("guitar preset should exist");
        assert_eq!(
            config.get_path("instrument.type"),
            &Value::String("guitar".to_string())
        );
        assert_eq!(config.get_path("diagrams.strings"), &Value::Number(6.0));
    }

    #[test]
    fn test_preset_ukulele() {
        let config = Config::preset("ukulele").expect("ukulele preset should exist");
        assert_eq!(
            config.get_path("instrument.type"),
            &Value::String("ukulele".to_string())
        );
        assert_eq!(config.get_path("diagrams.strings"), &Value::Number(4.0));
    }

    #[test]
    fn test_preset_case_insensitive() {
        assert!(Config::preset("Guitar").is_some());
        assert!(Config::preset("UKULELE").is_some());
    }

    #[test]
    fn test_preset_unknown_returns_none() {
        assert!(Config::preset("banjo").is_none());
    }

    #[test]
    fn test_preset_merges_with_defaults() {
        let config = Config::defaults().merge(Config::preset("guitar").unwrap());
        // Should have both default settings and guitar instrument
        assert!(!config.get("pdf").is_null());
        assert_eq!(
            config.get_path("instrument.type"),
            &Value::String("guitar".to_string())
        );
    }

    #[test]
    fn test_resolve_preset() {
        let config = Config::resolve("guitar").expect("guitar should resolve");
        assert_eq!(
            config.get_path("instrument.type"),
            &Value::String("guitar".to_string())
        );
    }

    #[test]
    fn test_resolve_nonexistent_file() {
        let result = Config::resolve("/nonexistent/file.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_guitar_tuning_has_6_strings() {
        let config = Config::preset("guitar").unwrap();
        match config.get("tuning") {
            Value::Array(arr) => assert_eq!(arr.len(), 6),
            _ => panic!("tuning should be an array"),
        }
    }

    #[test]
    fn test_ukulele_tuning_has_4_strings() {
        let config = Config::preset("ukulele").unwrap();
        match config.get("tuning") {
            Value::Array(arr) => assert_eq!(arr.len(), 4),
            _ => panic!("tuning should be an array"),
        }
    }

    #[test]
    fn test_define_empty_value() {
        let config = Config::empty().with_define("key=");
        assert_eq!(config.get("key"), &Value::String(String::new()));
    }

    // -- Integration tests for Config::load() with actual files ---------------

    #[test]
    fn test_load_project_config() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("chordpro.json"),
            r#"{ "settings": { "columns": 3 } }"#,
        )
        .unwrap();

        let result = Config::load(Some(dir.path().to_str().unwrap()), None);
        assert_eq!(
            result.config.get_path("settings.columns"),
            &Value::Number(3.0)
        );
        // Defaults should still be present for non-overridden keys
        assert_eq!(
            result.config.get_path("pdf.margins.top"),
            &Value::Number(56.0)
        );
    }

    #[test]
    fn test_load_song_config() {
        let dir = tempdir().unwrap();
        let song_path = dir.path().join("song.json");
        std::fs::write(&song_path, r#"{ "pdf": { "papersize": "letter" } }"#).unwrap();

        let result = Config::load(None, Some(song_path.to_str().unwrap()));
        assert_eq!(
            result.config.get_path("pdf.papersize"),
            &Value::String("letter".to_string())
        );
    }

    #[test]
    fn test_load_project_and_song_merge_precedence() {
        let project_dir = tempdir().unwrap();
        let song_dir = tempdir().unwrap();

        // Project sets columns=2 and transpose=5
        std::fs::write(
            project_dir.path().join("chordpro.json"),
            r#"{ "settings": { "columns": 2, "transpose": 5 } }"#,
        )
        .unwrap();

        // Song overrides columns=4
        let song_path = song_dir.path().join("song.json");
        std::fs::write(&song_path, r#"{ "settings": { "columns": 4 } }"#).unwrap();

        let result = Config::load(
            Some(project_dir.path().to_str().unwrap()),
            Some(song_path.to_str().unwrap()),
        );
        // Song overrides project
        assert_eq!(
            result.config.get_path("settings.columns"),
            &Value::Number(4.0)
        );
        // Project setting not overridden by song
        assert_eq!(
            result.config.get_path("settings.transpose"),
            &Value::Number(5.0)
        );
    }

    #[test]
    fn test_project_config_cannot_enable_delegates() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("chordpro.json"),
            r#"{ "delegates": { "abc2svg": true, "lilypond": true } }"#,
        )
        .unwrap();

        let result = Config::load(Some(dir.path().to_str().unwrap()), None);
        // Delegates should be reset to false
        assert_eq!(
            result.config.get_path("delegates.abc2svg"),
            &Value::Bool(false)
        );
        assert_eq!(
            result.config.get_path("delegates.lilypond"),
            &Value::Bool(false)
        );
        // Warnings should be emitted
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("delegates.abc2svg")),
            "expected delegate warning, got: {:?}",
            result.warnings
        );
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("delegates.lilypond")),
            "expected delegate warning, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_load_invalid_project_config_continues() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("chordpro.json"), "{ invalid json !!!").unwrap();

        // Should not panic; defaults are still loaded
        let result = Config::load(Some(dir.path().to_str().unwrap()), None);
        assert!(!result.config.get("pdf").is_null());
        // Warning should be collected instead of printed to stderr
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("failed to parse")),
            "expected parse warning, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_resolve_from_temp_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("custom.json");
        std::fs::write(&file_path, r#"{ "custom": true }"#).unwrap();

        let config = Config::resolve(file_path.to_str().unwrap()).unwrap();
        assert_eq!(config.get("custom"), &Value::Bool(true));
    }

    // -- File size / symlink guard tests --------------------------------------

    #[test]
    fn test_read_file_if_exists_normal_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");
        std::fs::write(&file_path, r#"{"key": "value"}"#).unwrap();

        let mut warnings = Vec::new();
        let result = read_file_if_exists(&file_path, &mut warnings);
        assert!(result.is_some());
        assert!(result.unwrap().contains("key"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_read_file_if_exists_nonexistent() {
        let mut warnings = Vec::new();
        let result = read_file_if_exists(Path::new("/nonexistent/path/config.json"), &mut warnings);
        assert!(result.is_none());
        assert!(warnings.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn test_read_file_if_exists_rejects_symlink() {
        let dir = tempdir().unwrap();
        let real_file = dir.path().join("real.json");
        let link_path = dir.path().join("link.json");
        std::fs::write(&real_file, r#"{"key": "value"}"#).unwrap();
        std::os::unix::fs::symlink(&real_file, &link_path).unwrap();

        let mut warnings = Vec::new();
        let result = read_file_if_exists(&link_path, &mut warnings);
        assert!(result.is_none(), "symlink should be rejected");
        assert!(!warnings.is_empty(), "should produce a warning for symlink");
    }

    // -- resolve() security tests ------------------------------------------------

    #[cfg(unix)]
    #[test]
    fn test_resolve_rejects_symlink() {
        let dir = tempdir().unwrap();
        let real_file = dir.path().join("real.json");
        let link_path = dir.path().join("link.json");
        std::fs::write(&real_file, r#"{"key": "value"}"#).unwrap();
        std::os::unix::fs::symlink(&real_file, &link_path).unwrap();

        let result = Config::resolve(link_path.to_str().unwrap());
        assert!(result.is_err(), "resolve() should reject symlinks");
    }

    #[test]
    fn test_read_config_file_normal() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");
        std::fs::write(&file_path, r#"{"ok": true}"#).unwrap();

        let text = read_config_file(&file_path).unwrap();
        assert!(text.contains("ok"));
    }

    #[test]
    fn test_read_config_file_nonexistent() {
        let result = read_config_file(Path::new("/nonexistent/path/config.json"));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    }

    #[cfg(unix)]
    #[test]
    fn test_read_config_file_rejects_symlink() {
        let dir = tempdir().unwrap();
        let real_file = dir.path().join("real.json");
        let link_path = dir.path().join("link.json");
        std::fs::write(&real_file, r#"{"key": "value"}"#).unwrap();
        std::os::unix::fs::symlink(&real_file, &link_path).unwrap();

        let result = read_config_file(&link_path);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    }

    // -- XDG_CONFIG_HOME tests ---------------------------------------------------
    // These tests manipulate the environment, so they must run serially
    // (cargo test runs each test function in its own thread, but env vars are
    // process-global). Use unique env manipulation patterns to minimize risk.

    // SAFETY: These tests manipulate process-global environment variables.
    // This is safe in test context because:
    // 1. Each test saves and restores the previous value
    // 2. The env var is only read by config_dir() within the same test

    #[test]
    fn test_config_dir_uses_xdg_config_home() {
        let dir = tempdir().unwrap();
        let abs_path = dir.path().to_path_buf();

        // Temporarily set XDG_CONFIG_HOME to our tempdir.
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        // SAFETY: test-only; we restore the value immediately after.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &abs_path);
        }

        let result = config_dir();

        // Restore previous value.
        unsafe {
            match prev {
                Some(val) => std::env::set_var("XDG_CONFIG_HOME", val),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }

        assert_eq!(result, Some(abs_path));
    }

    #[test]
    fn test_config_dir_rejects_relative_xdg_config_home() {
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        // SAFETY: test-only; we restore the value immediately after.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", "relative/path");
        }

        let result = config_dir();

        unsafe {
            match prev {
                Some(val) => std::env::set_var("XDG_CONFIG_HOME", val),
                None => std::env::remove_var("XDG_CONFIG_HOME"),
            }
        }

        // A relative path should be ignored; result should be the fallback.
        assert_ne!(result, Some(PathBuf::from("relative/path")));
    }
}
