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

use crate::rrjson::{self, Value};

// ---------------------------------------------------------------------------
// Deep merge
// ---------------------------------------------------------------------------

/// Deep-merge `overlay` into `base`, returning the merged result.
///
/// - Objects are recursively merged (keys in `overlay` override `base`).
/// - Arrays are replaced entirely (not concatenated).
/// - Scalar values in `overlay` replace those in `base`.
#[must_use]
pub fn deep_merge(base: Value, overlay: Value) -> Value {
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

/// A ChordPro configuration loaded from one or more sources.
///
/// Wraps a [`Value::Object`] and provides convenience accessors.
#[derive(Debug, Clone)]
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
        static NULL: Value = Value::Null;
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

    /// Build a configuration by loading and merging from all sources.
    ///
    /// Loads: defaults → system → user → project → song-specific.
    /// Missing files at any level are silently skipped.
    ///
    /// `project_dir` is the directory containing the song file (for
    /// project-level config). `song_config` is an optional path to a
    /// song-specific config file.
    #[must_use]
    pub fn load(project_dir: Option<&str>, song_config: Option<&str>) -> Self {
        let mut config = Self::defaults();

        // System config
        if let Some(text) = read_file_if_exists("/etc/chordpro.json") {
            if let Ok(sys) = Self::parse(&text) {
                config = config.merge(sys);
            }
        }

        // User config
        if let Some(home) = home_dir() {
            let user_path = format!("{home}/.config/chordpro/chordpro.json");
            if let Some(text) = read_file_if_exists(&user_path) {
                if let Ok(user) = Self::parse(&text) {
                    config = config.merge(user);
                }
            }
        }

        // Project config
        if let Some(dir) = project_dir {
            let project_path = format!("{dir}/chordpro.json");
            if let Some(text) = read_file_if_exists(&project_path) {
                if let Ok(proj) = Self::parse(&text) {
                    config = config.merge(proj);
                }
            }
        }

        // Song-specific config
        if let Some(path) = song_config {
            if let Some(text) = read_file_if_exists(path) {
                if let Ok(song) = Self::parse(&text) {
                    config = config.merge(song);
                }
            }
        }

        config
    }
}

/// Read a file to a String, returning None if it doesn't exist or can't be read.
fn read_file_if_exists(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

/// Get the user's home directory.
fn home_dir() -> Option<String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
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
    }
}"#;

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

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
        let config = Config::load(Some("/nonexistent/path"), None);
        // Should still have defaults
        assert!(!config.get("pdf").is_null());
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
}
