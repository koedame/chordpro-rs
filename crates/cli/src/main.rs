//! ChordPro command-line tool.
//!
//! Parses `.cho` / `.chordpro` files and renders them to text, HTML, or PDF.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use clap::Parser;

/// ChordPro file processor — parse and render ChordPro songs.
#[derive(Parser)]
#[command(name = "chordpro", version, about)]
struct Cli {
    /// Input ChordPro file(s) to process.
    #[arg(required = true)]
    files: Vec<String>,

    /// Write output to a file instead of stdout.
    #[arg(short, long)]
    output: Option<String>,

    /// Output format.
    #[arg(short, long, default_value = "text")]
    format: Format,

    /// Transpose all chords by N semitones (positive = up, negative = down).
    #[arg(short, long, default_value = "0")]
    transpose: i8,

    /// Load a custom configuration file or preset name (may be specified multiple times).
    ///
    /// Paths are treated as trusted input — the file is read without path restrictions.
    /// Do not pass untrusted paths when invoking chordpro programmatically.
    #[arg(short = 'c', long = "config")]
    configs: Vec<String>,

    /// Set a config value at runtime (highest precedence). Format: key=value.
    #[arg(short = 'D', long = "define")]
    defines: Vec<String>,

    /// Skip loading system, user, and project config files.
    /// Only built-in defaults are used as the base. --config and --define still apply.
    #[arg(long = "no-default-configs")]
    no_default_configs: bool,
}

/// Supported output formats.
#[derive(Clone, Debug, clap::ValueEnum)]
enum Format {
    /// Plain text with chords above lyrics.
    Text,
    /// Self-contained HTML5 document.
    Html,
    /// PDF document (A4, Helvetica).
    Pdf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Derive project directory from the first input file for project-level config.
    let project_dir = cli
        .files
        .first()
        .and_then(|f| Path::new(f).parent())
        .and_then(|p| p.to_str())
        .map(|s| s.to_string());

    // Build configuration: defaults → system → user → project → custom config files → defines
    let mut config = if cli.no_default_configs {
        chordpro_core::config::Config::defaults()
    } else {
        let result = chordpro_core::config::Config::load(project_dir.as_deref(), None);
        for warning in &result.warnings {
            eprintln!("warning: {warning}");
        }
        result.config
    };

    // Apply --config files/presets in order (preset names resolved first)
    for config_name in &cli.configs {
        match chordpro_core::config::Config::resolve(config_name) {
            Ok(result) => {
                for warning in &result.warnings {
                    eprintln!("warning: {config_name}: {warning}");
                }
                config = config.merge(result.config);
            }
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    // Apply --define overrides (highest precedence)
    for define in &cli.defines {
        match config.with_define(define) {
            Ok(updated) => config = updated,
            Err(e) => {
                eprintln!("error: invalid --define: {define} ({e})");
                return ExitCode::FAILURE;
            }
        }
    }

    // Combine CLI --transpose with settings.transpose from config.
    // CLI flag takes additive precedence: final = config + CLI.
    let config_transpose_f64 = config
        .get_path("settings.transpose")
        .as_f64()
        .unwrap_or(0.0);
    let config_transpose =
        if config_transpose_f64 < f64::from(i8::MIN) || config_transpose_f64 > f64::from(i8::MAX) {
            let clamped = config_transpose_f64.clamp(f64::from(i8::MIN), f64::from(i8::MAX)) as i8;
            eprintln!(
                "warning: settings.transpose value {} is out of i8 range, clamped to {}",
                config_transpose_f64, clamped
            );
            clamped
        } else {
            config_transpose_f64 as i8
        };
    let (effective_transpose, saturated) =
        chordpro_core::transpose::combine_transpose(config_transpose, cli.transpose);
    if saturated {
        eprintln!(
            "warning: transpose offset {} + {} exceeds i8 range, clamped to {}",
            config_transpose, cli.transpose, effective_transpose
        );
    }

    let mut all_songs: Vec<chordpro_core::ast::Song> = Vec::new();
    let mut had_error = false;

    for path in &cli.files {
        let input = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("error: {path}: {e}");
                had_error = true;
                continue;
            }
        };

        let result = chordpro_core::parse_multi_lenient(&input);
        for parse_result in &result.results {
            for e in &parse_result.errors {
                eprintln!(
                    "error: {path}: parse error at line {} column {}: {}",
                    e.line(),
                    e.column(),
                    e.message
                );
                had_error = true;
            }
        }
        all_songs.extend(result.results.into_iter().map(|r| r.song));
    }

    /// Output produced by a renderer.
    enum Output {
        Text(String),
        Binary(Vec<u8>),
    }

    let output = match cli.format {
        Format::Pdf => {
            let result = chordpro_render_pdf::render_songs_with_warnings(
                &all_songs,
                effective_transpose,
                &config,
            );
            for w in &result.warnings {
                eprintln!("warning: {w}");
            }
            Output::Binary(result.output)
        }
        Format::Text => {
            let result = chordpro_render_text::render_songs_with_warnings(
                &all_songs,
                effective_transpose,
                &config,
            );
            for w in &result.warnings {
                eprintln!("warning: {w}");
            }
            Output::Text(result.output)
        }
        Format::Html => {
            let result = chordpro_render_html::render_songs_with_warnings(
                &all_songs,
                effective_transpose,
                &config,
            );
            for w in &result.warnings {
                eprintln!("warning: {w}");
            }
            Output::Text(result.output)
        }
    };

    let is_empty = match &output {
        Output::Text(s) => s.is_empty(),
        Output::Binary(b) => b.is_empty(),
    };
    if had_error && is_empty {
        return ExitCode::FAILURE;
    }

    let write_result = match &output {
        Output::Binary(bytes) => write_bytes(&cli.output, bytes),
        Output::Text(text) => write_text(&cli.output, text),
    };

    if let Err(e) = write_result {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }

    if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Write text output to a file or stdout.
fn write_text(path: &Option<String>, content: &str) -> io::Result<()> {
    match path {
        Some(path) => fs::write(path, content),
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(content.as_bytes())
        }
    }
}

/// Write binary output to a file or stdout.
fn write_bytes(path: &Option<String>, content: &[u8]) -> io::Result<()> {
    match path {
        Some(path) => fs::write(path, content),
        None => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(content)
        }
    }
}
