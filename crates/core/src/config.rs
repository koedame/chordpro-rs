//! Hierarchical configuration loading for ChordPro.
//!
//! Configuration is loaded from multiple sources in precedence order (later
//! sources override earlier ones):
//!
//! 1. **Built-in defaults** — hardcoded baseline configuration
//! 2. **System config** — `/etc/chordsketch.json` (Linux), platform equivalent
//! 3. **User config** — `~/.config/chordsketch/chordsketch.json`
//! 4. **Project config** — `chordsketch.json` in the song file directory
//! 5. **Song-specific config** — referenced via CLI flag or directive
//!
//! Map values are deep-merged; array values are replaced (not concatenated).
//!
//! # Examples
//!
//! ```
//! use chordsketch_core::config::Config;
//!
//! let config = Config::defaults();
//! assert!(!config.get("pdf").is_null());
//! ```

use crate::rrjson::{self, NULL, Value};

use std::fmt;
#[cfg(not(target_arch = "wasm32"))]
use std::fs::File;
#[cfg(not(target_arch = "wasm32"))]
use std::io::Read;
#[cfg(not(target_arch = "wasm32"))]
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

/// An error from applying a `--define key=value` override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefineError {
    /// The input does not contain an `=` separator.
    MissingEquals,
    /// The key (before `=`) is empty or whitespace-only.
    EmptyKey,
    /// The dotted key contains an empty segment (e.g., `"a..b"`, `".a"`, `"a."`).
    EmptySegment,
    /// The dotted key exceeds the maximum nesting depth.
    ExcessiveDepth,
}

impl fmt::Display for DefineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEquals => write!(f, "invalid --define syntax (expected key=value)"),
            Self::EmptyKey => write!(f, "key must not be empty"),
            Self::EmptySegment => write!(f, "key contains empty segment"),
            Self::ExcessiveDepth => write!(f, "key exceeds maximum nesting depth"),
        }
    }
}

impl std::error::Error for DefineError {}

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
                if let Some((_, base_val_ref)) = base_entries.iter_mut().find(|(k, _)| k == &key) {
                    let old = std::mem::replace(base_val_ref, Value::Null);
                    *base_val_ref = deep_merge(old, overlay_val);
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

/// Result of resolving a configuration name (preset or file path),
/// including any non-fatal warnings from RRJSON parsing.
#[derive(Debug)]
pub struct ConfigResolveResult {
    /// The resolved configuration.
    pub config: Config,
    /// Non-fatal warnings (e.g., unsupported RRJSON include directives).
    pub warnings: Vec<String>,
}

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
    #[must_use = "parse errors should not be silently discarded"]
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
    /// Symlinks and files exceeding the 10 MB file size limit are rejected,
    /// matching the behavior of hierarchical config loading.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Io`] if the file cannot be read, or
    /// [`ConfigError::Parse`] if the content is malformed.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use = "config resolution errors should not be silently discarded"]
    pub fn resolve(name: &str) -> Result<ConfigResolveResult, ConfigError> {
        // Try preset first
        if let Some(preset) = Self::preset(name) {
            return Ok(ConfigResolveResult {
                config: preset,
                warnings: Vec::new(),
            });
        }
        // Try as a file path — apply the same security checks as hierarchical config
        // CLI --config files are trusted (user-provided paths), so use the
        // full MAX_CONFIG_FILE_SIZE limit.
        let text = read_config_file(Path::new(name), MAX_CONFIG_FILE_SIZE).map_err(|e| {
            ConfigError::Io {
                path: name.to_string(),
                source: e,
            }
        })?;
        let result = rrjson::parse_rrjson_with_warnings(&text).map_err(|e| ConfigError::Parse {
            path: name.to_string(),
            source: e,
        })?;
        Ok(ConfigResolveResult {
            config: Self { root: result.value },
            warnings: result.warnings,
        })
    }

    /// Apply a single `key=value` define override, consuming `self`.
    ///
    /// The key may be dot-separated (e.g., `pdf.chorus.indent=20`).
    /// The value is parsed as RRJSON (so `20` becomes a number, `"hello"`
    /// becomes a string, etc.). If the value cannot be parsed, it is
    /// treated as a plain string.
    ///
    /// # Errors
    ///
    /// Returns [`DefineError`] if the input has no `=`, the key is empty,
    /// or the dotted key exceeds the maximum nesting depth.
    #[must_use = "define errors should not be silently discarded"]
    pub fn with_define(mut self, define: &str) -> Result<Self, DefineError> {
        self.apply_define(define)?;
        Ok(self)
    }

    /// Apply a single `key=value` define override in place.
    ///
    /// Same semantics as [`with_define`](Self::with_define) but mutates
    /// `self` instead of consuming it, avoiding a clone when the caller
    /// needs to retain ownership on error.
    ///
    /// # Errors
    ///
    /// Returns [`DefineError`] if the input has no `=`, the key is empty,
    /// or the dotted key exceeds the maximum nesting depth. On error,
    /// `self` is not modified.
    #[must_use = "define errors should not be silently discarded"]
    pub fn apply_define(&mut self, define: &str) -> Result<(), DefineError> {
        let Some(eq_pos) = define.find('=') else {
            return Err(DefineError::MissingEquals);
        };
        let key = define[..eq_pos].trim();
        let raw_value = define[eq_pos + 1..].trim();
        if key.is_empty() {
            return Err(DefineError::EmptyKey);
        }
        if key.split('.').any(|s| s.is_empty()) {
            return Err(DefineError::EmptySegment);
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
        let Some(overlay) = build_nested_value(key, value) else {
            return Err(DefineError::ExcessiveDepth);
        };
        let root = std::mem::replace(&mut self.root, Value::Null);
        self.root = deep_merge(root, overlay);
        Ok(())
    }

    /// Apply song-level config overrides from `{+config.KEY: VALUE}` directives.
    ///
    /// Returns a new `Config` with the overrides applied, plus any warnings
    /// for disallowed keys. Only keys under explicitly allowed top-level
    /// prefixes may be overridden from song-level directives (allowlist
    /// approach). This is secure by default: newly added config sections
    /// are blocked until explicitly whitelisted.
    #[must_use]
    pub fn with_song_overrides(
        self,
        overrides: &[(&str, &str)],
        warnings: &mut Vec<String>,
    ) -> Self {
        /// Top-level config key prefixes that are safe for song-level
        /// overrides. These control rendering and display only — no
        /// external tool execution or filesystem access.
        const ALLOWED_PREFIXES: &[&str] = &[
            "settings.",
            "pdf.",
            "html.",
            "chords.",
            "metadata.",
            "instrument.",
            "diagrams.",
        ];

        /// Top-level config keys (without sub-keys) that are safe for
        /// song-level overrides. These require exact match, not prefix.
        const ALLOWED_EXACT_KEYS: &[&str] = &["tuning"];

        /// Maximum number of song-level config overrides processed per song.
        /// Prevents CPU exhaustion from malicious inputs with thousands of
        /// `{+config.*}` directives.
        const MAX_SONG_OVERRIDES: usize = 1000;

        let mut config = self;
        if overrides.len() > MAX_SONG_OVERRIDES {
            warnings.push(format!(
                "too many song-level config overrides ({}, max {}); excess ignored",
                overrides.len(),
                MAX_SONG_OVERRIDES
            ));
        }
        for &(key, value) in overrides.iter().take(MAX_SONG_OVERRIDES) {
            let allowed = ALLOWED_PREFIXES
                .iter()
                .any(|prefix| key.starts_with(prefix))
                || ALLOWED_EXACT_KEYS.contains(&key);
            if !allowed {
                warnings.push(format!(
                    "{key} cannot be overridden from a song-level config directive"
                ));
                continue;
            }
            if let Err(e) = config.apply_define(&format!("{key}={value}")) {
                warnings.push(format!("failed to apply song override {key}={value}: {e}"));
            }
        }
        config
    }

    /// Extract the `settings.transpose` value from song-level config overrides.
    ///
    /// Scans the override list in reverse order (last wins) for
    /// `settings.transpose` and parses it as an `i8` semitone delta.
    /// Returns `0` if the key is absent or the value is not a valid number.
    #[must_use]
    pub fn song_transpose_delta(overrides: &[(&str, &str)]) -> i8 {
        for &(key, value) in overrides.iter().rev() {
            if key == "settings.transpose" {
                return value
                    .trim()
                    .parse::<f64>()
                    .unwrap_or(0.0)
                    .clamp(f64::from(i8::MIN), f64::from(i8::MAX)) as i8;
            }
        }
        0
    }

    /// Build a configuration by loading and merging from all sources.
    ///
    /// Loads: defaults → system → user → project → song-specific.
    /// Missing files at any level are silently skipped.
    ///
    /// System and user configs are trusted and may be up to 10 MB.
    /// Project and song-specific configs are untrusted and limited to 1 MB.
    ///
    /// `project_dir` is the directory containing the song file (for
    /// project-level config). `song_config` is an optional path to a
    /// song-specific config file.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn load(project_dir: Option<&str>, song_config: Option<&str>) -> ConfigLoadResult {
        let mut config = Self::defaults();
        let mut warnings = Vec::new();

        // System config (trusted — full size limit)
        let system_path = PathBuf::from("/etc/chordsketch.json");
        if let Some(text) = read_file_if_exists(&system_path, MAX_CONFIG_FILE_SIZE, &mut warnings) {
            match Self::parse_collecting_warnings(&text, &mut warnings) {
                Ok(sys) => config = config.merge(sys),
                Err(e) => warnings.push(format!(
                    "failed to parse config file {}: {e}",
                    system_path.display()
                )),
            }
        }

        // User config (trusted — full size limit): respect XDG_CONFIG_HOME,
        // fall back to $HOME/.config.
        if let Some(config_dir) = config_dir() {
            let user_path = config_dir.join("chordsketch").join("chordsketch.json");
            if let Some(text) = read_file_if_exists(&user_path, MAX_CONFIG_FILE_SIZE, &mut warnings)
            {
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
        // delegate execution — only CLI flags or explicit user config
        // (~/.config/chordsketch/) may enable delegates.
        let trusted_abc2svg = config.get_path("delegates.abc2svg").clone();
        let trusted_lilypond = config.get_path("delegates.lilypond").clone();
        let trusted_musescore = config.get_path("delegates.musescore").clone();

        // Project config (untrusted — reduced size limit)
        if let Some(dir) = project_dir {
            let project_path = PathBuf::from(dir).join("chordsketch.json");
            if let Some(text) =
                read_file_if_exists(&project_path, MAX_UNTRUSTED_CONFIG_FILE_SIZE, &mut warnings)
            {
                match Self::parse_collecting_warnings(&text, &mut warnings) {
                    Ok(proj) => config = config.merge(proj),
                    Err(e) => warnings.push(format!(
                        "failed to parse config file {}: {e}",
                        project_path.display()
                    )),
                }
            }
        }

        // Song-specific config (untrusted — reduced size limit)
        if let Some(path) = song_config {
            if let Some(text) = read_file_if_exists(
                Path::new(path),
                MAX_UNTRUSTED_CONFIG_FILE_SIZE,
                &mut warnings,
            ) {
                match Self::parse_collecting_warnings(&text, &mut warnings) {
                    Ok(song) => config = config.merge(song),
                    Err(e) => warnings.push(format!("failed to parse config file {path}: {e}")),
                }
            }
        }

        // Restore delegate settings to trusted values if an untrusted config
        // made them more permissive. Permissiveness order:
        //   Bool(false) < Null (auto-detect) < Bool(true)
        // Any escalation is blocked: false→null, false→true, null→true.
        fn delegate_perm(v: &Value) -> u8 {
            match v.as_bool() {
                Some(false) => 0,
                None => 1, // Null or non-bool → auto-detect level
                Some(true) => 2,
            }
        }

        let current_abc2svg = config.get_path("delegates.abc2svg").clone();
        let current_lilypond = config.get_path("delegates.lilypond").clone();
        let current_musescore = config.get_path("delegates.musescore").clone();

        if delegate_perm(&current_abc2svg) > delegate_perm(&trusted_abc2svg) {
            // trusted is either null (perm 1) or false (perm 0). If it were
            // true (perm 2), no current value could exceed it.
            let reset = if trusted_abc2svg.is_null() {
                "null"
            } else {
                debug_assert_eq!(trusted_abc2svg.as_bool(), Some(false));
                "false"
            };
            config = config
                .with_define(&format!("delegates.abc2svg={reset}"))
                .expect("hardcoded define is valid");
            let explanation = if reset == "null" {
                "reset to null (auto-detect: enabled only if abc2svg is installed)"
            } else {
                "reset to false (disabled)"
            };
            warnings.push(format!(
                "delegates.abc2svg was escalated by an untrusted config and has been \
                 {explanation} for security; use --define delegates.abc2svg=true to enable"
            ));
        }
        if delegate_perm(&current_lilypond) > delegate_perm(&trusted_lilypond) {
            // trusted is either null (perm 1) or false (perm 0). See abc2svg comment above.
            let reset = if trusted_lilypond.is_null() {
                "null"
            } else {
                debug_assert_eq!(trusted_lilypond.as_bool(), Some(false));
                "false"
            };
            config = config
                .with_define(&format!("delegates.lilypond={reset}"))
                .expect("hardcoded define is valid");
            let explanation = if reset == "null" {
                "reset to null (auto-detect: enabled only if lilypond is installed)"
            } else {
                "reset to false (disabled)"
            };
            warnings.push(format!(
                "delegates.lilypond was escalated by an untrusted config and has been \
                 {explanation} for security; use --define delegates.lilypond=true to enable"
            ));
        }
        if delegate_perm(&current_musescore) > delegate_perm(&trusted_musescore) {
            let reset = if trusted_musescore.is_null() {
                "null"
            } else {
                debug_assert_eq!(trusted_musescore.as_bool(), Some(false));
                "false"
            };
            config = config
                .with_define(&format!("delegates.musescore={reset}"))
                .expect("hardcoded define is valid");
            let explanation = if reset == "null" {
                "reset to null (auto-detect: enabled only if mscore/musescore is installed)"
            } else {
                "reset to false (disabled)"
            };
            warnings.push(format!(
                "delegates.musescore was escalated by an untrusted config and has been \
                 {explanation} for security; use --define delegates.musescore=true to enable"
            ));
        }

        ConfigLoadResult { config, warnings }
    }

    /// Parse a configuration from a RRJSON string, collecting warnings into
    /// the provided vector.
    #[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
/// Maximum config file size for trusted sources (10 MB).
///
/// Applies to system configs (`/etc/chordsketch.json`), user configs
/// (`~/.config/chordsketch/chordsketch.json`), and CLI `--config` files.
const MAX_CONFIG_FILE_SIZE: u64 = 10 * 1024 * 1024;

#[cfg(not(target_arch = "wasm32"))]
/// Maximum config file size for untrusted sources (1 MB).
///
/// Applies to project-level (`chordsketch.json` in song directory) and
/// song-specific config files. These are less trusted and typically
/// much smaller than system/user configs, so a tighter limit reduces
/// the attack surface.
const MAX_UNTRUSTED_CONFIG_FILE_SIZE: u64 = 1024 * 1024;

#[cfg(not(target_arch = "wasm32"))]
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
        // O_NOFOLLOW values from platform fcntl(2) headers.
        // Note: chordsketch-core has a zero-dependency policy, so we cannot
        // use the `libc` crate for portable O_NOFOLLOW constants.
        //
        // References:
        //   Linux:   include/uapi/asm-generic/fcntl.h  — 0o400000 (0x20000)
        //   macOS:   sys/fcntl.h                        — 0x0100
        //   FreeBSD: sys/fcntl.h                        — 0x0100
        //   OpenBSD: sys/fcntl.h                        — 0x0100
        #[cfg(target_os = "linux")]
        const O_NOFOLLOW: i32 = 0o400000;
        #[cfg(target_os = "macos")]
        const O_NOFOLLOW: i32 = 0x0100;
        #[cfg(target_os = "freebsd")]
        const O_NOFOLLOW: i32 = 0x0100;
        #[cfg(target_os = "openbsd")]
        const O_NOFOLLOW: i32 = 0x0100;
        // On other Unix platforms the O_NOFOLLOW value is unknown and we
        // fall back to 0, which disables kernel-level symlink protection.
        // The pre-open symlink_metadata() check in read_config_file()
        // still provides TOCTOU-window-limited defense.
        //
        // WARNING: If you are porting to a new Unix platform, add the
        // correct O_NOFOLLOW constant above to enable atomic symlink
        // rejection.
        #[cfg(not(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "freebsd",
            target_os = "openbsd"
        )))]
        const O_NOFOLLOW: i32 = {
            // Emit a compile-time warning on unsupported Unix platforms.
            #[deprecated(note = "O_NOFOLLOW is not defined for this Unix platform; \
                kernel-level symlink protection is disabled. Add the platform's \
                O_NOFOLLOW constant to open_no_follow() for full protection.")]
            const UNSUPPORTED: i32 = 0;
            UNSUPPORTED
        };

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

#[cfg(not(target_arch = "wasm32"))]
/// Read a config file with symlink and size checks.
///
/// 1. Pre-open `symlink_metadata` check for a user-friendly error message.
/// 2. On Unix: opens with `O_NOFOLLOW` to atomically reject symlinks at
///    the kernel level, eliminating the TOCTOU window between the metadata
///    check and file open.
/// 3. On non-Unix: retains the pre-open metadata check only.
/// 4. Checks metadata on the file descriptor to enforce `max_size`.
fn read_config_file(path: &Path, max_size: u64) -> Result<String, std::io::Error> {
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
    let file = open_no_follow(path)?;
    let fd_meta = file.metadata()?;

    if fd_meta.len() > max_size {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "config file size {} exceeds {} byte limit: {}",
                fd_meta.len(),
                max_size,
                path.display()
            ),
        ));
    }

    // Use Read::take() to hard-cap the read as defense-in-depth, in case
    // the metadata size is inaccurate (e.g., FUSE or synthetic filesystems).
    let mut contents = String::new();
    file.take(max_size + 1).read_to_string(&mut contents)?;
    if contents.len() as u64 > max_size {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "config file read size exceeds {} byte limit: {}",
                max_size,
                path.display()
            ),
        ));
    }
    Ok(contents)
}

#[cfg(not(target_arch = "wasm32"))]
/// Read a file to a String, returning None if it doesn't exist or can't be read.
///
/// Rejects files that are symlinks or exceed `max_size`, emitting a warning
/// in either case.
fn read_file_if_exists(path: &Path, max_size: u64, warnings: &mut Vec<String>) -> Option<String> {
    match read_config_file(path, max_size) {
        Ok(contents) => Some(contents),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            warnings.push(format!("skipping config file {}: {e}", path.display()));
            None
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Get the user's home directory as a `PathBuf`.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(not(target_arch = "wasm32"))]
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

    // Delegate environments (external tool integration).
    // null = auto-detect on first use; true = force enable; false = force disable.
    delegates: {
        abc2svg: null,
        lilypond: null
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

    /// Map a delegate config value to a permissiveness level for ordering.
    /// Matches the `delegate_perm` function used in `Config::load()`.
    fn delegate_perm(v: &Value) -> u8 {
        match v.as_bool() {
            Some(false) => 0,
            None => 1, // Null or non-bool → auto-detect level
            Some(true) => 2,
        }
    }

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
        let config = Config::empty().with_define("key=42").unwrap();
        assert_eq!(config.get("key"), &Value::Number(42.0));
    }

    #[test]
    fn test_define_string() {
        let config = Config::empty().with_define(r#"key="hello""#).unwrap();
        assert_eq!(config.get("key"), &Value::String("hello".to_string()));
    }

    #[test]
    fn test_define_dotted_key() {
        let config = Config::empty().with_define("pdf.chorus.indent=20").unwrap();
        assert_eq!(config.get_path("pdf.chorus.indent"), &Value::Number(20.0));
    }

    #[test]
    fn test_define_overrides_existing() {
        let config = Config::defaults()
            .with_define("pdf.margins.top=100")
            .unwrap();
        assert_eq!(config.get_path("pdf.margins.top"), &Value::Number(100.0));
        // Other margins should be unchanged
        assert_eq!(config.get_path("pdf.margins.left"), &Value::Number(56.0));
    }

    #[test]
    fn test_define_bool() {
        let config = Config::empty().with_define("flag=true").unwrap();
        assert_eq!(config.get("flag"), &Value::Bool(true));
    }

    #[test]
    fn test_define_unquoted_string_fallback() {
        // Values that aren't valid JSON fall back to string
        let config = Config::empty().with_define("key=hello world").unwrap();
        assert_eq!(config.get("key"), &Value::String("hello world".to_string()));
    }

    #[test]
    fn test_define_no_equals_returns_error() {
        let result = Config::empty().with_define("noequalssign");
        assert_eq!(result.unwrap_err(), DefineError::MissingEquals);
    }

    #[test]
    fn test_define_empty_key_returns_error() {
        let result = Config::empty().with_define("=value");
        assert_eq!(result.unwrap_err(), DefineError::EmptyKey);
    }

    #[test]
    fn test_define_whitespace_key_returns_error() {
        let result = Config::empty().with_define("  =value");
        assert_eq!(result.unwrap_err(), DefineError::EmptyKey);
    }

    #[test]
    fn test_multiple_defines() {
        let config = Config::empty()
            .with_define("a=1")
            .unwrap()
            .with_define("b=2")
            .unwrap()
            .with_define("a=3")
            .unwrap();
        assert_eq!(config.get("a"), &Value::Number(3.0));
        assert_eq!(config.get("b"), &Value::Number(2.0));
    }

    #[test]
    fn test_define_excessive_depth_rejected() {
        // A dotted key with more than MAX_DEFINE_DEPTH segments should be rejected.
        let segments: Vec<String> = (0..=MAX_DEFINE_DEPTH).map(|i| format!("k{i}")).collect();
        let deep_key = segments.join(".");
        let result = Config::empty().with_define(&format!("{deep_key}=1"));
        assert_eq!(result.unwrap_err(), DefineError::ExcessiveDepth);
    }

    #[test]
    fn test_define_double_dot_rejected() {
        let result = Config::empty().with_define("a..b=1");
        assert_eq!(result.unwrap_err(), DefineError::EmptySegment);
    }

    #[test]
    fn test_define_leading_dot_rejected() {
        let result = Config::empty().with_define(".a=1");
        assert_eq!(result.unwrap_err(), DefineError::EmptySegment);
    }

    #[test]
    fn test_define_trailing_dot_rejected() {
        let result = Config::empty().with_define("a.=1");
        assert_eq!(result.unwrap_err(), DefineError::EmptySegment);
    }

    #[test]
    fn test_define_at_max_depth_accepted() {
        // Exactly MAX_DEFINE_DEPTH segments should be accepted.
        let segments: Vec<String> = (0..MAX_DEFINE_DEPTH).map(|i| format!("k{i}")).collect();
        let key = segments.join(".");
        let config = Config::empty().with_define(&format!("{key}=42")).unwrap();
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
        let result = Config::resolve("guitar").expect("guitar should resolve");
        assert!(result.warnings.is_empty());
        assert_eq!(
            result.config.get_path("instrument.type"),
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
        let config = Config::empty().with_define("key=").unwrap();
        assert_eq!(config.get("key"), &Value::String(String::new()));
    }

    // -- Integration tests for Config::load() with actual files ---------------

    #[test]
    fn test_load_project_config() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("chordsketch.json"),
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
            project_dir.path().join("chordsketch.json"),
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
            dir.path().join("chordsketch.json"),
            r#"{ "delegates": { "abc2svg": true, "lilypond": true } }"#,
        )
        .unwrap();

        let result = Config::load(Some(dir.path().to_str().unwrap()), None);
        // Delegates should be reset to null (auto-detect default)
        assert_eq!(result.config.get_path("delegates.abc2svg"), &Value::Null);
        assert_eq!(result.config.get_path("delegates.lilypond"), &Value::Null);
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
    fn test_load_project_null_does_not_escalate_default_null() {
        // Config::load() with a project config that sets delegates to null.
        // Default trusted baseline is null, so null→null is not an escalation
        // and no warning should be emitted.
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("chordsketch.json"),
            r#"{ "delegates": { "abc2svg": null } }"#,
        )
        .unwrap();

        let result = Config::load(Some(dir.path().to_str().unwrap()), None);
        assert_eq!(result.config.get_path("delegates.abc2svg"), &Value::Null);
        assert!(
            !result
                .warnings
                .iter()
                .any(|w| w.contains("delegates.abc2svg")),
            "null→null should not trigger a delegate warning: {:?}",
            result.warnings
        );
    }

    #[test]
    fn test_song_override_blocks_delegate_key() {
        // Song overrides block all `delegates.*` keys because they are not
        // on the allowlist (ALLOWED_PREFIXES / ALLOWED_EXACT_KEYS). This is
        // not escalation-based — any song override of a delegate key is
        // rejected regardless of the current or target value.
        let config = Config::defaults()
            .with_define("delegates.abc2svg=false")
            .expect("hardcoded");
        let mut warnings = Vec::new();
        let overrides = vec![("delegates.abc2svg", "null")];
        let config = config.with_song_overrides(&overrides, &mut warnings);
        assert_eq!(
            config.get_path("delegates.abc2svg"),
            &Value::Bool(false),
            "delegate key should remain unchanged — not on song-override allowlist"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("delegates.abc2svg") && w.contains("cannot be overridden")),
            "should warn about blocked delegate override: {warnings:?}"
        );
    }

    #[test]
    fn test_project_config_cannot_escalate_false_to_null() {
        // Verify that null (auto-detect) is more permissive than false (disabled).
        // The merge itself changes the value; the security check in load() restores it.
        let mut config = Config::defaults()
            .with_define("delegates.abc2svg=false")
            .expect("hardcoded");
        let overlay = Config::parse(r#"{ "delegates": { "abc2svg": null } }"#).unwrap();
        config = config.merge(overlay);
        // After merge the value is Null — the security check must detect this
        // as an escalation (false → null) and restore to false.
        assert_eq!(
            config.get_path("delegates.abc2svg"),
            &Value::Null,
            "merge changes false to null"
        );

        // Verify the permissiveness ordering used by the security check.
        assert!(
            delegate_perm(&Value::Null) > delegate_perm(&Value::Bool(false)),
            "null (auto-detect) must be more permissive than false (disabled)"
        );
        assert!(
            delegate_perm(&Value::Bool(true)) > delegate_perm(&Value::Null),
            "true must be more permissive than null (auto-detect)"
        );
    }

    #[test]
    fn test_project_config_can_downgrade_delegates() {
        // Downgrading from null (auto) to false (disabled) is safe and allowed.
        assert!(
            delegate_perm(&Value::Bool(false)) <= delegate_perm(&Value::Null),
            "false should not be considered an escalation over null"
        );
    }

    #[test]
    fn test_load_invalid_project_config_continues() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("chordsketch.json"), "{ invalid json !!!").unwrap();

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

        let result = Config::resolve(file_path.to_str().unwrap()).unwrap();
        assert_eq!(result.config.get("custom"), &Value::Bool(true));
    }

    // -- File size / symlink guard tests --------------------------------------

    #[test]
    fn test_read_file_if_exists_normal_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");
        std::fs::write(&file_path, r#"{"key": "value"}"#).unwrap();

        let mut warnings = Vec::new();
        let result = read_file_if_exists(&file_path, MAX_CONFIG_FILE_SIZE, &mut warnings);
        assert!(result.is_some());
        assert!(result.unwrap().contains("key"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_read_file_if_exists_nonexistent() {
        let mut warnings = Vec::new();
        let result = read_file_if_exists(
            Path::new("/nonexistent/path/config.json"),
            MAX_CONFIG_FILE_SIZE,
            &mut warnings,
        );
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
        let result = read_file_if_exists(&link_path, MAX_CONFIG_FILE_SIZE, &mut warnings);
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

        let text = read_config_file(&file_path, MAX_CONFIG_FILE_SIZE).unwrap();
        assert!(text.contains("ok"));
    }

    #[test]
    fn test_read_config_file_nonexistent() {
        let result = read_config_file(
            Path::new("/nonexistent/path/config.json"),
            MAX_CONFIG_FILE_SIZE,
        );
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

        let result = read_config_file(&link_path, MAX_CONFIG_FILE_SIZE);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    }

    // -- Untrusted config size limit tests ----------------------------------------

    #[test]
    fn test_read_config_file_rejects_oversized_untrusted() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");
        // Write a file that exceeds MAX_UNTRUSTED_CONFIG_FILE_SIZE (1 MB)
        // but is under MAX_CONFIG_FILE_SIZE (10 MB).
        let content = "x".repeat(MAX_UNTRUSTED_CONFIG_FILE_SIZE as usize + 1);
        std::fs::write(&file_path, &content).unwrap();

        // Should fail with the untrusted limit.
        let result = read_config_file(&file_path, MAX_UNTRUSTED_CONFIG_FILE_SIZE);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);

        // Should succeed with the trusted limit.
        let result = read_config_file(&file_path, MAX_CONFIG_FILE_SIZE);
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_file_if_exists_warns_on_oversized_untrusted() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("config.json");
        let content = "x".repeat(MAX_UNTRUSTED_CONFIG_FILE_SIZE as usize + 1);
        std::fs::write(&file_path, &content).unwrap();

        let mut warnings = Vec::new();
        let result = read_file_if_exists(&file_path, MAX_UNTRUSTED_CONFIG_FILE_SIZE, &mut warnings);
        assert!(result.is_none(), "oversized file should be rejected");
        assert!(!warnings.is_empty(), "should produce a warning");
        assert!(
            warnings[0].contains("byte limit"),
            "warning should mention byte limit"
        );
    }

    // -- XDG_CONFIG_HOME tests ---------------------------------------------------
    // Environment variables are process-global; tests that mutate them must not
    // run concurrently.  All XDG_CONFIG_HOME tests acquire ENV_LOCK before
    // touching the environment and release it (via drop) before asserting.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_config_dir_uses_xdg_config_home() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let dir = tempdir().unwrap();
        let abs_path = dir.path().to_path_buf();

        let prev = std::env::var_os("XDG_CONFIG_HOME");
        // SAFETY: test-only; we hold ENV_LOCK for the duration.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", &abs_path);
        }

        let result = config_dir();

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
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let prev = std::env::var_os("XDG_CONFIG_HOME");
        // SAFETY: test-only; we hold ENV_LOCK for the duration.
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

        // A relative path must be ignored; fall back to HOME/.config or None.
        assert_ne!(result, Some(PathBuf::from("relative/path")));
    }

    // -- with_song_overrides tests --------------------------------------------

    #[test]
    fn test_song_overrides_apply() {
        let config = Config::defaults();
        let mut warnings = Vec::new();
        let overrides = vec![("pdf.margins.top", "100")];
        let config = config.with_song_overrides(&overrides, &mut warnings);
        assert_eq!(config.get_path("pdf.margins.top"), &Value::Number(100.0));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_song_overrides_multiple() {
        let config = Config::defaults();
        let mut warnings = Vec::new();
        let overrides = vec![("pdf.margins.top", "100"), ("settings.transpose", "3")];
        let config = config.with_song_overrides(&overrides, &mut warnings);
        assert_eq!(config.get_path("pdf.margins.top"), &Value::Number(100.0));
        assert_eq!(config.get_path("settings.transpose"), &Value::Number(3.0));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_song_overrides_block_delegates() {
        let config = Config::defaults();
        let mut warnings = Vec::new();
        let overrides = vec![
            ("delegates.abc2svg", "true"),
            ("delegates.lilypond", "true"),
        ];
        let config = config.with_song_overrides(&overrides, &mut warnings);
        // Delegate keys should remain at their default (null = auto-detect)
        assert_eq!(config.get_path("delegates.abc2svg"), &Value::Null);
        assert_eq!(config.get_path("delegates.lilypond"), &Value::Null);
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("delegates.abc2svg"));
        assert!(warnings[1].contains("delegates.lilypond"));
    }

    #[test]
    fn test_song_overrides_block_unknown_keys() {
        let config = Config::defaults();
        let mut warnings = Vec::new();
        let overrides = vec![("some_new_key.dangerous", "true")];
        let config = config.with_song_overrides(&overrides, &mut warnings);
        assert_eq!(config.get_path("some_new_key.dangerous"), &Value::Null);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("some_new_key.dangerous"));
    }

    #[test]
    fn test_song_overrides_allow_all_safe_prefixes() {
        let config = Config::defaults();
        let mut warnings = Vec::new();
        let overrides = vec![
            ("settings.transpose", "2"),
            ("pdf.papersize", "\"letter\""),
            ("html.styles.body", "\"color: red;\""),
            ("chords.show", "\"none\""),
            ("metadata.separator", "\", \""),
            ("instrument.type", "\"ukulele\""),
            ("diagrams.frets", "4"),
        ];
        let config = config.with_song_overrides(&overrides, &mut warnings);
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        assert_eq!(config.get_path("settings.transpose"), &Value::Number(2.0));
        assert_eq!(
            config.get_path("pdf.papersize"),
            &Value::String("letter".to_string())
        );
        assert_eq!(
            config.get_path("chords.show"),
            &Value::String("none".to_string())
        );
    }

    #[test]
    fn test_song_overrides_empty_is_noop() {
        let config = Config::defaults();
        let original_top = config.get_path("pdf.margins.top").clone();
        let mut warnings = Vec::new();
        let config = config.with_song_overrides(&[], &mut warnings);
        assert_eq!(config.get_path("pdf.margins.top"), &original_top);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_song_overrides_do_not_persist_across_songs() {
        let base = Config::defaults();
        let mut warnings = Vec::new();

        // Song 1 overrides margins
        let song1_config = base
            .clone()
            .with_song_overrides(&[("pdf.margins.top", "100")], &mut warnings);
        assert_eq!(
            song1_config.get_path("pdf.margins.top"),
            &Value::Number(100.0)
        );

        // Song 2 uses the same base config — should NOT see song 1's override
        let song2_config = base.clone().with_song_overrides(&[], &mut warnings);
        assert_eq!(
            song2_config.get_path("pdf.margins.top"),
            &Value::Number(56.0)
        );
    }

    #[test]
    fn test_song_overrides_reject_tuning_prefix_mismatch() {
        let config = Config::defaults();
        let mut warnings = Vec::new();
        let overrides = vec![("tuningXYZ", "true")];
        let config = config.with_song_overrides(&overrides, &mut warnings);
        assert_eq!(config.get_path("tuningXYZ"), &Value::Null);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("tuningXYZ"));
    }

    #[test]
    fn test_song_overrides_accept_tuning_exact() {
        let config = Config::defaults();
        let mut warnings = Vec::new();
        let overrides = vec![("tuning", "[\"E2\",\"A2\",\"D3\",\"G3\",\"B3\",\"E4\"]")];
        let config = config.with_song_overrides(&overrides, &mut warnings);
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
        match config.get("tuning") {
            Value::Array(arr) => assert_eq!(arr.len(), 6),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn test_song_overrides_malformed_value_warns() {
        let config = Config::defaults();
        let mut warnings = Vec::new();
        // Key with no '=' inside the value — with_define will fail
        // We simulate this by passing a key that causes with_define to error.
        // with_define expects "key=value", and with_song_overrides formats it as
        // "{key}={value}", so with_define should always succeed for non-empty key+value.
        // However, an empty key (after the prefix check) would be caught by with_define.
        // We test the warning path by using a key that passes the allowlist but
        // causes with_define to fail due to excessive depth.
        let deep_key = "pdf.".to_string() + &"a.".repeat(100) + "x";
        let overrides = vec![(deep_key.as_str(), "1")];
        let config = config.with_song_overrides(&overrides, &mut warnings);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("failed to apply song override"));
        // Config should remain unchanged
        assert_eq!(config.get_path("pdf.margins.top"), &Value::Number(56.0));
    }

    #[test]
    fn test_song_overrides_excess_count_warns() {
        let config = Config::defaults();
        let mut warnings = Vec::new();
        // Create 1002 overrides — first 1000 should be applied, rest ignored.
        let overrides: Vec<(&str, &str)> = (0..1002).map(|_| ("settings.transpose", "1")).collect();
        let _config = config.with_song_overrides(&overrides, &mut warnings);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("too many song-level config overrides"));
    }

    // --- Additional edge case coverage (#580) ---

    #[test]
    fn test_deep_merge_type_change_object_to_scalar() {
        // When overlay replaces an object with a scalar, the scalar wins.
        let base = crate::rrjson::parse_rrjson(r#"{"a": {"b": 1}}"#).unwrap();
        let overlay = crate::rrjson::parse_rrjson(r#"{"a": 42}"#).unwrap();
        let merged = deep_merge(base, overlay);
        if let Value::Object(entries) = merged {
            let val = entries.iter().find(|(k, _)| k == "a").map(|(_, v)| v);
            assert_eq!(val, Some(&Value::Number(42.0)));
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_resolve_propagates_rrjson_warnings() {
        // RRJSON include directives produce warnings. Verify they propagate
        // through Config::resolve.
        let dir = tempfile::TempDir::new().unwrap();
        let file_path = dir.path().join("test.prp");
        std::fs::write(&file_path, "include \"nonexistent.prp\"\na = 1\n").unwrap();
        let result = Config::resolve(file_path.to_str().unwrap()).unwrap();
        assert!(
            !result.warnings.is_empty(),
            "RRJSON warnings should propagate through Config::resolve"
        );
    }

    #[test]
    fn test_define_value_with_braces() {
        let config = Config::defaults().with_define("settings.note={C}").unwrap();
        assert_eq!(
            config.get_path("settings.note"),
            &Value::String("{C}".to_string())
        );
    }

    #[test]
    fn test_define_value_with_equals() {
        let config = Config::defaults().with_define("settings.expr=a=b").unwrap();
        assert_eq!(
            config.get_path("settings.expr"),
            &Value::String("a=b".to_string())
        );
    }

    #[test]
    fn test_song_config_plus_delegate_blocking() {
        // Song-level config cannot enable delegates.
        let config = Config::defaults();
        let mut warnings = Vec::new();
        let overrides = vec![("delegates.abc2svg", "true")];
        let _result = config.with_song_overrides(&overrides, &mut warnings);
        // Delegate keys should be blocked — a warning should be emitted.
        assert!(
            warnings.iter().any(|w| w.contains("cannot be overridden")),
            "delegate override should be blocked: {warnings:?}"
        );
    }
}
